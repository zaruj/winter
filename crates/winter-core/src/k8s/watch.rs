//! Keeps a list up to date after it has been fetched, like `kubectl get --watch`.
//!
//! A watch starts from the resourceVersion the list returned, so no change between the list and
//! the watch is missed. Each change goes to a callback as a [`Change`]. The watch reconnects by
//! itself when the server closes it (every few minutes) or the network drops. If the server no
//! longer has the history needed to resume, it sends [`Change::Reset`] and stops: the caller
//! should list again.

use std::ops::ControlFlow;
use std::time::Duration;

use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use kube::core::WatchEvent;
use serde::Serialize;

/// What changed in a watched list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "item", rename_all = "camelCase")]
pub enum Change<T> {
    /// Added or modified: replaces the item with the same namespace and name, or adds it.
    Applied(T),
    /// Removed from the list (the last state of the item is included).
    Deleted(T),
    /// The watch cannot resume and has stopped. List again to catch up.
    Reset,
}

/// Result of listing every page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub total: usize,
    /// Where a watch can start so it sees every later change.
    pub resource_version: String,
    /// False when the caller stopped early, so there is nothing to keep up to date.
    pub complete: bool,
}

/// An event from the server, before mapping to [`Change`].
pub(super) enum Event<O> {
    Applied(O),
    Deleted(O),
    Reset,
}

pub(super) type Events<O> = BoxStream<'static, kube::Result<WatchEvent<O>>>;
/// Opens a watch from a resourceVersion.
pub(super) type Open<O> = Box<dyn Fn(String) -> BoxFuture<'static, kube::Result<Events<O>>> + Send>;

const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Watches until `on_event` breaks, the history is gone (after sending `Reset`), or the server
/// refuses the watch (e.g. no permission). `version` reads the resourceVersion of an object.
pub(super) async fn follow<O>(
    open: Open<O>,
    mut resource_version: String,
    version: impl Fn(&O) -> Option<String>,
    mut on_event: impl FnMut(Event<O>) -> ControlFlow<()>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let mut events = match open(resource_version.clone()).await {
            Ok(events) => events,
            Err(e) => match classify(&e) {
                Failure::Gone => {
                    let _ = on_event(Event::Reset);
                    return;
                }
                Failure::Refused => return,
                Failure::Retry => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }
            },
        };
        while let Some(event) = events.next().await {
            let event = match event {
                Ok(WatchEvent::Added(o) | WatchEvent::Modified(o)) => {
                    resource_version = version(&o).unwrap_or(resource_version);
                    Event::Applied(o)
                }
                Ok(WatchEvent::Deleted(o)) => {
                    resource_version = version(&o).unwrap_or(resource_version);
                    Event::Deleted(o)
                }
                Ok(WatchEvent::Bookmark(b)) => {
                    resource_version = b.metadata.resource_version;
                    continue;
                }
                Ok(WatchEvent::Error(status)) if status.code == 410 => Event::Reset,
                // Any other failure: reopen from the last version seen.
                Ok(WatchEvent::Error(_)) | Err(_) => break,
            };
            backoff = Duration::from_secs(1);
            let reset = matches!(event, Event::Reset);
            if on_event(event).is_break() || reset {
                return;
            }
        }
        // The server ends every watch after a timeout; a quick reconnect is normal. Wait a little
        // anyway so a watch that keeps failing does not spin.
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

enum Failure {
    /// 410: the resourceVersion is too old to resume from.
    Gone,
    /// The server will keep saying no (forbidden, not found, ...).
    Refused,
    /// Network trouble or a busy server.
    Retry,
}

fn classify(error: &kube::Error) -> Failure {
    match error {
        kube::Error::Api(status) => match status.code {
            410 => Failure::Gone,
            401 | 403 | 404 | 405 => Failure::Refused,
            _ => Failure::Retry,
        },
        _ => Failure::Retry,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use futures::FutureExt;
    use kube::core::Status;

    use super::*;

    fn status(code: u16) -> Box<Status> {
        Box::new(Status {
            code,
            ..Default::default()
        })
    }

    /// Serves one scripted stream per open, recording the versions asked for.
    fn script(streams: Vec<Vec<WatchEvent<u32>>>) -> (Open<u32>, Arc<Mutex<Vec<String>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let streams = Arc::new(Mutex::new(streams.into_iter()));
        let log = Arc::clone(&asked);
        let open: Open<u32> = Box::new(move |rv| {
            log.lock().unwrap().push(rv);
            let next = streams.lock().unwrap().next();
            async move {
                match next {
                    Some(events) => Ok(futures::stream::iter(events.into_iter().map(Ok)).boxed()),
                    None => Err(kube::Error::Api(status(403))),
                }
            }
            .boxed()
        });
        (open, asked)
    }

    #[tokio::test]
    async fn resumes_from_the_last_version_and_stops_when_refused() {
        let (open, asked) = script(vec![
            vec![WatchEvent::Added(5), WatchEvent::Modified(6)],
            vec![WatchEvent::Deleted(7)],
        ]);
        let mut seen = Vec::new();
        follow(
            open,
            "1".into(),
            |o| Some(o.to_string()),
            |e| {
                seen.push(match e {
                    Event::Applied(o) => format!("+{o}"),
                    Event::Deleted(o) => format!("-{o}"),
                    Event::Reset => "reset".into(),
                });
                ControlFlow::Continue(())
            },
        )
        .await;
        assert_eq!(seen, ["+5", "+6", "-7"]);
        assert_eq!(*asked.lock().unwrap(), ["1", "6", "7"]);
    }

    #[tokio::test]
    async fn reports_reset_when_history_is_gone() {
        let (open, _) = script(vec![vec![
            WatchEvent::Added(2),
            WatchEvent::Error(status(410)),
            WatchEvent::Added(3),
        ]]);
        let mut seen = Vec::new();
        follow(
            open,
            "1".into(),
            |o| Some(o.to_string()),
            |e| {
                seen.push(matches!(e, Event::Reset));
                ControlFlow::Continue(())
            },
        )
        .await;
        assert_eq!(seen, [false, true]);
    }

    #[tokio::test]
    async fn stops_when_the_callback_breaks() {
        let (open, asked) = script(vec![vec![WatchEvent::Added(2), WatchEvent::Added(3)]]);
        let mut calls = 0;
        follow(
            open,
            "1".into(),
            |o| Some(o.to_string()),
            |_| {
                calls += 1;
                ControlFlow::Break(())
            },
        )
        .await;
        assert_eq!((calls, asked.lock().unwrap().len()), (1, 1));
    }

    #[test]
    fn changes_serialize_for_the_gui() {
        let json = |c: Change<u32>| serde_json::to_string(&c).unwrap();
        assert_eq!(json(Change::Applied(1)), r#"{"type":"applied","item":1}"#);
        assert_eq!(json(Change::Deleted(1)), r#"{"type":"deleted","item":1}"#);
        assert_eq!(json(Change::Reset), r#"{"type":"reset"}"#);
    }
}
