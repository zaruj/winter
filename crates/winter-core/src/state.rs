//! What winter remembers between runs, in `~/.config/winter/state.json` (or
//! `$XDG_CONFIG_HOME/winter/state.json`): the AWS profiles and GitHub repositories opened most
//! recently. Never secrets: the GitHub token lives in the Keychain.
//!
//! The file is small JSON, written whole through a temporary file and a rename so a crash never
//! leaves half a file. Keys this version does not know are kept as they are. A file that cannot be
//! parsed is moved aside to `state.json.bad` rather than overwritten.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Error, Result};

/// Profiles or repositories remembered; older ones are forgotten.
const MAX_RECENT: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default)]
    pub aws: AwsState,
    #[serde(default)]
    pub github: GitHubState,
    #[serde(flatten)]
    other: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubState {
    /// Most recent first.
    #[serde(default)]
    pub recent: Vec<RecentRepo>,
    #[serde(flatten)]
    other: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRepo {
    /// `owner/name`.
    pub repo: String,
    /// Milliseconds since the Unix epoch.
    pub used_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwsState {
    /// Most recent first.
    #[serde(default)]
    pub recent: Vec<RecentProfile>,
    #[serde(flatten)]
    other: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProfile {
    pub profile: String,
    /// Region it was last opened in.
    pub region: String,
    /// Milliseconds since the Unix epoch.
    pub used_at: u64,
}

/// `$XDG_CONFIG_HOME/winter/state.json`, else `~/.config/winter/state.json`.
pub fn path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("winter").join("state.json"))
}

/// The saved state; empty when there is none yet or it cannot be read.
pub fn load() -> State {
    path().map(|p| read(&p)).unwrap_or_default()
}

/// Remembers that `profile` was just opened in `region`.
pub fn record_aws_profile(profile: &str, region: &str) -> Result<()> {
    let path = path().ok_or_else(|| Error::State("no home directory".into()))?;
    update(&path, |state| state.aws.used(profile, region, now_ms()))
}

/// Remembers that the repository `owner/name` was just opened.
pub fn record_github_repo(repo: &str) -> Result<()> {
    let path = path().ok_or_else(|| Error::State("no home directory".into()))?;
    update(&path, |state| state.github.used(repo, now_ms()))
}

impl GitHubState {
    fn used(&mut self, repo: &str, at: u64) {
        self.recent.retain(|r| !r.repo.eq_ignore_ascii_case(repo));
        self.recent.insert(
            0,
            RecentRepo {
                repo: repo.to_owned(),
                used_at: at,
            },
        );
        self.recent.truncate(MAX_RECENT);
    }
}

impl AwsState {
    fn used(&mut self, profile: &str, region: &str, at: u64) {
        self.recent.retain(|r| r.profile != profile);
        self.recent.insert(
            0,
            RecentProfile {
                profile: profile.to_owned(),
                region: region.to_owned(),
                used_at: at,
            },
        );
        self.recent.truncate(MAX_RECENT);
    }
}

/// Serializes writers within this process, so two quick updates cannot lose one another.
static WRITE: Mutex<()> = Mutex::new(());

fn update(path: &Path, change: impl FnOnce(&mut State)) -> Result<()> {
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = read(path);
    change(&mut state);
    write(path, &state)
}

fn read(path: &Path) -> State {
    let Ok(text) = std::fs::read_to_string(path) else {
        return State::default();
    };
    match serde_json::from_str(&text) {
        Ok(state) => state,
        Err(_) => {
            // Keep what was there for the user to look at; start again.
            let _ = std::fs::rename(path, path.with_extension("json.bad"));
            State::default()
        }
    }
}

fn write(path: &Path, state: &State) -> Result<()> {
    let fail = |e: std::io::Error| Error::State(format!("{}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let mut text = serde_json::to_string_pretty(state).expect("state serializes");
    text.push('\n');
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(fail)?;
    std::fs::rename(&temp, path).map_err(fail)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("winter-state-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn most_recent_first_without_duplicates() {
        let mut aws = AwsState::default();
        aws.used("a", "us-east-1", 1);
        aws.used("b", "eu-west-1", 2);
        aws.used("a", "ap-southeast-7", 3);
        let order: Vec<_> = aws
            .recent
            .iter()
            .map(|r| (r.profile.as_str(), r.region.as_str(), r.used_at))
            .collect();
        assert_eq!(order, [("a", "ap-southeast-7", 3), ("b", "eu-west-1", 2)]);
    }

    #[test]
    fn writes_creates_the_directory_and_keeps_unknown_keys() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("winter").join("state.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"theme":"dark","aws":{"pinned":["x"]}}"#).unwrap();

        update(&path, |s| s.aws.used("pp", "ap-southeast-7", 5)).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "dark");
        assert_eq!(saved["aws"]["pinned"][0], "x");
        assert_eq!(saved["aws"]["recent"][0]["profile"], "pp");
        assert_eq!(saved["aws"]["recent"][0]["usedAt"], 5);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_broken_file_is_moved_aside() {
        let dir = temp_dir("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        std::fs::write(&path, "{not json").unwrap();

        update(&path, |s| s.aws.used("pp", "us-east-1", 1)).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.bad")).unwrap(),
            "{not json"
        );
        assert_eq!(read(&path).aws.recent[0].profile, "pp");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repos_are_remembered_case_insensitively() {
        let mut github = GitHubState::default();
        github.used("zaruj/winter", 1);
        github.used("cli/cli", 2);
        github.used("Zaruj/Winter", 3);
        let order: Vec<_> = github.recent.iter().map(|r| r.repo.as_str()).collect();
        assert_eq!(order, ["Zaruj/Winter", "cli/cli"]);
    }

    #[test]
    fn missing_file_is_empty() {
        assert_eq!(
            read(Path::new("/nonexistent/winter/state.json")),
            State::default()
        );
    }
}
