//! Lists any resource kind as the table `kubectl get` prints, using the API server's
//! server-side printing (`Accept: application/json;as=Table`). This gives the right columns for
//! every kind, including custom resources with their printer columns.

use std::ops::ControlFlow;

use futures::{FutureExt, StreamExt};
use http::header::ACCEPT;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::watch::{self, Change, Listed};
use super::{Clusters, ResourceKind, StreamId};
use crate::{Error, Result};

const TABLE: &str = "application/json;as=Table;v=v1;g=meta.k8s.io,application/json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub name: String,
    /// OpenAPI type (`string`, `integer`, `date`, ...).
    #[serde(rename = "type")]
    pub type_: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    pub namespace: Option<String>,
    pub name: String,
    /// One per column, already rendered as text.
    pub cells: Vec<String>,
}

/// One page of rows. Columns are repeated on every page, so any page can be rendered alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePage {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
}

// The subset of meta.k8s.io/v1 Table that we read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Clone)]
struct Table {
    /// Null in watch events after the first.
    #[serde(default)]
    column_definitions: Option<Vec<ColumnDefinition>>,
    #[serde(default)]
    rows: Vec<Row>,
    #[serde(default)]
    metadata: ListMeta,
}

#[derive(Deserialize, Clone)]
struct ColumnDefinition {
    name: String,
    #[serde(rename = "type", default)]
    type_: String,
    #[serde(default)]
    format: String,
    #[serde(default)]
    priority: i32,
}

#[derive(Deserialize, Clone)]
struct Row {
    cells: Vec<Value>,
    #[serde(default)]
    object: Option<RowObject>,
}

#[derive(Deserialize, Default, Clone)]
struct RowObject {
    #[serde(default)]
    metadata: ObjectMeta,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct ObjectMeta {
    #[serde(default)]
    name: String,
    namespace: Option<String>,
    resource_version: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct ListMeta {
    #[serde(rename = "continue")]
    continue_: Option<String>,
    resource_version: Option<String>,
}

impl Clusters {
    /// Lists `kind` page by page as `kubectl get` would show it (default columns only).
    /// `namespace` is ignored for cluster-scoped kinds; `None` lists all namespaces.
    pub async fn table(
        &self,
        context: &str,
        kind: &ResourceKind,
        namespace: Option<&str>,
        mut on_page: impl FnMut(TablePage) -> ControlFlow<()>,
    ) -> Result<Listed> {
        let client = self.client(context).await?;
        let path = kind.path(namespace);
        let mut token: Option<String> = None;
        let mut listed = Listed {
            total: 0,
            resource_version: String::new(),
            complete: false,
        };
        loop {
            let mut url = format!("{path}?limit={}&includeObject=Metadata", self.page_size);
            if let Some(t) = &token {
                url.push_str("&continue=");
                url.push_str(&encode(t));
            }
            let table: Table = client.request(table_request(&url)?).await?;
            if listed.resource_version.is_empty() {
                listed.resource_version = table.metadata.resource_version.unwrap_or_default();
            }
            let columns = table.column_definitions.unwrap_or_default();
            let shown = shown(&columns);
            let page = TablePage {
                columns: shown
                    .iter()
                    .map(|&i| TableColumn {
                        name: columns[i].name.clone(),
                        type_: columns[i].type_.clone(),
                        format: columns[i].format.clone(),
                    })
                    .collect(),
                rows: to_rows(&shown, table.rows),
            };
            listed.total += page.rows.len();
            if on_page(page).is_break() {
                return Ok(listed);
            }
            match table.metadata.continue_.filter(|t| !t.is_empty()) {
                Some(next) => token = Some(next),
                None => {
                    listed.complete = true;
                    return Ok(listed);
                }
            }
        }
    }

    /// Follows changes to what [`Self::table`] returned, with rows in the same columns.
    pub async fn watch_table(
        &self,
        context: &str,
        kind: &ResourceKind,
        namespace: Option<&str>,
        resource_version: String,
        mut on_change: impl FnMut(Change<TableRow>) -> ControlFlow<()> + Send + 'static,
    ) -> Result<StreamId> {
        let client = self.connection(context).await?.watch_client;
        let path = kind.path(namespace);
        let open: watch::Open<Table> = Box::new(move |rv| {
            let client = client.clone();
            let url = format!(
                "{path}?watch=1&allowWatchBookmarks=true&includeObject=Metadata&resourceVersion={}",
                encode(&rv)
            );
            async move {
                let request =
                    table_request(&url).map_err(|e| kube::Error::Service(e.to_string().into()))?;
                Ok(client.request_events::<Table>(request).await?.boxed())
            }
            .boxed()
        });
        // Only the first event of each watch says which columns there are; later ones reuse it.
        let mut columns: Vec<usize> = Vec::new();
        let id = self.streams.next_id();
        self.streams.spawn(
            id,
            watch::follow(
                open,
                resource_version,
                |t: &Table| {
                    t.metadata.resource_version.clone().or_else(|| {
                        t.rows
                            .last()?
                            .object
                            .as_ref()?
                            .metadata
                            .resource_version
                            .clone()
                    })
                },
                move |event| {
                    let (table, deleted) = match event {
                        watch::Event::Applied(t) => (t, false),
                        watch::Event::Deleted(t) => (t, true),
                        watch::Event::Reset => return on_change(Change::Reset),
                    };
                    if let Some(defs) = &table.column_definitions {
                        columns = shown(defs);
                    }
                    for row in to_rows(&columns, table.rows) {
                        let change = if deleted {
                            Change::Deleted(row)
                        } else {
                            Change::Applied(row)
                        };
                        on_change(change)?;
                    }
                    ControlFlow::Continue(())
                },
            ),
        );
        Ok(id)
    }
}

fn table_request(url: &str) -> Result<http::Request<Vec<u8>>> {
    http::Request::get(url)
        .header(ACCEPT, TABLE)
        .body(Vec::new())
        .map_err(|e| Error::Kube(e.to_string()))
}

/// Indexes of the columns `kubectl get` shows by default (priority 0).
fn shown(columns: &[ColumnDefinition]) -> Vec<usize> {
    (0..columns.len())
        .filter(|&i| columns[i].priority == 0)
        .collect()
}

fn to_rows(shown: &[usize], rows: Vec<Row>) -> Vec<TableRow> {
    rows.into_iter()
        .map(|row| {
            let meta = row.object.unwrap_or_default().metadata;
            TableRow {
                cells: shown.iter().map(|&i| cell_text(row.cells.get(i))).collect(),
                namespace: meta.namespace,
                name: meta.name,
            }
        })
        .collect()
}

fn cell_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| cell_text(Some(v)))
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
    }
}

