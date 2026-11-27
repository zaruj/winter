//! Tauri commands: translate GUI calls into `winter-core` calls, with no logic of their own.
//!
//! List commands stream pages through a `Channel` as they arrive. Once the list is complete they
//! keep it up to date: a watch sends each later change through a second `Channel`.

use std::ops::ControlFlow;

use serde::Serialize;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody};
use winter_core::aws::{
    Aws, Bucket, CostOverview, DbInstance, EbsVolume, Ec2Instance, EksCluster, HostedZone,
    Identity, LoadBalancer, MskCluster, ProfileInfo, Section, SecurityGroup, Vpc,
};
use winter_core::github::{Account, GitHub, Job, Repo, Run, RunsPage, Workflow};
use winter_core::k8s::{
    Change, ClusterInfo, Clusters, ConfigMap, ConfigMapSummary, ContextInfo, Deployment,
    DeploymentSummary, Ingress, IngressSummary, LogRequest, Namespace, NamespaceSummary, Node,
    NodeDetail, NodeSummary, NodeUsage, Object, ObjectView, PersistentVolumeClaim, Pod, PodSummary,
    PvcSummary, ResourceKind, Secret, SecretSummary, Service, ServiceSummary, ShellRequest,
    StreamId, TablePage, TableRow,
};
use winter_core::terminal::{SessionId, ShellCommand, Size, Terminals};
use winter_core::terminal_profile::TerminalProfile;

use crate::error::CommandError;

type CommandResult<T> = Result<T, CommandError>;

#[tauri::command]
pub async fn k8s_contexts() -> CommandResult<Vec<ContextInfo>> {
    Ok(
        tauri::async_runtime::spawn_blocking(winter_core::k8s::list_contexts)
            .await
            .map_err(|e| CommandError::internal(&e))??,
    )
}

#[tauri::command]
pub async fn k8s_connect(
    clusters: State<'_, Clusters>,
    context: String,
    fresh: bool,
) -> CommandResult<ClusterInfo> {
    if fresh {
        clusters.disconnect(&context).await;
    }
    Ok(clusters.connect(&context).await?)
}

/// What a list command resolves with once every page has been sent: the total, and the id of
/// the watch now sending changes to `on_change` (stop it with `k8s_stream_stop`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    total: usize,
    watch: Option<StreamId>,
}

#[tauri::command]
pub async fn k8s_namespaces(
    clusters: State<'_, Clusters>,
    context: String,
    on_page: Channel<Vec<NamespaceSummary>>,
    on_change: Channel<Change<NamespaceSummary>>,
) -> CommandResult<Listing> {
    list_cluster::<Namespace, NamespaceSummary>(&clusters, &context, None, on_page, on_change).await
}

#[tauri::command]
pub async fn k8s_nodes(
    clusters: State<'_, Clusters>,
    context: String,
    on_page: Channel<Vec<NodeSummary>>,
    on_change: Channel<Change<NodeSummary>>,
) -> CommandResult<Listing> {
    list_cluster::<Node, NodeSummary>(&clusters, &context, None, on_page, on_change).await
}

/// One node in detail, kept up to date the same way as a list (of one).
#[tauri::command]
pub async fn k8s_node(
    clusters: State<'_, Clusters>,
    context: String,
    name: String,
    on_page: Channel<Vec<NodeDetail>>,
    on_change: Channel<Change<NodeDetail>>,
) -> CommandResult<Listing> {
    let fields = format!("metadata.name={name}");
    list_cluster::<Node, NodeDetail>(&clusters, &context, Some(&fields), on_page, on_change).await
}

/// Node CPU and memory use from metrics-server; null when the cluster has none.
#[tauri::command]
pub async fn k8s_node_usage(
    clusters: State<'_, Clusters>,
    context: String,
) -> CommandResult<Option<Vec<NodeUsage>>> {
    Ok(clusters.node_usage(&context).await?)
}

async fn list_cluster<K, T>(
    clusters: &Clusters,
    context: &str,
    fields: Option<&str>,
    on_page: Channel<Vec<T>>,
    on_change: Channel<Change<T>>,
) -> CommandResult<Listing>
where
    K: Object + Send + 'static,
    K::DynamicType: Default,
    T: From<K> + Serialize + Clone + Send + 'static,
{
    let listed = clusters
        .list_cluster::<K, T>(context, fields, forward(&on_page))
        .await?;
    let watch = if listed.complete {
        let changes = send_changes(on_change);
        Some(
            clusters
                .watch_cluster::<K, T>(context, fields, listed.resource_version, changes)
                .await?,
        )
    } else {
        None
    };
    Ok(Listing {
        total: listed.total,
        watch,
    })
}

