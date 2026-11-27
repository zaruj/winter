//! Kubernetes: kubeconfig contexts, cached connections, paged resource listing and watching.
//!
//! Lists are fetched page by page and handed to a callback as each page arrives, so a GUI can
//! show the first rows without waiting for the whole list.

mod discovery;
mod nodes;
mod object;
mod resources;
mod streams;
mod table;
mod watch;

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use kube::api::{ListParams, WatchParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};
use serde::Serialize;
use tokio::sync::{Mutex, OnceCell};

pub use discovery::ResourceKind;
pub use k8s_openapi::api::apps::v1::Deployment;
pub use k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, Node, PersistentVolumeClaim, Pod, Secret, Service,
};
pub use k8s_openapi::api::networking::v1::Ingress;
pub use nodes::{NodeAddress, NodeCondition, NodeDetail, NodeSummary, NodeTaint, NodeUsage};
pub use object::{ObjectEvent, ObjectView};
pub use resources::{
    ConfigMapSummary, DeploymentSummary, IngressSummary, NamespaceSummary, PodSummary, PvcSummary,
    SecretSummary, ServiceSummary,
};
pub use streams::{LogRequest, ShellRequest, StreamId};
pub use table::{TableColumn, TablePage, TableRow};
pub use watch::{Change, Listed};

use crate::error::describe;

/// A Kubernetes object that can be listed and watched.
pub trait Object: kube::Resource + Clone + serde::de::DeserializeOwned + std::fmt::Debug {}
impl<K: kube::Resource + Clone + serde::de::DeserializeOwned + std::fmt::Debug> Object for K {}

/// An object that lives in a namespace, such as `Pod`.
pub trait Namespaced: Object + kube::Resource<Scope = k8s_openapi::NamespaceResourceScope> {}
impl<K: Object + kube::Resource<Scope = k8s_openapi::NamespaceResourceScope>> Namespaced for K {}
use crate::{Error, Result};

/// Items requested per API call. Small enough for the first page to arrive quickly.
pub const PAGE_SIZE: u32 = 100;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Longer than the server's watch timeout (290 s), so a quiet watch is not cut off early.
const WATCH_READ_TIMEOUT: Duration = Duration::from_secs(300);
/// Covers the whole handshake, including exec credential plugins such as `aws eks get-token`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextInfo {
    pub name: String,
    pub cluster: Option<String>,
    pub user: Option<String>,
    pub namespace: Option<String>,
    pub server: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterInfo {
    pub context: String,
    pub server: String,
    pub version: String,
    pub platform: String,
}

/// Lists contexts from the default kubeconfig (`$KUBECONFIG` or `~/.kube/config`).
pub fn list_contexts() -> Result<Vec<ContextInfo>> {
    let config = read_kubeconfig()?;
    Ok(contexts_of(&config))
}

fn read_kubeconfig() -> Result<Kubeconfig> {
    Kubeconfig::read().map_err(|e| Error::Kubeconfig(describe(&e)))
}

fn contexts_of(config: &Kubeconfig) -> Vec<ContextInfo> {
    let current = config.current_context.as_deref();
    config
        .contexts
        .iter()
        .map(|named| {
            let context = named.context.as_ref();
            let cluster = context.map(|c| c.cluster.clone());
            let server = cluster.as_deref().and_then(|name| {
                config
                    .clusters
                    .iter()
                    .find(|c| c.name == name)
                    .and_then(|c| c.cluster.as_ref())
                    .and_then(|c| c.server.clone())
            });
            ContextInfo {
                name: named.name.clone(),
                cluster,
                user: context.and_then(|c| c.user.clone()),
                namespace: context.and_then(|c| c.namespace.clone()),
                server,
                current: current == Some(named.name.as_str()),
            }
        })
        .collect()
}

/// Connections to clusters, keyed by kubeconfig context and shared by all callers.
pub struct Clusters {
    page_size: u32,
    connections: Mutex<HashMap<String, Arc<OnceCell<Connection>>>>,
    streams: streams::Streams,
}

#[derive(Clone)]
struct Connection {
    client: Client,
    /// Like `client`, but waits longer for data, since a quiet watch sends nothing for minutes.
    watch_client: Client,
    server: String,
    /// Resource kinds the server offers, discovered on first use.
    kinds: Arc<OnceCell<Vec<ResourceKind>>>,
}

impl Default for Clusters {
    fn default() -> Self {
        Self::new()
    }
}

impl Clusters {
    pub fn new() -> Self {
        Self::with_page_size(PAGE_SIZE)
    }

    pub fn with_page_size(page_size: u32) -> Self {
        Self {
            page_size,
            connections: Mutex::default(),
            streams: streams::Streams::default(),
        }
    }

