//! Talks to a real cluster. Run with:
//! `WINTER_TEST_CONTEXT=<context> cargo test -p winter-core --test live_cluster -- --ignored --nocapture`
//! Lists resources and reads logs. `watches_changes` creates and deletes a ConfigMap named
//! `winter-watch-test` in the `default` namespace. `opens_a_shell` also runs `echo` inside the pod named by
//! `WINTER_TEST_POD` (`namespace/pod`), if set.

use std::ops::ControlFlow;

use std::time::Duration;
use winter_core::Error;

use k8s_openapi::api::core::v1::ConfigMap as RawConfigMap;
use kube::api::{DeleteParams, PostParams};
use winter_core::k8s::{
    Change, Clusters, ConfigMap, ConfigMapSummary, Deployment, DeploymentSummary, Ingress,
    IngressSummary, LogRequest, Namespace, NamespaceSummary, Node, NodeDetail, NodeSummary,
    PersistentVolumeClaim, Pod, PodSummary, PvcSummary, Secret, SecretSummary, Service,
    ServiceSummary, ShellRequest, list_contexts,
};
use winter_core::terminal::Size;

fn context() -> String {
    std::env::var("WINTER_TEST_CONTEXT").expect("set WINTER_TEST_CONTEXT")
}

#[tokio::test]
#[ignore = "needs a reachable cluster"]
async fn lists_in_pages() {
    let context = context();
    assert!(list_contexts().unwrap().iter().any(|c| c.name == context));

    // A tiny page size forces several round trips.
    let clusters = Clusters::with_page_size(3);
    let info = clusters.connect(&context).await.unwrap();
    println!("{info:?}");

    let (namespaces, deployments, pods) = tokio::join!(
        async {
            let mut pages = Vec::new();
            let total = clusters
                .list_cluster::<Namespace, NamespaceSummary>(&context, None, |p| {
                    pages.push(p.len());
                    ControlFlow::Continue(())
                })
                .await
                .unwrap();
            (total.total, pages)
        },
        async {
            let mut pages = Vec::new();
            let total = clusters
                .list::<Deployment, DeploymentSummary>(&context, None, None, |p| {
                    pages.push(p.len());
                    ControlFlow::Continue(())
                })
                .await
                .unwrap();
            (total.total, pages)
        },
        async {
            let mut all = Vec::new();
            let total = clusters
                .list::<Pod, PodSummary>(&context, None, None, |p| {
                    all.extend(p);
                    ControlFlow::Continue(())
                })
                .await
                .unwrap();
            (total.total, all)
        },
    );
    println!("namespaces {namespaces:?}\ndeployments {deployments:?}");
    for p in &pods.1 {
        println!(
            "POD {} {} {}/{} {} {}",
            p.namespace, p.name, p.ready, p.containers, p.status, p.restarts
        );
    }
    assert!(namespaces.1.iter().all(|&n| n <= 3));
    assert_eq!(namespaces.0, namespaces.1.iter().sum::<usize>());

    // Breaking stops after the first page.
    let mut calls = 0;
    let listed = clusters
        .list::<Pod, PodSummary>(&context, None, None, |_| {
            calls += 1;
            ControlFlow::Break(())
        })
        .await
        .unwrap();
    assert_eq!((calls, listed.complete), (1, false));
}

#[tokio::test]
#[ignore = "needs kubeconfig"]
async fn unknown_context_is_reported() {
    let err = Clusters::new()
        .connect("no-such-context")
        .await
        .unwrap_err();
    assert_eq!(err, Error::UnknownContext("no-such-context".into()));
}

#[tokio::test]
#[ignore = "needs an unreachable context"]
async fn unreachable_cluster_explains_why() {
    let Ok(context) = std::env::var("WINTER_TEST_DOWN_CONTEXT") else {
        return;
    };
    let started = std::time::Instant::now();
    let err = Clusters::new().connect(&context).await.unwrap_err();
    println!("{:?} after {:?}", err.to_string(), started.elapsed());
}

async fn count<K, T>(clusters: &Clusters, context: &str) -> usize
where
    K: kube::Resource<Scope = k8s_openapi::NamespaceResourceScope>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug,
    K::DynamicType: Default,
    T: From<K>,
{
    clusters
        .list::<K, T>(context, None, None, |_| ControlFlow::Continue(()))
        .await
        .unwrap()
        .total
}

