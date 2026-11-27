//! Profile names and regions from `~/.aws/config` and `~/.aws/credentials` (or the files named by
//! `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE`). Only names and settings are read here;
//! secret keys are never returned.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub name: String,
    pub region: Option<String>,
    /// How the profile signs in: `keys`, `sso`, `role`, `process`, or `unknown`.
    pub auth: String,
    /// `AWS_PROFILE` names it, or it is `default` when that is unset.
    pub current: bool,
}

/// Every profile in the shared config and credentials files, `default` first.
pub fn list_profiles() -> Result<Vec<ProfileInfo>> {
    let read = |path: Option<PathBuf>| {
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
    };
    let config = read(env_path("AWS_CONFIG_FILE", "config"));
    let credentials = read(env_path("AWS_SHARED_CREDENTIALS_FILE", "credentials"));
    let current = std::env::var("AWS_PROFILE")
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "default".into());
    Ok(merge(&config, &credentials, &current))
}

fn env_path(var: &str, file: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".aws").join(file)),
    }
}

type Sections = BTreeMap<String, BTreeMap<String, String>>;

fn merge(config: &str, credentials: &str, current: &str) -> Vec<ProfileInfo> {
    // In config, profiles are `[default]` or `[profile name]`; other sections (`[sso-session x]`,
    // `[services x]`) are not profiles. In credentials, every section is a profile.
    let mut profiles: Sections = BTreeMap::new();
    for (section, keys) in parse(config) {
        let name = if section == "default" {
            Some(section.as_str())
        } else {
            section.strip_prefix("profile ").map(str::trim)
        };
        if let Some(name) = name {
            profiles.entry(name.to_owned()).or_default().extend(keys);
        }
    }
    for (section, keys) in parse(credentials) {
        let entry = profiles.entry(section).or_default();
        for (key, value) in keys {
            entry.entry(key).or_insert(value);
        }
    }

    let mut list: Vec<ProfileInfo> = profiles
        .into_iter()
        .map(|(name, keys)| ProfileInfo {
            auth: auth(&keys).into(),
            region: keys.get("region").cloned(),
            current: name == current,
            name,
        })
        .collect();
    list.sort_by_key(|p| p.name != "default");
    list
}

fn auth(keys: &BTreeMap<String, String>) -> &'static str {
    let has = |k: &str| keys.contains_key(k);
    if has("sso_session") || has("sso_start_url") {
        "sso"
    } else if has("role_arn") {
        "role"
    } else if has("credential_process") {
        "process"
    } else if has("aws_access_key_id") {
        "keys"
    } else {
        "unknown"
    }
}

/// Minimal INI: `[section]` headers and `key = value` lines; comments start with `#` or `;`.
/// Indented lines continue the previous key (nested settings such as `s3 =`); they are skipped.
fn parse(text: &str) -> Sections {
    let mut sections: Sections = BTreeMap::new();
    let mut current: Option<String> = None;
    for raw in text.lines() {
        if raw.starts_with([' ', '\t']) {
            continue;
        }
        let line = raw.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim().to_owned();
            sections.entry(name.clone()).or_default();
            current = Some(name);
        } else if let (Some(section), Some((key, value))) = (&current, line.split_once('=')) {
            sections
                .get_mut(section)
                .expect("section was added")
                .insert(key.trim().to_lowercase(), value.trim().to_owned());
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "
[default]
region = ap-southeast-7

[profile work]
sso_session = corp
sso_account_id = 1
region=eu-west-1
s3 =
  max_concurrent_requests = 20

[profile admin]
role_arn = arn:aws:iam::1:role/admin
source_profile = default

[sso-session corp]
sso_start_url = https://example.awsapps.com/start
# a comment
";

    const CREDENTIALS: &str = "
[default]
aws_access_key_id = AKIAEXAMPLE
aws_secret_access_key = secret

[ci]
aws_access_key_id = AKIA2
aws_secret_access_key = secret2
";

    #[test]
    fn merges_config_and_credentials() {
        let list = merge(CONFIG, CREDENTIALS, "work");
        let summary: Vec<_> = list
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.region.as_deref(),
                    p.auth.as_str(),
                    p.current,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("default", Some("ap-southeast-7"), "keys", false),
                ("admin", None, "role", false),
                ("ci", None, "keys", false),
                ("work", Some("eu-west-1"), "sso", true),
            ]
        );
    }

    #[test]
    fn never_returns_secrets() {
        let json = serde_json::to_string(&merge(CONFIG, CREDENTIALS, "default")).unwrap();
        assert!(!json.contains("secret") && !json.contains("AKIA"));
    }

    #[test]
    fn empty_files_give_no_profiles() {
        assert!(merge("", "", "default").is_empty());
    }
}