/// One streaming list command per namespaced kind. `namespace: None` lists all namespaces;
/// `fields` is a field selector such as `spec.nodeName=node-1`.
macro_rules! list_command {
    ($name:ident, $kind:ty, $summary:ty) => {
        #[tauri::command]
        pub async fn $name(
            clusters: State<'_, Clusters>,
            context: String,
            namespace: Option<String>,
            fields: Option<String>,
            on_page: Channel<Vec<$summary>>,
            on_change: Channel<Change<$summary>>,
        ) -> CommandResult<Listing> {
            let (namespace, fields) = (namespace.as_deref(), fields.as_deref());
            let listed = clusters
                .list::<$kind, $summary>(&context, namespace, fields, forward(&on_page))
                .await?;
            let watch = if listed.complete {
                let changes = send_changes(on_change);
                let rv = listed.resource_version;
                Some(
                    clusters
                        .watch::<$kind, $summary>(&context, namespace, fields, rv, changes)
                        .await?,
                )
            } else {
                None
            };
            Ok(Listing {
                total: listed.total,
                watch,
            })
        }
    };
}

list_command!(k8s_deployments, Deployment, DeploymentSummary);
list_command!(k8s_pods, Pod, PodSummary);
list_command!(k8s_services, Service, ServiceSummary);
list_command!(k8s_ingresses, Ingress, IngressSummary);
list_command!(k8s_pvcs, PersistentVolumeClaim, PvcSummary);
list_command!(k8s_configmaps, ConfigMap, ConfigMapSummary);
list_command!(k8s_secrets, Secret, SecretSummary);

/// Every listable kind the cluster serves (for `:` resource search), including CRDs.
#[tauri::command]
pub async fn k8s_resource_kinds(
    clusters: State<'_, Clusters>,
    context: String,
) -> CommandResult<Vec<ResourceKind>> {
    Ok(clusters.resource_kinds(&context).await?)
}

/// One object of any kind, its YAML and its events. Secret values stay hidden unless `reveal`.
#[tauri::command]
pub async fn k8s_object(
    clusters: State<'_, Clusters>,
    context: String,
    kind: ResourceKind,
    namespace: Option<String>,
    name: String,
    reveal: bool,
) -> CommandResult<ObjectView> {
    Ok(clusters
        .object(&context, &kind, namespace.as_deref(), &name, reveal)
        .await?)
}

/// Streams any kind as `kubectl get` prints it, then watches it. `namespace: None` lists all
/// namespaces.
#[tauri::command]
pub async fn k8s_table(
    clusters: State<'_, Clusters>,
    context: String,
    kind: ResourceKind,
    namespace: Option<String>,
    on_page: Channel<TablePage>,
    on_change: Channel<Change<TableRow>>,
) -> CommandResult<Listing> {
    let namespace = namespace.as_deref();
    let listed = clusters
        .table(&context, &kind, namespace, |page| {
            match on_page.send(page) {
                Ok(()) => ControlFlow::Continue(()),
                Err(_) => ControlFlow::Break(()),
            }
        })
        .await?;
    let watch = if listed.complete {
        let changes = send_changes(on_change);
        let rv = listed.resource_version;
        Some(
            clusters
                .watch_table(&context, &kind, namespace, rv, changes)
                .await?,
        )
    } else {
        None
    };
    Ok(Listing {
        total: listed.total,
        watch,
    })
}

/// Streams a container log as raw bytes. `on_end` gets an error message, or null when the log
/// simply ended. Returns an id for `k8s_stream_stop`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn k8s_logs(
    clusters: State<'_, Clusters>,
    context: String,
    namespace: String,
    pod: String,
    container: Option<String>,
    tail_lines: Option<i64>,
    on_chunk: Channel<InvokeResponseBody>,
    on_end: Channel<Option<String>>,
) -> CommandResult<StreamId> {
    let request = LogRequest {
        namespace,
        pod,
        container,
        tail_lines,
        follow: true,
    };
    Ok(clusters
        .start_logs(
            &context,
            request,
            move |chunk| {
                let _ = on_chunk.send(InvokeResponseBody::Raw(chunk));
            },
            move |error| {
                let _ = on_end.send(error.map(|e| e.to_string()));
            },
        )
        .await?)
}

/// Opens a shell in a container, like `kubectl exec -it`. Same channels as `terminal_open`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn k8s_shell_open(
    clusters: State<'_, Clusters>,
    context: String,
    namespace: String,
    pod: String,
    container: Option<String>,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
    on_exit: Channel<Option<i32>>,
) -> CommandResult<StreamId> {
    let request = ShellRequest {
        namespace,
        pod,
        container,
    };
    Ok(clusters
        .open_shell(
            &context,
            request,
            Size { cols, rows },
            move |bytes| {
                let _ = on_output.send(InvokeResponseBody::Raw(bytes));
            },
            move |code| {
                let _ = on_exit.send(code);
            },
        )
        .await?)
}

