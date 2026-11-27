import { Channel, invoke } from "@tauri-apps/api/core";

// Typed wrappers for the GitHub commands in src-tauri/src/commands.rs.
// Commands reject with an error message string.

export type TokenSource = "keychain" | "environment" | "ghCli";

export interface Account {
  login: string;
  name: string | null;
  avatarUrl: string | null;
  source: TokenSource;
}

export interface Repo {
  /** `owner/name`. */
  name: string;
  description: string | null;
  private: boolean;
  fork: boolean;
  archived: boolean;
  language: string | null;
  defaultBranch: string | null;
  pushedAt: string | null;
  htmlUrl: string;
  /** Last opened in winter, ms since the epoch. */
  lastUsed: number | null;
}

export interface Workflow {
  id: number;
  name: string;
  path: string;
  state: string;
}

export interface Run {
  id: number;
  /** The workflow's name. */
  name: string;
  /** Commit message or PR title. */
  title: string;
  number: number;
  attempt: number;
  workflowId: number;
  /** `queued`, `in_progress`, `completed`, `waiting`, `requested` or `pending`. */
  status: string;
  conclusion: string | null;
  event: string;
  branch: string | null;
  sha: string;
  actor: string | null;
  createdAt: string | null;
  startedAt: string | null;
  updatedAt: string | null;
  htmlUrl: string;
}

export interface Step {
  number: number;
  name: string;
  status: string;
  conclusion: string | null;
  startedAt: string | null;
  completedAt: string | null;
}

export interface Job {
  id: number;
  name: string;
  status: string;
  conclusion: string | null;
  startedAt: string | null;
  completedAt: string | null;
  runner: string | null;
  labels: string[];
  steps: Step[];
  htmlUrl: string | null;
}

/** Signed-in account, from memory once known; `fresh` looks for a token again. */
export const connect = (fresh = false): Promise<Account | null> =>
  invoke("github_connect", { fresh });
export const signIn = (token: string): Promise<Account> => invoke("github_sign_in", { token });
export const signInWithGit = (): Promise<Account> => invoke("github_sign_in_with_git");
export const signOut = (): Promise<void> => invoke("github_sign_out");

export function streamRepos(onPage: (repos: Repo[]) => void): Promise<number> {
  return invoke("github_repos", { onPage: new Channel<Repo[]>(onPage) });
}

/** One repository; also marks it as recently opened. */
export const getRepo = (repo: string): Promise<Repo> => invoke("github_repo", { repo });
export const listWorkflows = (repo: string): Promise<Workflow[]> =>
  invoke("github_workflows", { repo });
export const listRuns = (
  repo: string,
  workflow: number | null,
): Promise<{ total: number; runs: Run[] }> => invoke("github_runs", { repo, workflow });
export const getRun = (repo: string, id: number): Promise<Run> =>
  invoke("github_run", { repo, id });
export const listJobs = (repo: string, run: number): Promise<Job[]> =>
  invoke("github_jobs", { repo, run });
export const getJobLog = (repo: string, job: number): Promise<string> =>
  invoke("github_job_log", { repo, job });
export const openUrl = (url: string): Promise<void> => invoke("open_github_url", { url });

/** Still going: worth polling. */
export const isActive = (status: string) => status !== "completed";