    /// Connects (or reuses the connection) and asks the API server for its version.
    pub async fn connect(&self, context: &str) -> Result<ClusterInfo> {
        let Connection { client, server, .. } = self.connection(context).await?;
        let info = client
            .apiserver_version()
            .await
            .map_err(|e| connect_error(context, &e))?;
        Ok(ClusterInfo {
            context: context.to_owned(),
            server,
            version: info.git_version,
            platform: info.platform,
        })
    }

    /// Drops the cached connection so the next call reconnects with fresh kubeconfig and credentials.
    pub async fn disconnect(&self, context: &str) {
        self.connections.lock().await.remove(context);
    }

    /// Lists a namespaced resource `K` as summaries `T`, e.g. `list::<Pod, PodSummary>`.
    /// `namespace: None` lists across all namespaces; `fields` is a field selector such as
    /// `spec.nodeName=node-1`.
    pub async fn list<K, T>(
        &self,
        context: &str,
        namespace: Option<&str>,
        fields: Option<&str>,
        on_page: impl FnMut(Vec<T>) -> ControlFlow<()>,
    ) -> Result<Listed>
    where
        K: Namespaced,
        K::DynamicType: Default,
        T: From<K>,
    {
        let api: Api<K> = scoped(self.client(context).await?, namespace);
        list_paged(&api, self.page_size, fields, T::from, on_page).await
    }

    /// Lists a cluster-scoped resource such as `Namespace` or `Node`.
    pub async fn list_cluster<K, T>(
        &self,
        context: &str,
        fields: Option<&str>,
        on_page: impl FnMut(Vec<T>) -> ControlFlow<()>,
    ) -> Result<Listed>
    where
        K: Object,
        K::DynamicType: Default,
        T: From<K>,
    {
        let api: Api<K> = Api::all(self.client(context).await?);
        list_paged(&api, self.page_size, fields, T::from, on_page).await
    }

    /// Follows changes to what [`Self::list`] returned, starting at its `resource_version`.
    /// Runs until stopped with [`Self::stop_stream`] or `on_change` breaks.
    pub async fn watch<K, T>(
        &self,
        context: &str,
        namespace: Option<&str>,
        fields: Option<&str>,
        resource_version: String,
        on_change: impl FnMut(Change<T>) -> ControlFlow<()> + Send + 'static,
    ) -> Result<StreamId>
    where
        K: Namespaced + Send + 'static,
        K::DynamicType: Default,
        T: From<K>,
    {
        let api: Api<K> = scoped(self.connection(context).await?.watch_client, namespace);
        Ok(self.spawn_watch(api, fields, resource_version, on_change))
    }

    /// [`Self::watch`] for a cluster-scoped resource.
    pub async fn watch_cluster<K, T>(
        &self,
        context: &str,
        fields: Option<&str>,
        resource_version: String,
        on_change: impl FnMut(Change<T>) -> ControlFlow<()> + Send + 'static,
    ) -> Result<StreamId>
    where
        K: Object + Send + 'static,
        K::DynamicType: Default,
        T: From<K>,
    {
        let api: Api<K> = Api::all(self.connection(context).await?.watch_client);
        Ok(self.spawn_watch(api, fields, resource_version, on_change))
    }

    fn spawn_watch<K, T>(
        &self,
        api: Api<K>,
        fields: Option<&str>,
        resource_version: String,
        mut on_change: impl FnMut(Change<T>) -> ControlFlow<()> + Send + 'static,
    ) -> StreamId
    where
        K: Object + Send + 'static,
        T: From<K>,
    {
        let mut params = WatchParams::default();
        if let Some(fields) = fields {
            params = params.fields(fields);
        }
        let open: watch::Open<K> = Box::new(move |rv| {
            let api = api.clone();
            let params = params.clone();
            async move { Ok(api.watch(&params, &rv).await?.boxed()) }.boxed()
        });
        let id = self.streams.next_id();
        self.streams.spawn(
            id,
            watch::follow(
                open,
                resource_version,
                |o: &K| o.meta().resource_version.clone(),
                move |event| {
                    on_change(match event {
                        watch::Event::Applied(o) => Change::Applied(T::from(o)),
                        watch::Event::Deleted(o) => Change::Deleted(T::from(o)),
                        watch::Event::Reset => Change::Reset,
                    })
                },
            ),
        );
        id
    }

    async fn client(&self, context: &str) -> Result<Client> {
        Ok(self.connection(context).await?.client)
    }

    /// Returns the cached connection, connecting once even when many calls race for it.
    /// A failed attempt is not cached, so the next call retries.
    async fn connection(&self, context: &str) -> Result<Connection> {
        let cell = self
            .connections
            .lock()
            .await
            .entry(context.to_owned())
            .or_default()
            .clone();
        cell.get_or_try_init(|| connect(context)).await.cloned()
    }
}

