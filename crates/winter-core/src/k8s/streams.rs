//! Long-running pod streams: following logs and interactive shells (`kubectl exec -it`).
//!
//! Each runs as a Tokio task that hands bytes to a callback. Tasks are tracked by id so the GUI
//! can stop them when the user leaves the view.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::{AsyncReadExt, SinkExt};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Status;
use kube::Api;
use kube::api::{AttachParams, LogParams, TerminalSize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt, SimplexStream, WriteHalf};
use tokio::task::AbortHandle;

use super::Clusters;
use crate::terminal::Size;
use crate::{Error, Result};

pub type StreamId = u32;

/// Runs bash when the image has it, otherwise sh (as k9s does).
const SHELL: &str = "TERM=xterm-256color; export TERM; \
    if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRequest {
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
    /// Lines from the end to start with; `None` for the whole log.
    pub tail_lines: Option<i64>,
    pub follow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellRequest {
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
}

struct Shell {
    stdin: Arc<tokio::sync::Mutex<WriteHalf<SimplexStream>>>,
    resize: mpsc::Sender<TerminalSize>,
}

#[derive(Default)]
pub(super) struct Streams {
    next_id: AtomicU32,
    tasks: Arc<Mutex<HashMap<StreamId, AbortHandle>>>,
    shells: Arc<Mutex<HashMap<StreamId, Shell>>>,
}

impl Streams {
    pub(super) fn next_id(&self) -> StreamId {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Runs `task` under `id`, forgetting it when it ends.
    pub(super) fn spawn(&self, id: StreamId, task: impl Future<Output = ()> + Send + 'static) {
        let tasks = Arc::clone(&self.tasks);
        let shells = Arc::clone(&self.shells);
        let handle = tokio::spawn(async move {
            task.await;
            tasks.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            shells.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
        });
        self.tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, handle.abort_handle());
    }
}

impl Drop for Streams {
    fn drop(&mut self) {
        for task in self
            .tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
        {
            task.abort();
        }
    }
}

impl Clusters {
    /// Streams a container's log. Fails right away if the log cannot be opened (e.g. the pod is
    /// gone); later errors go to `on_end`, which is called exactly once unless the stream is
    /// stopped.
    pub async fn start_logs(
        &self,
        context: &str,
        request: LogRequest,
        mut on_chunk: impl FnMut(Vec<u8>) + Send + 'static,
        on_end: impl FnOnce(Option<Error>) + Send + 'static,
    ) -> Result<StreamId> {
        let api: Api<Pod> = Api::namespaced(self.client(context).await?, &request.namespace);
        let params = LogParams {
            container: request.container,
            follow: request.follow,
            tail_lines: request.tail_lines,
            ..Default::default()
        };
        let mut reader = api.log_stream(&request.pod, &params).await?;
        let id = self.streams.next_id();
        self.streams.spawn(id, async move {
            let mut buf = vec![0u8; 32 * 1024];
            let error = loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break None,
                    Ok(n) => on_chunk(buf[..n].to_vec()),
                    Err(e) => break Some(Error::Kube(e.to_string())),
                }
            };
            on_end(error);
        });
        Ok(id)
    }

    /// Opens an interactive shell in a container. `on_exit` gets the shell's exit code when
    /// known.
    pub async fn open_shell(
        &self,
        context: &str,
        request: ShellRequest,
        size: Size,
        mut on_output: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<i32>) + Send + 'static,
    ) -> Result<StreamId> {
        let api: Api<Pod> = Api::namespaced(self.client(context).await?, &request.namespace);
        let mut params = AttachParams::interactive_tty();
        if let Some(container) = request.container {
            params = params.container(container);
        }
        let mut process = api
            .exec(&request.pod, ["/bin/sh", "-c", SHELL], &params)
            .await?;
        let (Some(stdin), Some(mut stdout), Some(mut resize), Some(status)) = (
            process.stdin(),
            process.stdout(),
            process.terminal_size(),
            process.take_status(),
        ) else {
            return Err(Error::Kube("exec did not open a terminal".into()));
        };
        // Best effort: the shell still works at the default size.
        let _ = resize
            .send(TerminalSize {
                width: size.cols,
                height: size.rows,
            })
            .await;

        let id = self.streams.next_id();
        self.streams
            .shells
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                id,
                Shell {
                    stdin: Arc::new(tokio::sync::Mutex::new(stdin)),
                    resize,
                },
            );
        self.streams.spawn(id, async move {
            let mut buf = vec![0u8; 16 * 1024];
            while let Ok(n) = stdout.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                on_output(buf[..n].to_vec());
            }
            on_exit(status.await.as_ref().and_then(exit_code));
            // Dropping `process` here closes the connection.
            drop(process);
        });
        Ok(id)
    }

    pub async fn shell_write(&self, id: StreamId, data: &[u8]) -> Result<()> {
        let stdin = self.shell(id, |s| Arc::clone(&s.stdin))?;
        let mut stdin = stdin.lock().await;
        stdin
            .write_all(data)
            .await
            .map_err(|e| Error::Kube(e.to_string()))?;
        stdin.flush().await.map_err(|e| Error::Kube(e.to_string()))
    }

    pub async fn shell_resize(&self, id: StreamId, size: Size) -> Result<()> {
        let mut resize = self.shell(id, |s| s.resize.clone())?;
        resize
            .send(TerminalSize {
                width: size.cols,
                height: size.rows,
            })
            .await
            .map_err(|e| Error::Kube(e.to_string()))
    }

    /// Stops a log stream or closes a shell. Unknown ids are ignored: the stream may have ended.
    pub fn stop_stream(&self, id: StreamId) {
        if let Some(task) = self
            .streams
            .tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id)
        {
            task.abort();
        }
        self.streams
            .shells
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
    }

    fn shell<T>(&self, id: StreamId, get: impl FnOnce(&Shell) -> T) -> Result<T> {
        self.streams
            .shells
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .map(get)
            .ok_or(Error::UnknownSession(id))
    }
}

/// Exit code from the status the API server sends when an exec ends.
fn exit_code(status: &Status) -> Option<i32> {
    if status.status.as_deref() == Some("Success") {
        return Some(0);
    }
    status
        .details
        .as_ref()?
        .causes
        .as_ref()?
        .iter()
        .find(|c| c.reason.as_deref() == Some("ExitCode"))?
        .message
        .as_deref()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_from_status() {
        let ok: Status =
            serde_json::from_value(serde_json::json!({ "status": "Success" })).unwrap();
        assert_eq!(exit_code(&ok), Some(0));
        let failed: Status = serde_json::from_value(serde_json::json!({
            "status": "Failure", "reason": "NonZeroExitCode",
            "details": { "causes": [{ "reason": "ExitCode", "message": "130" }] }
        }))
        .unwrap();
        assert_eq!(exit_code(&failed), Some(130));
        let unknown: Status =
            serde_json::from_value(serde_json::json!({ "status": "Failure" })).unwrap();
        assert_eq!(exit_code(&unknown), None);
    }
}