/// Lists every kind, to check each one deserializes against a real API server.
#[tokio::test]
#[ignore = "needs a reachable cluster"]
async fn lists_every_kind() {
    let context = context();
    let c = Clusters::new();
    println!(
        "services {} ingresses {} pvcs {} configmaps {} secrets {}",
        count::<Service, ServiceSummary>(&c, &context).await,
        count::<Ingress, IngressSummary>(&c, &context).await,
        count::<PersistentVolumeClaim, PvcSummary>(&c, &context).await,
        count::<ConfigMap, ConfigMapSummary>(&c, &context).await,
        count::<Secret, SecretSummary>(&c, &context).await,
    );
}

#[tokio::test]
#[ignore = "needs a reachable cluster"]
async fn reads_logs() {
    let context = context();
    let clusters = Clusters::new();
    let mut pods = Vec::new();
    clusters
        .list::<Pod, PodSummary>(&context, None, None, |p| {
            pods.extend(p);
            ControlFlow::Continue(())
        })
        .await
        .unwrap();
    let pod = pods
        .iter()
        .find(|p| p.status == "Running")
        .expect("a running pod");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (end_tx, end_rx) = tokio::sync::oneshot::channel();
    clusters
        .start_logs(
            &context,
            LogRequest {
                namespace: pod.namespace.clone(),
                pod: pod.name.clone(),
                container: pod.default_container.clone(),
                tail_lines: Some(5),
                follow: false,
            },
            move |chunk| {
                let _ = tx.send(chunk);
            },
            move |error| {
                let _ = end_tx.send(error);
            },
        )
        .await
        .unwrap();
    assert_eq!(end_rx.await.unwrap(), None);
    let mut text = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        text.extend(chunk);
    }
    let text = String::from_utf8_lossy(&text);
    println!("{}/{} last lines:\n{text}", pod.namespace, pod.name);
    assert!(text.lines().count() <= 5);
}

#[tokio::test]
#[ignore = "needs a reachable cluster and WINTER_TEST_POD"]
async fn opens_a_shell() {
    let Ok(target) = std::env::var("WINTER_TEST_POD") else {
        return;
    };
    let (namespace, pod) = target.split_once('/').expect("namespace/pod");
    let context = context();
    let clusters = Clusters::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
    let id = clusters
        .open_shell(
            &context,
            ShellRequest {
                namespace: namespace.into(),
                pod: pod.into(),
                container: None,
            },
            Size {
                cols: 100,
                rows: 30,
            },
            move |bytes| {
                let _ = tx.send(bytes);
            },
            move |code| {
                let _ = exit_tx.send(code);
            },
        )
        .await
        .unwrap();
    clusters
        .shell_write(id, b"echo winter-$((40+2)); stty size; exit 7\n")
        .await
        .unwrap();
    let code = tokio::time::timeout(std::time::Duration::from_secs(15), exit_rx)
        .await
        .unwrap()
        .unwrap();
    let mut out = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        out.extend(chunk);
    }
    let out = String::from_utf8_lossy(&out);
    println!("exit {code:?}\n{out}");
    assert!(out.contains("winter-42"));
    assert!(out.contains("30 100"));
    assert_eq!(code, Some(7));
}

#[tokio::test]
#[ignore = "needs a reachable cluster"]
async fn discovers_kinds_and_prints_tables() {
    let context = context();
    let clusters = Clusters::with_page_size(2);
    let kinds = clusters.resource_kinds(&context).await.unwrap();
    println!("{} kinds", kinds.len());
    let find = |name: &str| {
        kinds
            .iter()
            .find(|k| k.plural == name || k.short_names.iter().any(|s| s == name))
            .unwrap_or_else(|| panic!("no {name}"))
            .clone()
    };
    for (alias, namespace) in [
        ("no", None),
        ("app", Some("argocd")),
        ("cj", None),
        ("crd", None),
    ] {
        let kind = find(alias);
        let mut columns = Vec::new();
        let mut rows = Vec::new();
        clusters
            .table(&context, &kind, namespace, |page| {
                columns = page
                    .columns
                    .iter()
                    .map(|c| format!("{}:{}", c.name, c.type_))
                    .collect();
                rows.extend(page.rows);
                ControlFlow::Continue(())
            })
            .await
            .unwrap();
        println!("== {alias} -> {}.{} {:?}", kind.plural, kind.group, columns);
        for r in rows.iter().take(3) {
            println!("   {:?} {} {:?}", r.namespace, r.name, r.cells);
        }
        println!("   ({} rows)", rows.len());
    }
}