#[tauri::command]
pub async fn k8s_shell_write(
    clusters: State<'_, Clusters>,
    id: StreamId,
    data: String,
) -> CommandResult<()> {
    Ok(clusters.shell_write(id, data.as_bytes()).await?)
}

#[tauri::command]
pub async fn k8s_shell_resize(
    clusters: State<'_, Clusters>,
    id: StreamId,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    Ok(clusters.shell_resize(id, Size { cols, rows }).await?)
}

/// Stops a log stream or closes a pod shell.
#[tauri::command]
pub fn k8s_stream_stop(clusters: State<'_, Clusters>, id: StreamId) {
    clusters.stop_stream(id);
}

/// Profiles from `~/.aws/config` and `~/.aws/credentials` (names and regions, no secrets), the
/// most recently opened first.
#[tauri::command]
pub async fn aws_profiles() -> CommandResult<Vec<ProfileInfo>> {
    Ok(
        tauri::async_runtime::spawn_blocking(winter_core::aws::list_profiles)
            .await
            .map_err(|e| CommandError::internal(&e))??,
    )
}

/// Checks the profile's credentials; `fresh` reloads them first (e.g. after `aws sso login`).
#[tauri::command]
pub async fn aws_connect(
    aws: State<'_, Aws>,
    profile: String,
    region: String,
    fresh: bool,
) -> CommandResult<Identity> {
    if fresh {
        aws.disconnect(&profile).await;
    }
    let identity = aws.connect(&profile, &region).await?;
    // Signed in: remember it for the profile list. Failing to save is not worth failing for.
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = winter_core::state::record_aws_profile(&profile, &region) {
            eprintln!("winter: {e}");
        }
    });
    Ok(identity)
}

#[tauri::command]
pub async fn aws_regions(
    aws: State<'_, Aws>,
    profile: String,
    region: String,
) -> CommandResult<Vec<String>> {
    Ok(aws.regions(&profile, &region).await?)
}

/// One streaming list command per AWS resource; resolves with the total.
macro_rules! aws_list_command {
    ($name:ident, $method:ident, $item:ty) => {
        #[tauri::command]
        pub async fn $name(
            aws: State<'_, Aws>,
            profile: String,
            region: String,
            on_page: Channel<Vec<$item>>,
        ) -> CommandResult<usize> {
            Ok(aws.$method(&profile, &region, forward(&on_page)).await?)
        }
    };
}

aws_list_command!(aws_ec2_instances, ec2_instances, Ec2Instance);
aws_list_command!(aws_eks_clusters, eks_clusters, EksCluster);
aws_list_command!(aws_load_balancers, load_balancers, LoadBalancer);
aws_list_command!(aws_vpcs, vpcs, Vpc);
aws_list_command!(aws_security_groups, security_groups, SecurityGroup);
aws_list_command!(aws_ebs_volumes, ebs_volumes, EbsVolume);
aws_list_command!(aws_s3_buckets, s3_buckets, Bucket);
aws_list_command!(aws_hosted_zones, hosted_zones, HostedZone);
aws_list_command!(aws_msk_clusters, msk_clusters, MskCluster);
aws_list_command!(aws_rds_instances, rds_instances, DbInstance);

/// One resource in detail, as sections (see `winter_core::aws::Aws::detail`).
#[tauri::command]
pub async fn aws_detail(
    aws: State<'_, Aws>,
    profile: String,
    region: String,
    kind: String,
    id: String,
) -> CommandResult<Vec<Section>> {
    Ok(aws.detail(&profile, &region, &kind, &id).await?)
}

/// Spend this month and last, forecast, daily spend and top services (billed by AWS per call).
#[tauri::command]
pub async fn aws_cost_overview(
    aws: State<'_, Aws>,
    profile: String,
) -> CommandResult<CostOverview> {
    Ok(aws.cost_overview(&profile).await?)
}

/// Signs in to GitHub with a token from the Keychain, the environment or the GitHub CLI; null
/// when none is found. `fresh` looks again even when already signed in.
#[tauri::command]
pub async fn github_connect(
    github: State<'_, GitHub>,
    fresh: bool,
) -> CommandResult<Option<Account>> {
    Ok(github.connect(fresh).await?)
}

/// Checks a personal access token and saves it in the Keychain.
#[tauri::command]
pub async fn github_sign_in(github: State<'_, GitHub>, token: String) -> CommandResult<Account> {
    Ok(github.sign_in(&token).await?)
}

