//! Local terminal sessions: a shell running in a pseudo-terminal.
//!
//! Output is read on a dedicated thread (pty I/O is blocking) and handed to a callback as raw
//! bytes; decoding is left to the terminal emulator, which copes with UTF-8 split across reads.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::{Error, Result};

pub type SessionId = u32;

/// The program a session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

impl ShellCommand {
    /// The user's login shell (`$SHELL -l`) started in their home directory.
    pub fn login() -> Self {
        Self {
            program: std::env::var("SHELL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/bin/zsh".into()),
            args: vec!["-l".into()],
            cwd: std::env::var_os("HOME").map(PathBuf::from),
        }
    }

    fn builder(&self) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(&self.program);
        cmd.args(&self.args);
        if let Some(cwd) = &self.cwd {
            cmd.cwd(cwd);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "winter");
        // Apps opened from Finder have no locale, which breaks non-ASCII input in most shells.
        if cmd.get_env("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        cmd
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl From<Size> for PtySize {
    fn from(size: Size) -> Self {
        PtySize {
            cols: size.cols.max(1),
            rows: size.rows.max(1),
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

/// Open terminal sessions. Dropping it kills every shell it started.
#[derive(Default)]
pub struct Terminals {
    sessions: Arc<Mutex<HashMap<SessionId, Session>>>,
    next_id: AtomicU32,
}

impl Terminals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts `shell` in a new pseudo-terminal.
    ///
    /// `on_output` receives everything the shell prints. When the shell exits, the session is
    /// removed and `on_exit` gets its exit code (`None` if it could not be read).
    pub fn open(
        &self,
        shell: &ShellCommand,
        size: Size,
        mut on_output: impl FnMut(&[u8]) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<SessionId> {
        let pair = native_pty_system()
            .openpty(size.into())
            .map_err(terminal_error)?;
        let mut child = pair
            .slave
            .spawn_command(shell.builder())
            .map_err(terminal_error)?;
        // Only the child may hold the slave side, so reads see EOF once the shell exits.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(terminal_error)?;
        let writer = pair.master.take_writer().map_err(terminal_error)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        lock(&self.sessions).insert(
            id,
            Session {
                master: pair.master,
                writer,
                killer: child.clone_killer(),
            },
        );

        let sessions = Arc::clone(&self.sessions);
        std::thread::Builder::new()
            .name(format!("terminal-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => on_output(&buf[..n]),
                    }
                }
                let code = child.wait().ok().map(|status| status.exit_code());
                lock(&sessions).remove(&id);
                on_exit(code);
            })
            .map_err(|e| Error::Terminal(e.to_string()))?;
        Ok(id)
    }

    /// Sends keyboard input to the shell.
    pub fn write(&self, id: SessionId, data: &[u8]) -> Result<()> {
        let mut sessions = lock(&self.sessions);
        let session = sessions.get_mut(&id).ok_or(Error::UnknownSession(id))?;
        session.writer.write_all(data).map_err(terminal_error)?;
        session.writer.flush().map_err(terminal_error)
    }

    pub fn resize(&self, id: SessionId, size: Size) -> Result<()> {
        let sessions = lock(&self.sessions);
        let session = sessions.get(&id).ok_or(Error::UnknownSession(id))?;
        session.master.resize(size.into()).map_err(terminal_error)
    }

    /// Kills the shell. `on_exit` still fires from the reader thread. Closing a session that has
    /// already exited is not an error.
    pub fn close(&self, id: SessionId) -> Result<()> {
        if let Some(session) = lock(&self.sessions).get_mut(&id) {
            session.killer.kill().map_err(terminal_error)?;
        }
        Ok(())
    }

    pub fn is_open(&self, id: SessionId) -> bool {
        lock(&self.sessions).contains_key(&id)
    }
}

impl Drop for Terminals {
    fn drop(&mut self) {
        for session in lock(&self.sessions).values_mut() {
            let _ = session.killer.kill();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock leaves the map itself intact, so keep going.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn terminal_error(error: impl std::fmt::Display) -> Error {
    Error::Terminal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    const SIZE: Size = Size { cols: 80, rows: 24 };
    const WAIT: Duration = Duration::from_secs(10);

    fn sh(script: &str) -> ShellCommand {
        ShellCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            cwd: None,
        }
    }

    /// Opens a session and returns channels for its output and exit code.
    fn open(
        terminals: &Terminals,
        shell: &ShellCommand,
    ) -> (
        SessionId,
        mpsc::Receiver<Vec<u8>>,
        mpsc::Receiver<Option<u32>>,
    ) {
        let (out_tx, out_rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let id = terminals
            .open(
                shell,
                SIZE,
                move |bytes| {
                    let _ = out_tx.send(bytes.to_vec());
                },
                move |code| {
                    let _ = exit_tx.send(code);
                },
            )
            .unwrap();
        (id, out_rx, exit_rx)
    }

    /// Collects output until it contains `needle`.
    fn read_until(rx: &mpsc::Receiver<Vec<u8>>, needle: &str) -> String {
        let mut text = String::new();
        while !text.contains(needle) {
            let chunk = rx
                .recv_timeout(WAIT)
                .unwrap_or_else(|_| panic!("no {needle:?} in {text:?}"));
            text.push_str(&String::from_utf8_lossy(&chunk));
        }
        text
    }

    #[test]
    fn runs_input_and_reports_exit() {
        let terminals = Terminals::new();
        let shell = sh("printf 'ready\\n'; read line; echo \"got:$line\"; exit 3");
        let (id, out, exit) = open(&terminals, &shell);

        read_until(&out, "ready");
        terminals.write(id, "hello\r".as_bytes()).unwrap();
        read_until(&out, "got:hello");

        assert_eq!(exit.recv_timeout(WAIT).unwrap(), Some(3));
        assert!(!terminals.is_open(id));
        assert_eq!(terminals.write(id, b"x"), Err(Error::UnknownSession(id)));
    }

    #[test]
    fn resize_is_seen_by_the_shell() {
        let terminals = Terminals::new();
        let shell = sh("printf 'ready\\n'; read _; stty size");
        let (id, out, _exit) = open(&terminals, &shell);

        read_until(&out, "ready");
        terminals
            .resize(
                id,
                Size {
                    cols: 100,
                    rows: 30,
                },
            )
            .unwrap();
        terminals.write(id, b"\r").unwrap();
        read_until(&out, "30 100");
    }

    #[test]
    fn close_kills_the_shell() {
        let terminals = Terminals::new();
        let (id, out, exit) = open(&terminals, &sh("printf 'ready\\n'; sleep 60"));

        read_until(&out, "ready");
        terminals.close(id).unwrap();
        exit.recv_timeout(WAIT).expect("shell did not exit");
        assert!(!terminals.is_open(id));
        terminals.close(id).unwrap();
    }

    #[test]
    fn login_shell_uses_home() {
        let shell = ShellCommand::login();
        assert_eq!(shell.args, vec!["-l".to_string()]);
        assert_eq!(shell.cwd, std::env::var_os("HOME").map(PathBuf::from));
    }
}
