//! Nodes: the summary `kubectl get nodes -o wide` prints, the details `kubectl describe node`
//! shows, and live usage from metrics-server (`kubectl top nodes`) when the cluster has it.

use std::collections::BTreeMap;

use http::header::ACCEPT;
use k8s_openapi::api::core::v1::Node;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use serde::{Deserialize, Serialize};

use super::Clusters;
use super::resources::{created, name, rfc3339};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSummary {
    pub name: String,
    /// As `kubectl` prints it: `Ready`, `NotReady` or `Unknown`, plus `,SchedulingDisabled` when
    /// cordoned.
    pub status: String,
    pub roles: Vec<String>,
    /// Kubelet version.
    pub version: String,
    pub internal_ip: Option<String>,
    pub external_ip: Option<String>,
    pub os_image: String,
    pub kernel: String,
    pub arch: String,
    pub runtime: String,
    /// Allocatable CPU in millicores.
    pub cpu_millis: Option<u64>,
    /// Allocatable memory in bytes.
    pub memory_bytes: Option<u64>,
    /// How many pods the node accepts.
    pub pods: Option<u64>,
    pub taints: usize,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    #[serde(flatten)]
    pub summary: NodeSummary,
    pub addresses: Vec<NodeAddress>,
    pub conditions: Vec<NodeCondition>,
    pub taints: Vec<NodeTaint>,
    pub labels: BTreeMap<String, String>,
    pub capacity: BTreeMap<String, String>,
    pub allocatable: BTreeMap<String, String>,
    pub pod_cidrs: Vec<String>,
    pub provider_id: Option<String>,
    pub kube_proxy: String,
    pub images: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeAddress {
    #[serde(rename = "type")]
    pub type_: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCondition {
    #[serde(rename = "type")]
    pub type_: String,
    /// `True`, `False` or `Unknown`.
    pub status: String,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub last_transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTaint {
    pub key: String,
    pub value: Option<String>,
    pub effect: String,
}

/// Current usage from metrics-server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeUsage {
    pub name: String,
    pub cpu_millis: u64,
    pub memory_bytes: u64,
}

impl From<Node> for NodeSummary {
    fn from(node: Node) -> Self {
        summary(&node)
    }
}

fn summary(node: &Node) -> NodeSummary {
    let spec = node.spec.as_ref();
    let status = node.status.as_ref();
    let info = status.and_then(|s| s.node_info.as_ref());
    let address = |kind: &str| {
        status
            .and_then(|s| s.addresses.as_ref())
            .and_then(|list| list.iter().find(|a| a.type_ == kind))
            .map(|a| a.address.clone())
    };
    let allocatable = status.and_then(|s| s.allocatable.as_ref());
    let quantity = |key: &str| allocatable.and_then(|a| a.get(key));
    NodeSummary {
        name: name(&node.metadata),
        status: node_status(node),
        roles: roles(node.metadata.labels.as_ref()),
        version: info.map(|i| i.kubelet_version.clone()).unwrap_or_default(),
        internal_ip: address("InternalIP"),
        external_ip: address("ExternalIP"),
        os_image: info.map(|i| i.os_image.clone()).unwrap_or_default(),
        kernel: info.map(|i| i.kernel_version.clone()).unwrap_or_default(),
        arch: info.map(|i| i.architecture.clone()).unwrap_or_default(),
        runtime: info
            .map(|i| i.container_runtime_version.clone())
            .unwrap_or_default(),
        cpu_millis: quantity("cpu")
            .and_then(|q| parse_quantity(&q.0))
            .map(|v| (v * 1000.0).round() as u64),
        memory_bytes: quantity("memory")
            .and_then(|q| parse_quantity(&q.0))
            .map(|v| v.round() as u64),
        pods: quantity("pods")
            .and_then(|q| parse_quantity(&q.0))
            .map(|v| v.round() as u64),
        taints: spec.and_then(|s| s.taints.as_ref()).map_or(0, Vec::len),
        created: created(&node.metadata),
    }
}

impl From<Node> for NodeDetail {
    fn from(node: Node) -> Self {
        let summary = summary(&node);
        let status = node.status.unwrap_or_default();
        let spec = node.spec.unwrap_or_default();
        let quantities = |map: Option<BTreeMap<String, Quantity>>| {
            map.unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, v.0))
                .collect()
        };
        Self {
            summary,
            addresses: status
                .addresses
                .unwrap_or_default()
                .into_iter()
                .map(|a| NodeAddress {
                    type_: a.type_,
                    address: a.address,
                })
                .collect(),
            conditions: status
                .conditions
                .unwrap_or_default()
                .into_iter()
                .map(|c| NodeCondition {
                    type_: c.type_,
                    status: c.status,
                    reason: c.reason,
                    message: c.message,
                    last_transition: c.last_transition_time.as_ref().map(rfc3339),
                })
                .collect(),
            taints: spec
                .taints
                .unwrap_or_default()
                .into_iter()
                .map(|t| NodeTaint {
                    key: t.key,
                    value: t.value,
                    effect: t.effect,
                })
                .collect(),
            labels: node.metadata.labels.unwrap_or_default(),
            capacity: quantities(status.capacity),
            allocatable: quantities(status.allocatable),
            pod_cidrs: spec.pod_cidrs.unwrap_or_default(),
            provider_id: spec.provider_id,
            kube_proxy: status
                .node_info
                .map(|i| i.kube_proxy_version)
                .unwrap_or_default(),
            images: status.images.map_or(0, |i| i.len()),
        }
    }
}