/// Saves the credential git uses for github.com in winter's Keychain item and signs in with it.
#[tauri::command]
pub async fn github_sign_in_with_git(github: State<'_, GitHub>) -> CommandResult<Account> {
    Ok(github.sign_in_with_git().await?)
}

#[tauri::command]
pub async fn github_sign_out(github: State<'_, GitHub>) -> CommandResult<()> {
    Ok(github.sign_out().await?)
}

#[tauri::command]
pub async fn github_repos(
    github: State<'_, GitHub>,
    on_page: Channel<Vec<Repo>>,
) -> CommandResult<usize> {
    Ok(github.repos(forward(&on_page)).await?)
}

/// One repository; also remembers it as recently opened.
#[tauri::command]
pub async fn github_repo(github: State<'_, GitHub>, repo: String) -> CommandResult<Repo> {
    let found = github.repo(&repo).await?;
    let name = found.name.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = winter_core::state::record_github_repo(&name) {
            eprintln!("winter: {e}");
        }
    });
    Ok(found)
}

#[tauri::command]
pub async fn github_workflows(
    github: State<'_, GitHub>,
    repo: String,
) -> CommandResult<Vec<Workflow>> {
    Ok(github.workflows(&repo).await?)
}

#[tauri::command]
pub async fn github_runs(
    github: State<'_, GitHub>,
    repo: String,
    workflow: Option<u64>,
) -> CommandResult<RunsPage> {
    Ok(github.runs(&repo, workflow).await?)
}

#[tauri::command]
pub async fn github_run(github: State<'_, GitHub>, repo: String, id: u64) -> CommandResult<Run> {
    Ok(github.run(&repo, id).await?)
}

#[tauri::command]
pub async fn github_jobs(
    github: State<'_, GitHub>,
    repo: String,
    run: u64,
) -> CommandResult<Vec<Job>> {
    Ok(github.jobs(&repo, run).await?)
}

#[tauri::command]
pub async fn github_job_log(
    github: State<'_, GitHub>,
    repo: String,
    job: u64,
) -> CommandResult<String> {
    Ok(github.job_log(&repo, job).await?)
}

/// Opens a github.com page in the default browser. Other URLs are refused.
#[tauri::command]
pub fn open_github_url(url: String) -> CommandResult<()> {
    let allowed = url.starts_with("https://github.com/")
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !allowed {
        return Err(winter_core::Error::GitHub(format!("not a github.com URL: {url}")).into());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .spawn()
        .map_err(|e| CommandError::internal(&e))?;
    Ok(())
}

/// Font and colours of Terminal.app's default profile, if any.
#[tauri::command]
pub async fn terminal_profile() -> Option<TerminalProfile> {
    tauri::async_runtime::spawn_blocking(winter_core::terminal_profile::default_profile)
        .await
        .ok()
        .flatten()
}

/// Starts the user's login shell. Output is streamed as raw bytes (an `ArrayBuffer` in JS);
/// `on_exit` receives the exit code once the shell ends.
#[tauri::command]
pub fn terminal_open(
    terminals: State<'_, Terminals>,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
    on_exit: Channel<Option<u32>>,
) -> CommandResult<SessionId> {
    Ok(terminals.open(
        &ShellCommand::login(),
        Size { cols, rows },
        move |bytes| {
            let _ = on_output.send(InvokeResponseBody::Raw(bytes.to_vec()));
        },
        move |code| {
            let _ = on_exit.send(code);
        },
    )?)
}

#[tauri::command]
pub fn terminal_write(
    terminals: State<'_, Terminals>,
    id: SessionId,
    data: String,
) -> CommandResult<()> {
    Ok(terminals.write(id, data.as_bytes())?)
}

#[tauri::command]
pub fn terminal_resize(
    terminals: State<'_, Terminals>,
    id: SessionId,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    Ok(terminals.resize(id, Size { cols, rows })?)
}

#[tauri::command]
pub fn terminal_close(terminals: State<'_, Terminals>, id: SessionId) -> CommandResult<()> {
    Ok(terminals.close(id)?)
}

/// Sends each change to the webview; stops watching if the webview is gone.
fn send_changes<T: Serialize + Clone + Send + 'static>(
    channel: Channel<Change<T>>,
) -> impl FnMut(Change<T>) -> ControlFlow<()> + Send + 'static {
    move |change| match channel.send(change) {
        Ok(()) => ControlFlow::Continue(()),
        Err(_) => ControlFlow::Break(()),
    }
}

/// Sends each page to the webview; stops listing if the webview is gone.
fn forward<T: Serialize + Clone>(
    channel: &Channel<Vec<T>>,
) -> impl FnMut(Vec<T>) -> ControlFlow<()> {
    move |page| match channel.send(page) {
        Ok(()) => ControlFlow::Continue(()),
        Err(_) => ControlFlow::Break(()),
    }
}