/// Percent-encodes a continue token for a query string.
fn encode(token: &str) -> String {
    token
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_default_columns_and_renders_cells() {
        let table: Table = serde_json::from_value(serde_json::json!({
            "columnDefinitions": [
                { "name": "Name", "type": "string", "format": "name", "priority": 0 },
                { "name": "Ready", "type": "string", "priority": 0 },
                { "name": "Images", "type": "string", "priority": 1 },
                { "name": "Age", "type": "string", "priority": 0 }
            ],
            "rows": [{
                "cells": ["web", "2/3", "nginx", "5d"],
                "object": { "metadata": { "name": "web", "namespace": "shop" } }
            }],
            "metadata": { "continue": "" }
        }))
        .unwrap();
        let columns = shown(table.column_definitions.as_deref().unwrap());
        assert_eq!(columns, [0, 1, 3]);
        let rows = to_rows(&columns, table.rows);
        assert_eq!(rows[0].cells, ["web", "2/3", "5d"]);
        assert_eq!(rows[0].namespace.as_deref(), Some("shop"));
    }

    #[test]
    fn reads_watch_events_without_columns() {
        let event: kube::core::WatchEvent<Table> = serde_json::from_value(serde_json::json!({
            "type": "DELETED",
            "object": {
                "kind": "Table", "apiVersion": "meta.k8s.io/v1",
                "metadata": { "resourceVersion": "102098" },
                "columnDefinitions": null,
                "rows": [{ "cells": ["probe", 1, "0s"], "object": { "metadata": {
                    "name": "probe", "namespace": "default", "resourceVersion": "102098"
                } } }]
            }
        }))
        .unwrap();
        let kube::core::WatchEvent::Deleted(table) = event else {
            panic!("expected a deleted event");
        };
        assert!(table.column_definitions.is_none());
        assert_eq!(table.metadata.resource_version.as_deref(), Some("102098"));
        assert_eq!(to_rows(&[0, 1], table.rows)[0].cells, ["probe", "1"]);
    }

    #[test]
    fn cell_values() {
        assert_eq!(cell_text(Some(&serde_json::json!(3))), "3");
        assert_eq!(cell_text(Some(&serde_json::json!(["a", "b"]))), "a, b");
        assert_eq!(cell_text(Some(&Value::Null)), "");
        assert_eq!(cell_text(Some(&serde_json::json!(true))), "true");
    }

    #[test]
    fn encodes_tokens() {
        assert_eq!(encode("eyJ2Ijoi+/=="), "eyJ2Ijoi%2B%2F%3D%3D");
    }
}