/// Mirrors `kubectl get nodes` STATUS.
fn node_status(node: &Node) -> String {
    let ready = node
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|c| c.iter().find(|c| c.type_ == "Ready"));
    let mut status = match ready.map(|c| c.status.as_str()) {
        Some("True") => "Ready".to_owned(),
        Some("False") => "NotReady".to_owned(),
        _ => "Unknown".to_owned(),
    };
    if node.spec.as_ref().and_then(|s| s.unschedulable) == Some(true) {
        status.push_str(",SchedulingDisabled");
    }
    status
}

/// Roles from `node-role.kubernetes.io/<role>` and `kubernetes.io/role=<role>` labels, as
/// `kubectl` reads them.
fn roles(labels: Option<&BTreeMap<String, String>>) -> Vec<String> {
    let mut roles: Vec<String> = labels
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            if let Some(role) = key.strip_prefix("node-role.kubernetes.io/") {
                Some(role.to_owned())
            } else if key == "kubernetes.io/role" {
                Some(value.clone())
            } else {
                None
            }
        })
        .filter(|r| !r.is_empty())
        .collect();
    roles.sort();
    roles.dedup();
    roles
}

/// Parses a Kubernetes quantity (`250m`, `2`, `1536Mi`, `4Gi`, `123456n`, `1e3`) into a plain
/// number: cores for CPU, bytes for memory.
pub(super) fn parse_quantity(text: &str) -> Option<f64> {
    let text = text.trim();
    if let Ok(value) = text.parse::<f64>() {
        return Some(value);
    }
    let split = text.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, suffix) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let scale = match suffix {
        "n" => 1e-9,
        "u" => 1e-6,
        "m" => 1e-3,
        "k" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "P" => 1e15,
        "E" => 1e18,
        "Ki" => 1024f64,
        "Mi" => 1024f64.powi(2),
        "Gi" => 1024f64.powi(3),
        "Ti" => 1024f64.powi(4),
        "Pi" => 1024f64.powi(5),
        "Ei" => 1024f64.powi(6),
        _ => return None,
    };
    Some(number * scale)
}

const NODE_METRICS: &str = "/apis/metrics.k8s.io/v1beta1/nodes";

#[derive(Deserialize)]
struct MetricsList {
    #[serde(default)]
    items: Vec<NodeMetrics>,
}

