//! Which resource kinds a cluster serves, with the names `kubectl` accepts for them
//! (plural, singular, short names such as `po` or `svc`), including custom resources.

use futures::future::join_all;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResourceList;
use serde::{Deserialize, Serialize};

use super::Clusters;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceKind {
    /// Empty for the core group.
    pub group: String,
    pub version: String,
    pub kind: String,
    pub plural: String,
    pub singular: String,
    pub short_names: Vec<String>,
    pub namespaced: bool,
}

impl ResourceKind {
    /// API path of the collection, optionally inside a namespace.
    pub(super) fn path(&self, namespace: Option<&str>) -> String {
        let base = if self.group.is_empty() {
            format!("/api/{}", self.version)
        } else {
            format!("/apis/{}/{}", self.group, self.version)
        };
        match namespace.filter(|_| self.namespaced) {
            Some(ns) => format!("{base}/namespaces/{ns}/{}", self.plural),
            None => format!("{base}/{}", self.plural),
        }
    }
}

impl Clusters {
    /// Every listable kind the cluster serves: core first, then each API group at its preferred
    /// version. Discovered once per connection.
    pub async fn resource_kinds(&self, context: &str) -> Result<Vec<ResourceKind>> {
        let connection = self.connection(context).await?;
        let client = connection.client.clone();
        connection
            .kinds
            .get_or_try_init(|| async move {
                let core = client.list_core_api_resources("v1").await?;
                let groups = client.list_api_groups().await?;
                let lists = join_all(groups.groups.iter().filter_map(|g| {
                    let version = g.preferred_version.as_ref().or(g.versions.first())?;
                    Some(client.list_api_group_resources(&version.group_version))
                }))
                .await;
                let mut kinds = kinds_of(&core);
                // A group that fails (e.g. an unavailable aggregated API) is skipped, like kubectl.
                for list in lists.into_iter().flatten() {
                    kinds.extend(kinds_of(&list));
                }
                Ok(kinds)
            })
            .await
            .cloned()
    }
}

fn kinds_of(list: &APIResourceList) -> Vec<ResourceKind> {
    let (group, version) = match list.group_version.split_once('/') {
        Some((g, v)) => (g.to_owned(), v.to_owned()),
        None => (String::new(), list.group_version.clone()),
    };
    list.resources
        .iter()
        // Subresources (pods/log) and kinds that cannot be listed are not browsable.
        .filter(|r| !r.name.contains('/') && r.verbs.iter().any(|v| v == "list"))
        .map(|r| ResourceKind {
            group: group.clone(),
            version: version.clone(),
            kind: r.kind.clone(),
            plural: r.name.clone(),
            singular: if r.singular_name.is_empty() {
                r.kind.to_lowercase()
            } else {
                r.singular_name.clone()
            },
            short_names: r.short_names.clone().unwrap_or_default(),
            namespaced: r.namespaced,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_resource_lists() {
        let list: APIResourceList = serde_json::from_value(serde_json::json!({
            "groupVersion": "apps/v1",
            "resources": [
                { "name": "deployments", "singularName": "deployment", "namespaced": true, "kind": "Deployment",
                  "verbs": ["get", "list", "watch"], "shortNames": ["deploy"] },
                { "name": "deployments/scale", "singularName": "", "namespaced": true, "kind": "Scale", "verbs": ["get"] },
                { "name": "tokenreviews", "singularName": "", "namespaced": false, "kind": "TokenReview", "verbs": ["create"] }
            ]
        }))
        .unwrap();
        let kinds = kinds_of(&list);
        assert_eq!(kinds.len(), 1);
        let d = &kinds[0];
        assert_eq!(
            (d.group.as_str(), d.version.as_str(), d.plural.as_str()),
            ("apps", "v1", "deployments")
        );
        assert_eq!(d.short_names, vec!["deploy"]);
        assert_eq!(
            d.path(Some("web")),
            "/apis/apps/v1/namespaces/web/deployments"
        );
    }

    #[test]
    fn core_and_cluster_scoped_paths() {
        let nodes = ResourceKind {
            group: String::new(),
            version: "v1".into(),
            kind: "Node".into(),
            plural: "nodes".into(),
            singular: "node".into(),
            short_names: vec!["no".into()],
            namespaced: false,
        };
        // A namespace is ignored for cluster-scoped kinds.
        assert_eq!(nodes.path(Some("web")), "/api/v1/nodes");
    }
}