async fn next_change<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Change<T>>) -> Change<T> {
    tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("a change within 15 s")
        .expect("watch still running")
}

/// A watch started from a list sees later additions and deletions, including in the table view.
#[tokio::test]
#[ignore = "needs a reachable cluster; writes a ConfigMap"]
async fn watches_changes() {
    const NAME: &str = "winter-watch-test";
    let context = context();
    let clusters = Clusters::new();
    let listed = clusters
        .list::<ConfigMap, ConfigMapSummary>(&context, Some("default"), None, |_| {
            ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert!(listed.complete && !listed.resource_version.is_empty());

    let (tx, mut typed) = tokio::sync::mpsc::unbounded_channel();
    let typed_id = clusters
        .watch::<ConfigMap, ConfigMapSummary>(
            &context,
            Some("default"),
            None,
            listed.resource_version.clone(),
            move |c| {
                let _ = tx.send(c);
                ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();
    let kinds = clusters.resource_kinds(&context).await.unwrap();
    let cm = kinds.iter().find(|k| k.plural == "configmaps").unwrap();
    let table = clusters
        .table(&context, cm, Some("default"), |_| ControlFlow::Continue(()))
        .await
        .unwrap();
    let (tx, mut rows) = tokio::sync::mpsc::unbounded_channel();
    let table_id = clusters
        .watch_table(
            &context,
            cm,
            Some("default"),
            table.resource_version,
            move |c| {
                let _ = tx.send(c);
                ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();

    let config = kube::Config::from_kubeconfig(&kube::config::KubeConfigOptions {
        context: Some(context.clone()),
        ..Default::default()
    })
    .await
    .unwrap();
    let api: kube::Api<RawConfigMap> =
        kube::Api::namespaced(kube::Client::try_from(config).unwrap(), "default");
    let _ = api.delete(NAME, &DeleteParams::default()).await;
    // Ignore events left over from an earlier run.
    tokio::time::sleep(Duration::from_millis(500)).await;
    while typed.try_recv().is_ok() {}
    while rows.try_recv().is_ok() {}

    let mut object = RawConfigMap::default();
    object.metadata.name = Some(NAME.into());
    object.data = Some([("a".to_owned(), "b".to_owned())].into());
    api.create(&PostParams::default(), &object).await.unwrap();
    api.delete(NAME, &DeleteParams::default()).await.unwrap();

    let added = next_change(&mut typed).await;
    assert!(
        matches!(&added, Change::Applied(c) if c.name == NAME && c.keys == 1),
        "{added:?}"
    );
    loop {
        match next_change(&mut typed).await {
            Change::Deleted(c) if c.name == NAME => break,
            Change::Applied(_) => continue,
            other => panic!("unexpected {other:?}"),
        }
    }
    let added = next_change(&mut rows).await;
    println!("table row {added:?}");
    assert!(matches!(&added, Change::Applied(r) if r.name == NAME && r.cells.len() >= 2));
    loop {
        match next_change(&mut rows).await {
            Change::Deleted(r) if r.name == NAME => break,
            Change::Applied(_) => continue,
            other => panic!("unexpected {other:?}"),
        }
    }
    clusters.stop_stream(typed_id);
    clusters.stop_stream(table_id);
}

#[tokio::test]
#[ignore = "needs a reachable cluster"]
async fn reads_nodes() {
    let context = context();
    let clusters = Clusters::new();
    let mut nodes = Vec::new();
    clusters
        .list_cluster::<Node, NodeSummary>(&context, None, |p| {
            nodes.extend(p);
            ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert!(!nodes.is_empty());
    for n in &nodes {
        println!("{n:?}");
    }
    let first = &nodes[0].name;
    let mut detail = Vec::new();
    clusters
        .list_cluster::<Node, NodeDetail>(&context, Some(&format!("metadata.name={first}")), |p| {
            detail.extend(p);
            ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(detail.len(), 1);
    println!("conditions {:?}", detail[0].conditions);
    println!("usage {:?}", clusters.node_usage(&context).await.unwrap());

    let mut pods = Vec::new();
    clusters
        .list::<Pod, PodSummary>(
            &context,
            None,
            Some(&format!("spec.nodeName={first}")),
            |p| {
                pods.extend(p);
                ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();
    assert!(
        pods.iter()
            .all(|p| p.node.as_deref() == Some(first.as_str()))
    );
    println!("{} pods on {first}", pods.len());
}
