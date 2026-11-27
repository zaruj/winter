//! One object of any kind, as `kubectl get -o yaml` and the events part of `kubectl describe`
//! show it. The GUI builds its details view from the same object.

use serde::Serialize;
use serde_json::Value;

use super::{Clusters, ResourceKind};
use crate::{Error, Result};

/// Events shown for an object; the most recent first.
const MAX_EVENTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectView {
    /// The object as the API server returns it, without `metadata.managedFields`. Secret
    /// values are hidden unless asked for.
    pub object: Value,
    /// The same, as YAML.
    pub yaml: String,
    pub events: Vec<ObjectEvent>,
    /// Why events could not be read (e.g. no permission); the object itself was.
    pub events_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectEvent {
    /// `Normal` or `Warning`.
    #[serde(rename = "type")]
    pub type_: String,
    pub reason: String,
    pub message: String,
    pub count: u64,
    /// RFC 3339; when it last happened.
    pub last: Option<String>,
    pub source: Option<String>,
}

impl Clusters {
    /// Reads one object and its events. `reveal` shows Secret values (base64, as stored).
    pub async fn object(
        &self,
        context: &str,
        kind: &ResourceKind,
        namespace: Option<&str>,
        name: &str,
        reveal: bool,
    ) -> Result<ObjectView> {
        let name = path_segment(name)?;
        let namespace = namespace.map(path_segment).transpose()?;
        let client = self.client(context).await?;
        let url = format!("{}/{name}", kind.path(namespace));
        let mut object: Value = client.request(get(&url)?).await?;
        tidy(&mut object, is_secret(kind) && !reveal);

        let uid = object["metadata"]["uid"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let (events, events_error) = if uid.is_empty() {
            (Vec::new(), None)
        } else {
            // Events of a namespaced object live in its namespace; of a cluster-scoped one, in
            // any (usually `default`).
            let base = match namespace.filter(|_| kind.namespaced) {
                Some(ns) => format!("/api/v1/namespaces/{ns}/events"),
                None => "/api/v1/events".to_owned(),
            };
            let url = format!("{base}?fieldSelector=involvedObject.uid%3D{uid}");
            match client.request::<Value>(get(&url)?).await {
                Ok(list) => (events_of(&list), None),
                Err(e) => (Vec::new(), Some(Error::from(e).to_string())),
            }
        };
        let yaml = serde_yaml_ng::to_string(&object)
            .map_err(|e| Error::Kube(format!("cannot render YAML: {e}")))?;
        Ok(ObjectView {
            object,
            yaml,
            events,
            events_error,
        })
    }
}

fn get(url: &str) -> Result<http::Request<Vec<u8>>> {
    http::Request::get(url)
        .body(Vec::new())
        .map_err(|e| Error::Kube(e.to_string()))
}

/// Object and namespace names never contain these, so refusing them keeps the URL intact.
fn path_segment(s: &str) -> Result<&str> {
    let bad = s.is_empty()
        || s == "."
        || s == ".."
        || s.chars()
            .any(|c| matches!(c, '/' | '?' | '#' | '%') || c.is_whitespace() || c.is_control());
    if bad {
        Err(Error::Kube(format!("\"{s}\" is not a valid object name")))
    } else {
        Ok(s)
    }
}

fn is_secret(kind: &ResourceKind) -> bool {
    kind.group.is_empty() && kind.kind == "Secret"
}

/// Drops `managedFields` (bookkeeping, often longer than the object) and, for a Secret, replaces
/// each value with its size.
fn tidy(object: &mut Value, hide_secret_values: bool) {
    if let Some(meta) = object.get_mut("metadata").and_then(Value::as_object_mut) {
        meta.remove("managedFields");
    }
    if hide_secret_values {
        for field in ["data", "stringData"] {
            if let Some(values) = object.get_mut(field).and_then(Value::as_object_mut) {
                for value in values.values_mut() {
                    let text = value.as_str().unwrap_or_default();
                    let bytes = if field == "data" {
                        decoded_len(text)
                    } else {
                        text.len()
                    };
                    *value = Value::String(format!("<hidden: {bytes} bytes; press x to show>"));
                }
            }
        }
    }
}

/// Bytes a base64 string decodes to.
fn decoded_len(b64: &str) -> usize {
    let padding = b64.bytes().rev().take_while(|&b| b == b'=').count();
    (b64.len() / 4 * 3).saturating_sub(padding)
}

fn events_of(list: &Value) -> Vec<ObjectEvent> {
    let text = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let mut events: Vec<ObjectEvent> = list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| ObjectEvent {
            type_: text(&e["type"]).unwrap_or_else(|| "Normal".into()),
            reason: text(&e["reason"]).unwrap_or_default(),
            message: text(&e["message"]).unwrap_or_default().trim().to_owned(),
            count: e["series"]["count"]
                .as_u64()
                .or_else(|| e["count"].as_u64())
                .unwrap_or(1),
            last: text(&e["series"]["lastObservedTime"])
                .or_else(|| text(&e["lastTimestamp"]))
                .or_else(|| text(&e["eventTime"]))
                .or_else(|| text(&e["metadata"]["creationTimestamp"])),
            source: text(&e["reportingComponent"]).or_else(|| text(&e["source"]["component"])),
        })
        .collect();
    // RFC 3339 in UTC sorts as text.
    events.sort_by(|a, b| b.last.cmp(&a.last));
    events.truncate(MAX_EVENTS);
    events
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hides_secret_values_and_managed_fields() {
        let mut secret = json!({
            "kind": "Secret",
            "metadata": {"name": "db", "managedFields": [{"manager": "kubectl"}]},
            "data": {"password": "aHVudGVyMg=="},
            "stringData": {"user": "admin"}
        });
        tidy(&mut secret, true);
        assert!(secret["metadata"].get("managedFields").is_none());
        assert_eq!(
            secret["data"]["password"],
            "<hidden: 7 bytes; press x to show>"
        );
        assert_eq!(
            secret["stringData"]["user"],
            "<hidden: 5 bytes; press x to show>"
        );
        let yaml = serde_yaml_ng::to_string(&secret).unwrap();
        assert!(!yaml.contains("aHVudGVyMg") && !yaml.contains("admin"));
    }

    #[test]
    fn keeps_secret_values_when_asked() {
        let mut secret = json!({"data": {"password": "aHVudGVyMg=="}});
        tidy(&mut secret, false);
        assert_eq!(secret["data"]["password"], "aHVudGVyMg==");
    }

    #[test]
    fn events_newest_first_with_series_counts() {
        let list = json!({"items": [
            {"type": "Normal", "reason": "Pulled", "message": "pulled ", "count": 2,
             "lastTimestamp": "2026-01-01T00:00:00Z", "source": {"component": "kubelet"}},
            {"type": "Warning", "reason": "BackOff", "message": "restarting",
             "series": {"count": 9, "lastObservedTime": "2026-01-02T00:00:00Z"},
             "reportingComponent": "kubelet"}
        ]});
        let events = events_of(&list);
        assert_eq!(events[0].reason, "BackOff");
        assert_eq!((events[0].count, events[1].count), (9, 2));
        assert_eq!(events[1].message, "pulled");
    }

    #[test]
    fn names_cannot_change_the_path() {
        assert!(path_segment("web-7d9f").is_ok());
        for bad in ["", "..", "a/b", "a?watch=1", "a#b", "a b", "%2e"] {
            assert!(path_segment(bad).is_err(), "{bad}");
        }
        assert_eq!(decoded_len("aGk="), 2);
    }
}
