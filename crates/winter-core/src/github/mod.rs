//! GitHub: sign in with a token, list the repositories it can see, and follow their GitHub
//! Actions pipelines: workflows, runs, jobs with their steps, and job logs. Read-only.
//!
//! Requests carry the ETag of the last answer for the same URL, so polling a pipeline that has
//! not changed gets `304 Not Modified`, which GitHub does not count against the rate limit.

mod token;
mod types;

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::StatusCode;
use reqwest::header::{ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, LINK, USER_AGENT};
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, RwLock};

use token::Token;
pub use token::TokenSource;
pub use types::{Account, Job, Repo, Run, RunsPage, Step, Workflow};
use types::{WireJobs, WireRepo, WireRun, WireRuns, WireUser, WireWorkflows};

use crate::{Error, Result};

const API: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Answers remembered for their ETags.
const CACHE_ENTRIES: usize = 256;
/// Runs fetched per request.
pub const RUNS_PER_PAGE: u32 = 50;
/// The end of a job log that is kept; logs can run to many megabytes.
const LOG_TAIL: usize = 2 * 1024 * 1024;

pub struct GitHub {
    http: reqwest::Client,
    token: RwLock<Option<Token>>,
    /// Who the token belongs to, once checked.
    account: RwLock<Option<Account>>,
    /// URL → (ETag, body).
    cache: Mutex<HashMap<String, (String, String)>>,
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHub {
    pub fn new() -> Self {
        crate::tls::init();
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("the TLS provider is installed");
        Self {
            http,
            token: RwLock::new(None),
            account: RwLock::new(None),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Signs in with the first token found (see [`token`]); None when there is none. Once signed
    /// in, answers from memory unless `fresh`: reading the Keychain can mean a macOS prompt.
    pub async fn connect(&self, fresh: bool) -> Result<Option<Account>> {
        if !fresh && let Some(account) = self.account.read().await.clone() {
            return Ok(Some(account));
        }
        let Some(found) = token::find().await else {
            *self.token.write().await = None;
            *self.account.write().await = None;
            return Ok(None);
        };
        let source = found.source;
        let user = self.user(&found).await.map_err(|e| match source {
            TokenSource::Keychain => Error::GitHub(format!("the token saved in winter: {e}")),
            TokenSource::Environment => Error::GitHub(format!("GH_TOKEN / GITHUB_TOKEN: {e}")),
            TokenSource::GhCli => Error::GitHub(format!("the GitHub CLI's token: {e}")),
        })?;
        let account = account(user, source);
        self.use_token(found, account.clone()).await;
        Ok(Some(account))
    }

    /// Checks `token`, saves it in the Keychain and signs in with it.
    pub async fn sign_in(&self, token: &str) -> Result<Account> {
        let token = Token {
            value: token.trim().to_owned(),
            source: TokenSource::Keychain,
        };
        if token.value.is_empty() {
            return Err(Error::GitHub("the token is empty".into()));
        }
        let user = self.user(&token).await?;
        token::keychain_set(&token.value)?;
        let account = account(user, TokenSource::Keychain);
        self.use_token(token, account.clone()).await;
        Ok(account)
    }

    /// Copies the credential git uses for github.com into the Keychain and signs in with it.
    pub async fn sign_in_with_git(&self) -> Result<Account> {
        let value = token::from_git().await?;
        self.sign_in(&value).await
    }

    /// Forgets the token saved in winter. A token from the environment or the GitHub CLI is still
    /// found by the next [`connect`](Self::connect).
    pub async fn sign_out(&self) -> Result<()> {
        token::keychain_delete()?;
        *self.token.write().await = None;
        *self.account.write().await = None;
        self.cache.lock().await.clear();
        Ok(())
    }

    /// Repositories the token can see (own, collaborator and organization), most recently pushed
    /// first, page by page. Each notes when it was last opened in winter.
    pub async fn repos(
        &self,
        mut on_page: impl FnMut(Vec<Repo>) -> ControlFlow<()>,
    ) -> Result<usize> {
        let recent = crate::state::load().github.recent;
        let mut url = format!(
            "{API}/user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member"
        );
        let mut total = 0;
        loop {
            let (repos, next) = self.get_page::<Vec<WireRepo>>(&url).await?;
            let repos: Vec<Repo> = repos
                .into_iter()
                .map(|r| {
                    let mut repo = Repo::from(r);
                    repo.last_used = recent
                        .iter()
                        .find(|u| u.repo.eq_ignore_ascii_case(&repo.name))
                        .map(|u| u.used_at);
                    repo
                })
                .collect();
            total += repos.len();
            if on_page(repos).is_break() {
                break;
            }
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        Ok(total)
    }

    /// One repository by `owner/name`, which need not be in [`repos`](Self::repos) (any public
    /// repository works).
    pub async fn repo(&self, repo: &str) -> Result<Repo> {
        let repo = checked(repo)?;
        let found: WireRepo = self.get(&format!("{API}/repos/{repo}")).await?;
        Ok(Repo::from(found))
    }

    pub async fn workflows(&self, repo: &str) -> Result<Vec<Workflow>> {
        let repo = checked(repo)?;
        let found: WireWorkflows = self
            .get(&format!(
                "{API}/repos/{repo}/actions/workflows?per_page=100"
            ))
            .await?;
        Ok(found.workflows.into_iter().map(Workflow::from).collect())
    }

    /// The latest runs, of one workflow or all of them.
    pub async fn runs(&self, repo: &str, workflow: Option<u64>) -> Result<RunsPage> {
        let repo = checked(repo)?;
        let url = match workflow {
            Some(id) => {
                format!("{API}/repos/{repo}/actions/workflows/{id}/runs?per_page={RUNS_PER_PAGE}")
            }
            None => format!("{API}/repos/{repo}/actions/runs?per_page={RUNS_PER_PAGE}"),
        };
        let found: WireRuns = self.get(&url).await?;
        Ok(RunsPage {
            total: found.total_count,
            runs: found.workflow_runs.into_iter().map(Run::from).collect(),
        })
    }

    pub async fn run(&self, repo: &str, id: u64) -> Result<Run> {
        let repo = checked(repo)?;
        let found: WireRun = self
            .get(&format!("{API}/repos/{repo}/actions/runs/{id}"))
            .await?;
        Ok(Run::from(found))
    }

    /// Jobs of a run's latest attempt, with their steps.
    pub async fn jobs(&self, repo: &str, run: u64) -> Result<Vec<types::Job>> {
        let repo = checked(repo)?;
        let found: WireJobs = self
            .get(&format!(
                "{API}/repos/{repo}/actions/runs/{run}/jobs?per_page=100&filter=latest"
            ))
            .await?;
        Ok(found.jobs.into_iter().map(Job::from).collect())
    }

    /// A job's log (its last 2 MiB). GitHub has it once the job has finished.
    pub async fn job_log(&self, repo: &str, job: u64) -> Result<String> {
        let repo = checked(repo)?;
        let token = self.token.read().await.clone();
        let response = self
            .request(
                &format!("{API}/repos/{repo}/actions/jobs/{job}/logs"),
                token.as_ref(),
            )
            .send()
            .await
            .map_err(http_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::GitHub(
                "no log yet: GitHub makes it available once the job has finished".into(),
            ));
        }
        let response = check(response).await?;
        let bytes = response.bytes().await.map_err(http_error)?;
        let start = bytes.len().saturating_sub(LOG_TAIL);
        let mut text = String::from_utf8_lossy(&bytes[start..]).into_owned();
        if start > 0 {
            // Drop the partial first line.
            if let Some(newline) = text.find('\n') {
                text.drain(..=newline);
            }
        }
        Ok(text)
    }

    async fn use_token(&self, token: Token, account: Account) {
        *self.token.write().await = Some(token);
        *self.account.write().await = Some(account);
        self.cache.lock().await.clear();
    }

    async fn user(&self, token: &Token) -> Result<WireUser> {
        let response = self
            .request(&format!("{API}/user"), Some(token))
            .send()
            .await
            .map_err(http_error)?;
        check(response).await?.json().await.map_err(http_error)
    }

    fn request(&self, url: &str, token: Option<&Token>) -> reqwest::RequestBuilder {
        let request = self
            .http
            .get(url)
            .header(USER_AGENT, "winter")
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION);
        match token {
            Some(t) => request.header(AUTHORIZATION, format!("Bearer {}", t.value)),
            None => request,
        }
    }

    async fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(self.get_page(url).await?.0)
    }

    /// GETs JSON, revalidating a cached answer by its ETag; also the `next` page's URL, if any.
    async fn get_page<T: DeserializeOwned>(&self, url: &str) -> Result<(T, Option<String>)> {
        let token = self.token.read().await.clone();
        let cached = self.cache.lock().await.get(url).cloned();
        let mut request = self.request(url, token.as_ref());
        if let Some((etag, _)) = &cached {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(http_error)?;
        let next = response
            .headers()
            .get(LINK)
            .and_then(|l| l.to_str().ok())
            .and_then(next_link);

        let body = if response.status() == StatusCode::NOT_MODIFIED
            && let Some((_, body)) = cached
        {
            body
        } else {
            let response = check(response).await?;
            let etag = response
                .headers()
                .get(ETAG)
                .and_then(|e| e.to_str().ok())
                .map(str::to_owned);
            let body = response.text().await.map_err(http_error)?;
            if let Some(etag) = etag {
                let mut cache = self.cache.lock().await;
                if cache.len() >= CACHE_ENTRIES {
                    cache.clear();
                }
                cache.insert(url.to_owned(), (etag, body.clone()));
            }
            body
        };
        let value = serde_json::from_str(&body)
            .map_err(|e| Error::GitHub(format!("unexpected answer from GitHub: {e}")))?;
        Ok((value, next))
    }
}

fn account(user: WireUser, source: TokenSource) -> Account {
    Account {
        login: user.login,
        name: user.name.filter(|n| !n.is_empty()),
        avatar_url: user.avatar_url,
        source,
    }
}

/// `owner/name`, made only of the characters GitHub allows, so it is safe in a URL path.
fn checked(repo: &str) -> Result<&str> {
    let ok = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repo.split_once('/') {
        Some((owner, name)) if ok(owner) && ok(name) => Ok(repo),
        _ => Err(Error::GitHub(format!(
            "\"{repo}\" is not an owner/name repository"
        ))),
    }
}

/// The `rel="next"` URL of a `Link` header.
fn next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        rel.contains("rel=\"next\"").then(|| {
            url.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        })
    })
}

