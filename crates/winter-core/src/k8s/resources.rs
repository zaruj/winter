//! Compact, GUI-friendly views of Kubernetes objects. Columns follow `kubectl get`.

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{
    ConfigMap, ContainerStatus, Namespace, PersistentVolumeClaim, Pod, Secret, Service,
};
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceSummary {
    pub name: String,
    pub status: String,
    /// RFC 3339 creation time.
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentSummary {
    pub namespace: String,
    pub name: String,
    pub desired: i32,
    pub ready: i32,
    pub up_to_date: i32,
    pub available: i32,
    pub images: Vec<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSummary {
    pub namespace: String,
    pub name: String,
    pub ready: usize,
    pub containers: usize,
    /// What `kubectl get pods` prints in STATUS, e.g. `Running`, `CrashLoopBackOff`, `Init:0/1`.
    pub status: String,
    pub restarts: i32,
    pub node: Option<String>,
    pub ip: Option<String>,
    pub created: Option<String>,
    /// Container names, for picking whose logs to show or where to open a shell.
    pub container_names: Vec<String>,
    /// From the `kubectl.kubernetes.io/default-container` annotation, else the first container.
    pub default_container: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    pub namespace: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub cluster_ip: Option<String>,
    pub external: Vec<String>,
    /// Like `kubectl`: `80/TCP`, or `80:30080/TCP` with a node port.
    pub ports: Vec<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngressSummary {
    pub namespace: String,
    pub name: String,
    pub class: Option<String>,
    pub hosts: Vec<String>,
    pub addresses: Vec<String>,
    pub tls: bool,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvcSummary {
    pub namespace: String,
    pub name: String,
    pub status: String,
    pub volume: Option<String>,
    pub capacity: Option<String>,
    /// Short forms as `kubectl` prints them: RWO, ROX, RWX, RWOP.
    pub access_modes: Vec<String>,
    pub storage_class: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapSummary {
    pub namespace: String,
    pub name: String,
    pub keys: usize,
    pub created: Option<String>,
}

/// Name, type and key count only. Secret values never leave this crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretSummary {
    pub namespace: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub keys: usize,
    pub created: Option<String>,
}

pub(super) fn name(meta: &ObjectMeta) -> String {
    meta.name.clone().unwrap_or_default()
}

fn namespace(meta: &ObjectMeta) -> String {
    meta.namespace.clone().unwrap_or_default()
}

pub(super) fn created(meta: &ObjectMeta) -> Option<String> {
    meta.creation_timestamp.as_ref().map(rfc3339)
}

pub(super) fn rfc3339(time: &Time) -> String {
    time.0.to_string()
}

impl From<Namespace> for NamespaceSummary {
    fn from(ns: Namespace) -> Self {
        Self {
            name: name(&ns.metadata),
            status: ns
                .status
                .and_then(|s| s.phase)
                .unwrap_or_else(|| "Unknown".into()),
            created: created(&ns.metadata),
        }
    }
}

impl From<Deployment> for DeploymentSummary {
    fn from(deploy: Deployment) -> Self {
        let spec = &deploy.spec;
        let status = deploy.status.as_ref();
        Self {
            namespace: namespace(&deploy.metadata),
            name: name(&deploy.metadata),
            desired: spec.replicas.unwrap_or(1),
            ready: status.and_then(|s| s.ready_replicas).unwrap_or(0),
            up_to_date: status.and_then(|s| s.updated_replicas).unwrap_or(0),
            available: status.and_then(|s| s.available_replicas).unwrap_or(0),
            images: spec
                .template
                .spec
                .as_ref()
                .map(|pod| {
                    pod.containers
                        .iter()
                        .filter_map(|c| c.image.clone())
                        .collect()
                })
                .unwrap_or_default(),
            created: created(&deploy.metadata),
        }
    }
}

impl From<Pod> for PodSummary {
    fn from(pod: Pod) -> Self {
        let containers = pod.spec.as_ref().map_or(0, |s| s.containers.len());
        let statuses = pod
            .status
            .as_ref()
            .and_then(|s| s.container_statuses.as_deref())
            .unwrap_or_default();
        let container_names: Vec<String> = pod
            .spec
            .as_ref()
            .map(|s| s.containers.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        let default_container = pod
            .metadata
            .annotations
            .as_ref()
            .and_then(|a| a.get("kubectl.kubernetes.io/default-container"))
            .filter(|name| container_names.contains(name))
            .or(container_names.first())
            .cloned();
        Self {
            namespace: namespace(&pod.metadata),
            name: name(&pod.metadata),
            ready: statuses.iter().filter(|c| c.ready).count(),
            containers,
            status: pod_status(&pod),
            restarts: statuses.iter().map(|c| c.restart_count).sum(),
            node: pod.spec.as_ref().and_then(|s| s.node_name.clone()),
            ip: pod.status.as_ref().and_then(|s| s.pod_ip.clone()),
            created: created(&pod.metadata),
            container_names,
            default_container,
        }
    }
}

impl From<Service> for ServiceSummary {
    fn from(svc: Service) -> Self {
        let spec = svc.spec.as_ref();
        let mut external: Vec<String> = svc
            .status
            .as_ref()
            .and_then(|s| s.load_balancer.as_ref())
            .and_then(|lb| lb.ingress.as_ref())
            .map(|ingress| {
                ingress
                    .iter()
                    .filter_map(|i| i.ip.clone().or_else(|| i.hostname.clone()))
                    .collect()
            })
            .unwrap_or_default();
        external.extend(
            spec.and_then(|s| s.external_ips.clone())
                .unwrap_or_default(),
        );
        if let Some(name) = spec.and_then(|s| s.external_name.clone()) {
            external.push(name);
        }
        let ports = spec
            .and_then(|s| s.ports.as_ref())
            .map(|ports| {
                ports
                    .iter()
                    .map(|p| {
                        let protocol = p.protocol.as_deref().unwrap_or("TCP");
                        match p.node_port {
                            Some(node) => format!("{}:{node}/{protocol}", p.port),
                            None => format!("{}/{protocol}", p.port),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            namespace: namespace(&svc.metadata),
            name: name(&svc.metadata),
            type_: spec
                .and_then(|s| s.type_.clone())
                .unwrap_or_else(|| "ClusterIP".into()),
            cluster_ip: spec.and_then(|s| s.cluster_ip.clone()),
            external,
            ports,
            created: created(&svc.metadata),
        }
    }
}

impl From<Ingress> for IngressSummary {
    fn from(ing: Ingress) -> Self {
        let spec = ing.spec.as_ref();
        Self {
            namespace: namespace(&ing.metadata),
            name: name(&ing.metadata),
            class: spec.and_then(|s| s.ingress_class_name.clone()),
            hosts: spec
                .and_then(|s| s.rules.as_ref())
                .map(|rules| rules.iter().filter_map(|r| r.host.clone()).collect())
                .unwrap_or_default(),
            addresses: ing
                .status
                .as_ref()
                .and_then(|s| s.load_balancer.as_ref())
                .and_then(|lb| lb.ingress.as_ref())
                .map(|ingress| {
                    ingress
                        .iter()
                        .filter_map(|i| i.ip.clone().or_else(|| i.hostname.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            tls: spec
                .and_then(|s| s.tls.as_ref())
                .is_some_and(|t| !t.is_empty()),
            created: created(&ing.metadata),
        }
    }
}

impl From<PersistentVolumeClaim> for PvcSummary {
    fn from(pvc: PersistentVolumeClaim) -> Self {
        let spec = pvc.spec.as_ref();
        let status = pvc.status.as_ref();
        Self {
            namespace: namespace(&pvc.metadata),
            name: name(&pvc.metadata),
            status: status
                .and_then(|s| s.phase.clone())
                .unwrap_or_else(|| "Unknown".into()),
            volume: spec.and_then(|s| s.volume_name.clone()),
            capacity: status
                .and_then(|s| s.capacity.as_ref())
                .and_then(|c| c.get("storage"))
                .map(|q| q.0.clone()),
            access_modes: status
                .and_then(|s| s.access_modes.as_ref())
                .map(|modes| modes.iter().map(|m| short_access_mode(m)).collect())
                .unwrap_or_default(),
            storage_class: spec.and_then(|s| s.storage_class_name.clone()),
            created: created(&pvc.metadata),
        }
    }
}

fn short_access_mode(mode: &str) -> String {
    match mode {
        "ReadWriteOnce" => "RWO",
        "ReadOnlyMany" => "ROX",
        "ReadWriteMany" => "RWX",
        "ReadWriteOncePod" => "RWOP",
        other => other,
    }
    .into()
}

impl From<ConfigMap> for ConfigMapSummary {
    fn from(cm: ConfigMap) -> Self {
        Self {
            namespace: namespace(&cm.metadata),
            name: name(&cm.metadata),
            keys: cm.data.map_or(0, |d| d.len()) + cm.binary_data.map_or(0, |d| d.len()),
            created: created(&cm.metadata),
        }
    }
}

impl From<Secret> for SecretSummary {
    fn from(secret: Secret) -> Self {
        Self {
            namespace: namespace(&secret.metadata),
            name: name(&secret.metadata),
            type_: secret.type_.unwrap_or_else(|| "Opaque".into()),
            keys: secret.data.map_or(0, |d| d.len()) + secret.string_data.map_or(0, |d| d.len()),
            created: created(&secret.metadata),
        }
    }
}

/// Reproduces the STATUS column of `kubectl get pods` (kubectl's `printPod`), minus rarely-seen
/// cases such as sidecar restart policies and readiness gates.
fn pod_status(pod: &Pod) -> String {
    let Some(status) = pod.status.as_ref() else {
        return "Unknown".into();
    };
    let mut reason = status
        .reason
        .clone()
        .or_else(|| status.phase.clone())
        .unwrap_or_else(|| "Unknown".into());

    let init = status
        .init_container_statuses
        .as_deref()
        .unwrap_or_default();
    let init_total = pod
        .spec
        .as_ref()
        .and_then(|s| s.init_containers.as_ref())
        .map_or(init.len(), Vec::len);
    let mut initializing = false;
    for (i, c) in init.iter().enumerate() {
        let state = c.state.as_ref();
        if let Some(t) = state.and_then(|s| s.terminated.as_ref()) {
            if t.exit_code == 0 {
                continue;
            }
            reason = match &t.reason {
                Some(r) if !r.is_empty() => format!("Init:{r}"),
                _ if t.signal.unwrap_or(0) != 0 => format!("Init:Signal:{}", t.signal.unwrap_or(0)),
                _ => format!("Init:ExitCode:{}", t.exit_code),
            };
        } else if let Some(r) = state
            .and_then(|s| s.waiting.as_ref())
            .and_then(|w| w.reason.as_deref())
            .filter(|r| !r.is_empty() && *r != "PodInitializing")
        {
            reason = format!("Init:{r}");
        } else {
            reason = format!("Init:{i}/{init_total}");
        }
        initializing = true;
        break;
    }

    if !initializing {
        let mut has_running = false;
        let statuses: &[ContainerStatus] = status.container_statuses.as_deref().unwrap_or_default();
        for c in statuses.iter().rev() {
            let state = c.state.as_ref();
            if let Some(r) = state
                .and_then(|s| s.waiting.as_ref())
                .and_then(|w| w.reason.as_deref())
                .filter(|r| !r.is_empty())
            {
                reason = r.to_owned();
            } else if let Some(t) = state.and_then(|s| s.terminated.as_ref()) {
                reason = match &t.reason {
                    Some(r) if !r.is_empty() => r.clone(),
                    _ if t.signal.unwrap_or(0) != 0 => format!("Signal:{}", t.signal.unwrap_or(0)),
                    _ => format!("ExitCode:{}", t.exit_code),
                };
            } else if c.ready && state.is_some_and(|s| s.running.is_some()) {
                has_running = true;
            }
        }
        if reason == "Completed" && has_running {
            let all_ready = pod
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .is_some_and(|cs| cs.iter().any(|c| c.type_ == "Ready" && c.status == "True"));
            reason = if all_ready { "Running" } else { "NotReady" }.into();
        }
    }

    if pod.metadata.deletion_timestamp.is_some() {
        reason = if status.reason.as_deref() == Some("NodeLost") {
            "Unknown".into()
        } else {
            "Terminating".into()
        };
    }
    reason
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod(json: serde_json::Value) -> Pod {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn running_pod() {
        let summary = PodSummary::from(pod(serde_json::json!({
            "metadata": { "name": "web-1", "namespace": "shop", "creationTimestamp": "2026-01-02T03:04:05Z" },
            "spec": { "nodeName": "n1", "containers": [{ "name": "a" }, { "name": "b" }] },
            "status": {
                "phase": "Running",
                "podIP": "10.0.0.5",
                "containerStatuses": [
                    { "name": "a", "ready": true, "restartCount": 2, "image": "", "imageID": "", "state": { "running": {} } },
                    { "name": "b", "ready": false, "restartCount": 1, "image": "", "imageID": "", "state": { "running": {} } }
                ]
            }
        })));
        assert_eq!(
            summary,
            PodSummary {
                namespace: "shop".into(),
                name: "web-1".into(),
                ready: 1,
                containers: 2,
                status: "Running".into(),
                restarts: 3,
                node: Some("n1".into()),
                ip: Some("10.0.0.5".into()),
                created: Some("2026-01-02T03:04:05Z".into()),
                container_names: vec!["a".into(), "b".into()],
                default_container: Some("a".into()),
            }
        );
    }

    #[test]
    fn crash_looping_container_wins() {
        let p = pod(serde_json::json!({
            "spec": { "containers": [{ "name": "a" }] },
            "status": { "phase": "Running", "containerStatuses": [
                { "name": "a", "ready": false, "restartCount": 7, "image": "", "imageID": "",
                  "state": { "waiting": { "reason": "CrashLoopBackOff" } } }
            ] }
        }));
        assert_eq!(pod_status(&p), "CrashLoopBackOff");
    }

    #[test]
    fn init_container_progress() {
        let p = pod(serde_json::json!({
            "spec": { "initContainers": [{ "name": "i1" }, { "name": "i2" }], "containers": [{ "name": "a" }] },
            "status": { "phase": "Pending", "initContainerStatuses": [
                { "name": "i1", "ready": true, "restartCount": 0, "image": "", "imageID": "",
                  "state": { "terminated": { "exitCode": 0 } } },
                { "name": "i2", "ready": false, "restartCount": 0, "image": "", "imageID": "",
                  "state": { "running": {} } }
            ] }
        }));
        assert_eq!(pod_status(&p), "Init:1/2");
    }

    #[test]
    fn completed_job_pod() {
        let p = pod(serde_json::json!({
            "spec": { "containers": [{ "name": "a" }] },
            "status": { "phase": "Succeeded", "containerStatuses": [
                { "name": "a", "ready": false, "restartCount": 0, "image": "", "imageID": "",
                  "state": { "terminated": { "exitCode": 0, "reason": "Completed" } } }
            ] }
        }));
        assert_eq!(pod_status(&p), "Completed");
    }

    #[test]
    fn deleting_pod_is_terminating() {
        let p = pod(serde_json::json!({
            "metadata": { "deletionTimestamp": "2026-01-02T03:04:05Z" },
            "spec": { "containers": [{ "name": "a" }] },
            "status": { "phase": "Running" }
        }));
        assert_eq!(pod_status(&p), "Terminating");
    }

    #[test]
    fn default_container_annotation() {
        let summary = PodSummary::from(pod(serde_json::json!({
            "metadata": { "annotations": { "kubectl.kubernetes.io/default-container": "app" } },
            "spec": { "containers": [{ "name": "istio-proxy" }, { "name": "app" }] }
        })));
        assert_eq!(summary.default_container.as_deref(), Some("app"));
    }

    #[test]
    fn service_ports_and_external_addresses() {
        let svc: Service = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "web", "namespace": "shop" },
            "spec": { "type": "LoadBalancer", "clusterIP": "10.0.0.1",
                      "ports": [{ "port": 80, "nodePort": 30080 }, { "port": 53, "protocol": "UDP" }] },
            "status": { "loadBalancer": { "ingress": [{ "ip": "1.2.3.4" }, { "hostname": "lb.example" }] } }
        }))
        .unwrap();
        let s = ServiceSummary::from(svc);
        assert_eq!(s.type_, "LoadBalancer");
        assert_eq!(s.ports, vec!["80:30080/TCP", "53/UDP"]);
        assert_eq!(s.external, vec!["1.2.3.4", "lb.example"]);
    }

    #[test]
    fn ingress_hosts_and_tls() {
        let ing: Ingress = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "web" },
            "spec": { "ingressClassName": "nginx", "tls": [{ "hosts": ["a.example"] }],
                      "rules": [{ "host": "a.example" }, { "host": "b.example" }] }
        }))
        .unwrap();
        let s = IngressSummary::from(ing);
        assert_eq!(s.class.as_deref(), Some("nginx"));
        assert_eq!(s.hosts, vec!["a.example", "b.example"]);
        assert!(s.tls);
    }

    #[test]
    fn pvc_capacity_and_modes() {
        let pvc: PersistentVolumeClaim = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "data" },
            "spec": { "volumeName": "pv-1", "storageClassName": "local-path" },
            "status": { "phase": "Bound", "capacity": { "storage": "10Gi" },
                        "accessModes": ["ReadWriteOnce", "ReadWriteMany"] }
        }))
        .unwrap();
        let s = PvcSummary::from(pvc);
        assert_eq!(
            (s.status.as_str(), s.capacity.as_deref(), s.access_modes),
            (
                "Bound",
                Some("10Gi"),
                vec!["RWO".to_string(), "RWX".to_string()]
            )
        );
    }

    #[test]
    fn secret_summary_has_no_values() {
        let secret: Secret = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "db" },
            "type": "kubernetes.io/basic-auth",
            "data": { "username": "YWRtaW4=", "password": "c2VjcmV0" }
        }))
        .unwrap();
        let s = SecretSummary::from(secret);
        assert_eq!((s.type_.as_str(), s.keys), ("kubernetes.io/basic-auth", 2));
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("YWRtaW4") && !json.contains("c2VjcmV0"));
    }

    #[test]
    fn deployment_counts_and_images() {
        let d: Deployment = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "api", "namespace": "shop" },
            "spec": {
                "replicas": 3,
                "selector": {},
                "template": { "spec": { "containers": [{ "name": "api", "image": "api:1.2" }] } }
            },
            "status": { "readyReplicas": 2, "updatedReplicas": 3, "availableReplicas": 2 }
        }))
        .unwrap();
        let s = DeploymentSummary::from(d);
        assert_eq!(
            (s.desired, s.ready, s.up_to_date, s.available),
            (3, 2, 3, 2)
        );
        assert_eq!(s.images, vec!["api:1.2".to_string()]);
    }
}
