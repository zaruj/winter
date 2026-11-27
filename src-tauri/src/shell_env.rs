//! Apps opened from Finder or the Dock get a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), so
//! kubeconfig credential plugins such as `aws`, `gke-gcloud-auth-plugin` or `kubelogin` are not
//! found. Borrow PATH from the user's login shell, as a terminal would have it.

use std::io::{IsTerminal, Read};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "__WINTER_PATH__";
const TIMEOUT: Duration = Duration::from_secs(3);

/// Must run before any other thread starts, because it sets an environment variable.
pub fn inherit_login_path() {
    // Started from a terminal (e.g. `tauri dev`): PATH is already right.
    if !cfg!(unix) || std::io::stderr().is_terminal() {
        return;
    }
    if let Some(path) = login_shell_path() {
        // SAFETY: called at startup before Tauri or Tokio spawn threads.
        unsafe { std::env::set_var("PATH", path) };
    }
}

fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", &format!("printf '{MARKER}%s{MARKER}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Startup files can hang (prompts, network calls); don't block the app on them.
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }

    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    let path = output.split(MARKER).nth(1)?.trim();
    (!path.is_empty()).then(|| path.to_owned())
}
