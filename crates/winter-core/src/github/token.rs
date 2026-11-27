//! Where a GitHub token comes from. In order of preference:
//!
//! 1. one the user saved in winter, kept in the macOS Keychain (never in a file);
//! 2. `GH_TOKEN` or `GITHUB_TOKEN` in the environment;
//! 3. the GitHub CLI's (`gh auth token`).
//!
//! The user may also ask to copy the credential git uses for github.com (`git credential fill`,
//! e.g. from the osxkeychain helper) into winter's Keychain item. That is never done unasked.

use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::{Error, Result};

/// Keychain item: service and account.
#[cfg(target_os = "macos")]
const KEYCHAIN: (&str, &str) = ("dev.winter.github", "api.github.com");

/// Commands that should answer at once, but might wait on a prompt that never shows.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenSource {
    /// Saved in winter (the Keychain).
    Keychain,
    /// `GH_TOKEN` or `GITHUB_TOKEN`.
    Environment,
    /// `gh auth token`.
    GhCli,
}

#[derive(Clone)]
pub struct Token {
    pub value: String,
    pub source: TokenSource,
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the token itself.
        f.debug_struct("Token")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// The first token found, if any.
pub async fn find() -> Option<Token> {
    // May wait for the user: macOS asks before an app it does not recognise (e.g. a new build)
    // reads the item. Off the async workers, so nothing else stalls meanwhile.
    let saved = tokio::task::spawn_blocking(keychain_get)
        .await
        .ok()
        .flatten();
    if let Some(value) = saved {
        return Some(Token {
            value,
            source: TokenSource::Keychain,
        });
    }
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Some(value) = std::env::var(var)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
        {
            return Some(Token {
                value,
                source: TokenSource::Environment,
            });
        }
    }
    let out = run(
        Command::new("gh").args(["auth", "token", "--hostname", "github.com"]),
        None,
    )
    .await
    .ok()?;
    let value = out.trim().to_owned();
    (!value.is_empty()).then_some(Token {
        value,
        source: TokenSource::GhCli,
    })
}

/// The password git's credential helper has for `https://github.com`.
pub async fn from_git() -> Result<String> {
    let out = run(
        Command::new("git")
            .args(["credential", "fill"])
            // Ask the helpers only; never prompt.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS", ""),
        Some("protocol=https\nhost=github.com\n\n"),
    )
    .await
    .map_err(|e| Error::GitHub(format!("git has no credential for github.com ({e})")))?;
    out.lines()
        .find_map(|l| l.strip_prefix("password="))
        .map(str::to_owned)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| Error::GitHub("git has no credential for github.com".into()))
}

/// Runs a command with `input` on stdin; its stdout, or a short error.
async fn run(command: &mut Command, input: Option<&str>) -> std::result::Result<String, String> {
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = tokio::time::timeout(COMMAND_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "timed out".to_owned())?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("exit {}", out.status.code().unwrap_or(-1)));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn keychain_get() -> Option<String> {
    let bytes = security_framework::passwords::get_generic_password(KEYCHAIN.0, KEYCHAIN.1).ok()?;
    String::from_utf8(bytes).ok().filter(|t| !t.is_empty())
}

#[cfg(target_os = "macos")]
pub fn keychain_set(token: &str) -> Result<()> {
    security_framework::passwords::set_generic_password(KEYCHAIN.0, KEYCHAIN.1, token.as_bytes())
        .map_err(|e| Error::GitHub(format!("cannot save the token in the Keychain: {e}")))
}

/// Removes winter's saved token; fine if there was none.
#[cfg(target_os = "macos")]
pub fn keychain_delete() -> Result<()> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN.0, KEYCHAIN.1) {
        Ok(()) => Ok(()),
        // errSecItemNotFound
        Err(e) if e.code() == -25300 => Ok(()),
        Err(e) => Err(Error::GitHub(format!(
            "cannot remove the token from the Keychain: {e}"
        ))),
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_get() -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn keychain_set(_token: &str) -> Result<()> {
    Err(Error::GitHub(
        "saving a token needs the macOS Keychain; set GH_TOKEN instead".into(),
    ))
}

#[cfg(not(target_os = "macos"))]
pub fn keychain_delete() -> Result<()> {
    Ok(())
}