/// Turns an error status into a message that says what to do.
async fn check(response: reqwest::Response) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let remaining = header("x-ratelimit-remaining");
    let reset = header("x-ratelimit-reset").and_then(|r| r.parse::<u64>().ok());
    let message = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("message")?.as_str().map(str::to_owned))
        .unwrap_or_default();
    let text = match status {
        StatusCode::UNAUTHORIZED => "GitHub rejected the token (expired or revoked?)".to_owned(),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS if remaining.as_deref() == Some("0") => {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let minutes = reset.map_or(0, |r| r.saturating_sub(now).div_ceil(60));
            format!("GitHub's rate limit is used up; it resets in {minutes} min")
        }
        StatusCode::FORBIDDEN => format!("not allowed: {message} (does the token have access?)"),
        StatusCode::NOT_FOUND => {
            "not found, or the token cannot see it (private repositories need the `repo` scope or fine-grained access)".to_owned()
        }
        _ if message.is_empty() => format!("GitHub answered {status}"),
        _ => format!("GitHub answered {status}: {message}"),
    };
    Err(Error::GitHub(text))
}

fn http_error(error: reqwest::Error) -> Error {
    Error::GitHub(crate::error::describe(&error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_must_be_owner_slash_name() {
        assert!(checked("zaruj/winter").is_ok());
        assert!(checked("my-org/repo.name_2").is_ok());
        for bad in [
            "winter", "a/b/c", "../x", "a/..", "a/b?x=1", "/b", "a/", "a/b c",
        ] {
            assert!(checked(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn finds_the_next_page() {
        let header = r#"<https://api.github.com/user/repos?page=2>; rel="next", <https://api.github.com/user/repos?page=5>; rel="last""#;
        assert_eq!(
            next_link(header).as_deref(),
            Some("https://api.github.com/user/repos?page=2")
        );
        assert_eq!(next_link(r#"<https://x?page=1>; rel="prev""#), None);
    }
}