#[derive(Deserialize)]
struct NodeMetrics {
    metadata: MetricsMeta,
    #[serde(default)]
    usage: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct MetricsMeta {
    name: String,
}

impl Clusters {
    /// CPU and memory each node uses now, like `kubectl top nodes`. `None` when the cluster has
    /// no metrics-server.
    pub async fn node_usage(&self, context: &str) -> Result<Option<Vec<NodeUsage>>> {
        let client = self.client(context).await?;
        let request = http::Request::get(NODE_METRICS)
            .header(ACCEPT, "application/json")
            .body(Vec::new())
            .map_err(|e| Error::Kube(e.to_string()))?;
        let list: MetricsList = match client.request(request).await {
            Ok(list) => list,
            // Not installed (404), or installed but not running (503).
            Err(kube::Error::Api(status)) if matches!(status.code, 404 | 503) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(Some(
            list.items
                .into_iter()
                .map(|m| {
                    let get = |key: &str| m.usage.get(key).and_then(|q| parse_quantity(q));
                    NodeUsage {
                        name: m.metadata.name,
                        cpu_millis: get("cpu").map_or(0, |v| (v * 1000.0).round() as u64),
                        memory_bytes: get("memory").map_or(0, |v| v.round() as u64),
                    }
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quantities() {
        let q = |s| parse_quantity(s).unwrap();
        assert_eq!(q("2"), 2.0);
        assert_eq!(q("250m"), 0.25);
        assert_eq!(q("1536Mi"), 1536.0 * 1024.0 * 1024.0);
        assert_eq!(q("4Gi"), 4.0 * 1024f64.powi(3));
        assert_eq!(q("8k"), 8000.0);
        assert_eq!(q("1e3"), 1000.0);
        assert!((q("270123456n") - 0.270123456).abs() < 1e-12);
        assert_eq!(parse_quantity("12Q"), None);
    }

    #[test]
    fn summarizes_a_node_like_kubectl() {
        let node: Node = serde_json::from_value(serde_json::json!({
            "metadata": {
                "name": "n1",
                "creationTimestamp": "2026-01-02T03:04:05Z",
                "labels": {
                    "node-role.kubernetes.io/control-plane": "",
                    "node-role.kubernetes.io/master": "true",
                    "kubernetes.io/role": "worker"
                }
            },
            "spec": { "unschedulable": true, "taints": [{ "key": "a", "effect": "NoSchedule" }] },
            "status": {
                "conditions": [{ "type": "Ready", "status": "True" }],
                "addresses": [{ "type": "InternalIP", "address": "10.0.0.1" }],
                "allocatable": { "cpu": "3500m", "memory": "8Gi", "pods": "110" },
                "nodeInfo": {
                    "kubeletVersion": "v1.31.0", "osImage": "Ubuntu", "kernelVersion": "6.1",
                    "architecture": "arm64", "containerRuntimeVersion": "containerd://2",
                    "kubeProxyVersion": "", "machineID": "", "systemUUID": "", "bootID": "",
                    "operatingSystem": "linux"
                }
            }
        }))
        .unwrap();
        let s = NodeSummary::from(node.clone());
        assert_eq!(s.status, "Ready,SchedulingDisabled");
        assert_eq!(s.roles, ["control-plane", "master", "worker"]);
        assert_eq!(s.internal_ip.as_deref(), Some("10.0.0.1"));
        assert_eq!((s.cpu_millis, s.pods), (Some(3500), Some(110)));
        assert_eq!(s.memory_bytes, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(s.taints, 1);

        let d = NodeDetail::from(node);
        assert_eq!(d.conditions[0].type_, "Ready");
        assert_eq!(d.taints[0].effect, "NoSchedule");
        assert_eq!(d.allocatable["cpu"], "3500m");
    }

    #[test]
    fn unknown_without_ready_condition() {
        let node: Node =
            serde_json::from_value(serde_json::json!({ "metadata": { "name": "x" } })).unwrap();
        assert_eq!(NodeSummary::from(node).status, "Unknown");
    }
}