async fn connect(context: &str) -> Result<Connection> {
    crate::tls::init();
    let kubeconfig = read_kubeconfig()?;
    if !kubeconfig.contexts.iter().any(|c| c.name == context) {
        return Err(Error::UnknownContext(context.to_owned()));
    }
    let options = KubeConfigOptions {
        context: Some(context.to_owned()),
        ..Default::default()
    };
    let build = async {
        let mut config = Config::from_custom_kubeconfig(kubeconfig, &options)
            .await
            .map_err(|e| connect_error(context, &e))?;
        config.connect_timeout = Some(CONNECT_TIMEOUT);
        config.read_timeout = Some(READ_TIMEOUT);
        let server = config.cluster_url.to_string();
        let mut watch_config = config.clone();
        watch_config.read_timeout = Some(WATCH_READ_TIMEOUT);
        let client = Client::try_from(config).map_err(|e| connect_error(context, &e))?;
        let watch_client =
            Client::try_from(watch_config).map_err(|e| connect_error(context, &e))?;
        Ok(Connection {
            client,
            watch_client,
            server,
            kinds: Arc::default(),
        })
    };
    tokio::time::timeout(HANDSHAKE_TIMEOUT, build)
        .await
        .map_err(|_| Error::ConnectTimeout(context.to_owned()))?
}

fn connect_error(context: &str, error: &dyn std::error::Error) -> Error {
    Error::Connect {
        context: context.to_owned(),
        message: describe(error),
    }
}

fn scoped<K>(client: Client, namespace: Option<&str>) -> Api<K>
where
    K: Namespaced,
    K::DynamicType: Default,
{
    match namespace {
        Some(ns) => Api::namespaced(client, ns),
        None => Api::all(client),
    }
}

/// Lists every object page by page, passing each mapped page to `on_page`.
/// Stops early when `on_page` breaks.
async fn list_paged<K, T>(
    api: &Api<K>,
    page_size: u32,
    fields: Option<&str>,
    map: impl Fn(K) -> T,
    mut on_page: impl FnMut(Vec<T>) -> ControlFlow<()>,
) -> Result<Listed>
where
    K: Object,
{
    let mut params = ListParams::default().limit(page_size);
    if let Some(fields) = fields {
        params = params.fields(fields);
    }
    let mut listed = Listed {
        total: 0,
        resource_version: String::new(),
        complete: false,
    };
    loop {
        let list = api.list(&params).await?;
        // Pages after the first share its snapshot, so its version covers the whole list.
        if listed.resource_version.is_empty() {
            listed.resource_version = list.metadata.resource_version.unwrap_or_default();
        }
        let next = list.metadata.continue_.filter(|token| !token.is_empty());
        let page: Vec<T> = list.items.into_iter().map(&map).collect();
        listed.total += page.len();
        if on_page(page).is_break() {
            return Ok(listed);
        }
        match next {
            Some(token) => params = params.continue_token(&token),
            None => {
                listed.complete = true;
                return Ok(listed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KUBECONFIG: &str = r#"
apiVersion: v1
kind: Config
current-context: dev
clusters:
  - name: dev-cluster
    cluster: { server: "https://dev.example:6443" }
contexts:
  - name: dev
    context: { cluster: dev-cluster, user: alice, namespace: web }
  - name: orphan
    context: { cluster: missing, user: bob }
users:
  - name: alice
    user: { token: t }
"#;

    /// Regression: with the AWS SDK in the build, creating a kube client panicked because rustls
    /// could not pick a crypto provider. Building one needs no network.
    #[tokio::test]
    async fn builds_a_tls_client_with_both_providers_compiled_in() {
        crate::tls::init();
        let config = Kubeconfig::from_yaml(KUBECONFIG).unwrap();
        let options = KubeConfigOptions {
            context: Some("dev".into()),
            ..Default::default()
        };
        let config = Config::from_custom_kubeconfig(config, &options)
            .await
            .unwrap();
        Client::try_from(config).expect("TLS client");
    }

    #[test]
    fn reads_contexts_with_server_and_current_flag() {
        let config = Kubeconfig::from_yaml(KUBECONFIG).unwrap();
        let contexts = contexts_of(&config);
        assert_eq!(
            contexts,
            vec![
                ContextInfo {
                    name: "dev".into(),
                    cluster: Some("dev-cluster".into()),
                    user: Some("alice".into()),
                    namespace: Some("web".into()),
                    server: Some("https://dev.example:6443".into()),
                    current: true,
                },
                ContextInfo {
                    name: "orphan".into(),
                    cluster: Some("missing".into()),
                    user: Some("bob".into()),
                    namespace: None,
                    server: None,
                    current: false,
                },
            ]
        );
    }
}
