//! GitHub REST responses (only the fields used) and the GUI-friendly views made from them.

use serde::{Deserialize, Serialize};

use super::token::TokenSource;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub source: TokenSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    /// `owner/name`; also the list key.
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    pub language: Option<String>,
    pub default_branch: Option<String>,
    pub pushed_at: Option<String>,
    pub html_url: String,
    /// When it was last opened in winter, ms since the Unix epoch.
    pub last_used: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    /// e.g. `.github/workflows/ci.yml`.
    pub path: String,
    /// `active`, `disabled_manually`, `disabled_inactivity`, ...
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: u64,
    /// The workflow's name.
    pub name: String,
    /// Commit message or PR title, as GitHub shows it.
    pub title: String,
    pub number: u64,
    pub attempt: u64,
    pub workflow_id: u64,
    /// `queued`, `in_progress`, `completed`, `waiting`, `requested` or `pending`.
    pub status: String,
    /// When completed: `success`, `failure`, `cancelled`, `skipped`, `timed_out`, ...
    pub conclusion: Option<String>,
    pub event: String,
    pub branch: Option<String>,
    /// Short commit SHA.
    pub sha: String,
    pub actor: Option<String>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub html_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsPage {
    /// Runs matching, across all pages.
    pub total: u64,
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub number: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub runner: Option<String>,
    pub labels: Vec<String>,
    pub steps: Vec<Step>,
    pub html_url: Option<String>,
}

// --- wire formats ------------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct WireUser {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct WireRepo {
    full_name: String,
    description: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    fork: bool,
    #[serde(default)]
    archived: bool,
    language: Option<String>,
    default_branch: Option<String>,
    pushed_at: Option<String>,
    html_url: String,
}

impl From<WireRepo> for Repo {
    fn from(r: WireRepo) -> Self {
        Self {
            name: r.full_name,
            description: r.description.filter(|d| !d.is_empty()),
            private: r.private,
            fork: r.fork,
            archived: r.archived,
            language: r.language,
            default_branch: r.default_branch,
            pushed_at: r.pushed_at,
            html_url: r.html_url,
            last_used: None,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct WireWorkflows {
    pub workflows: Vec<WireWorkflow>,
}

#[derive(Deserialize)]
pub(super) struct WireWorkflow {
    id: u64,
    name: String,
    path: String,
    state: String,
}

impl From<WireWorkflow> for Workflow {
    fn from(w: WireWorkflow) -> Self {
        Self {
            id: w.id,
            name: w.name,
            path: w.path,
            state: w.state,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct WireRuns {
    pub total_count: u64,
    pub workflow_runs: Vec<WireRun>,
}

#[derive(Deserialize)]
struct WireActor {
    login: String,
}

#[derive(Deserialize)]
pub(super) struct WireRun {
    id: u64,
    name: Option<String>,
    display_title: Option<String>,
    run_number: u64,
    #[serde(default = "one")]
    run_attempt: u64,
    workflow_id: u64,
    status: Option<String>,
    conclusion: Option<String>,
    event: String,
    head_branch: Option<String>,
    head_sha: String,
    actor: Option<WireActor>,
    triggering_actor: Option<WireActor>,
    created_at: Option<String>,
    run_started_at: Option<String>,
    updated_at: Option<String>,
    html_url: String,
}

fn one() -> u64 {
    1
}

impl From<WireRun> for Run {
    fn from(r: WireRun) -> Self {
        Self {
            id: r.id,
            name: r.name.unwrap_or_default(),
            title: r.display_title.unwrap_or_default(),
            number: r.run_number,
            attempt: r.run_attempt,
            workflow_id: r.workflow_id,
            status: r.status.unwrap_or_else(|| "unknown".into()),
            conclusion: r.conclusion,
            event: r.event,
            branch: r.head_branch,
            sha: r.head_sha.chars().take(7).collect(),
            actor: r.triggering_actor.or(r.actor).map(|a| a.login),
            created_at: r.created_at,
            started_at: r.run_started_at,
            updated_at: r.updated_at,
            html_url: r.html_url,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct WireJobs {
    pub jobs: Vec<WireJob>,
}

#[derive(Deserialize)]
pub(super) struct WireJob {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    runner_name: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    steps: Vec<Step>,
    html_url: Option<String>,
}

impl From<WireJob> for Job {
    fn from(j: WireJob) -> Self {
        Self {
            id: j.id,
            name: j.name,
            status: j.status,
            conclusion: j.conclusion,
            // GitHub reports queued jobs as started when they were queued; keep it as given.
            started_at: j.started_at,
            completed_at: j.completed_at,
            runner: j.runner_name.filter(|r| !r.is_empty()),
            labels: j.labels,
            steps: j.steps,
            html_url: j.html_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_run() {
        let json = r#"{
            "id": 42, "name": "CI", "display_title": "Fix the build", "run_number": 7,
            "workflow_id": 3, "status": "completed", "conclusion": "failure", "event": "push",
            "head_branch": "main", "head_sha": "0123456789abcdef",
            "actor": {"login": "someone"}, "triggering_actor": {"login": "rerunner"},
            "html_url": "https://github.com/o/r/actions/runs/42", "extra": [1, 2]
        }"#;
        let run = Run::from(serde_json::from_str::<WireRun>(json).unwrap());
        assert_eq!((run.sha.as_str(), run.attempt), ("0123456", 1));
        assert_eq!(run.actor.as_deref(), Some("rerunner"));
        assert_eq!(run.conclusion.as_deref(), Some("failure"));
    }

    #[test]
    fn reads_a_job_with_steps() {
        let json = r#"{
            "id": 1, "name": "build", "status": "in_progress", "conclusion": null,
            "runner_name": "", "labels": ["ubuntu-latest"],
            "steps": [{"number": 1, "name": "Checkout", "status": "completed",
                       "conclusion": "success", "started_at": null, "completed_at": null}]
        }"#;
        let job = Job::from(serde_json::from_str::<WireJob>(json).unwrap());
        assert_eq!(job.runner, None);
        assert_eq!(job.steps[0].name, "Checkout");
    }
}
