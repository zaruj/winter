//! Talks to the real GitHub API without a token (60 requests an hour), reading a public
//! repository's Actions. Run with:
//! `cargo test -p winter-core --test live_github -- --ignored --nocapture`
//! Set `WINTER_TEST_GITHUB_REPO=owner/name` to read another public repository.

use winter_core::github::GitHub;

fn repo() -> String {
    std::env::var("WINTER_TEST_GITHUB_REPO").unwrap_or_else(|_| "rust-lang/rustlings".into())
}

#[tokio::test]
#[ignore = "needs network"]
async fn reads_a_public_pipeline() {
    let github = GitHub::new();
    let repo = repo();
    let info = github.repo(&repo).await.unwrap();
    assert!(info.name.eq_ignore_ascii_case(&repo));

    let workflows = github.workflows(&repo).await.unwrap();
    assert!(!workflows.is_empty(), "{repo} has workflows");
    println!(
        "{} workflows: {:?}",
        workflows.len(),
        workflows.iter().map(|w| &w.name).collect::<Vec<_>>()
    );

    let all = github.runs(&repo, None).await.unwrap();
    assert!(!all.runs.is_empty() && all.total >= all.runs.len() as u64);
    // Asking again revalidates by ETag and must give the same answer.
    assert_eq!(github.runs(&repo, None).await.unwrap(), all);

    let one = github
        .runs(&repo, Some(all.runs[0].workflow_id))
        .await
        .unwrap();
    assert!(
        one.runs
            .iter()
            .all(|r| r.workflow_id == all.runs[0].workflow_id)
    );

    let finished = all
        .runs
        .iter()
        .find(|r| r.status == "completed")
        .expect("a finished run");
    println!(
        "run {} {:?} {:?}",
        finished.number, finished.title, finished.conclusion
    );
    let jobs = github.jobs(&repo, finished.id).await.unwrap();
    assert!(!jobs.is_empty());
    for job in &jobs {
        println!(
            "  job {} {:?} ({} steps)",
            job.name,
            job.conclusion,
            job.steps.len()
        );
    }
    // Logs need a token even for public repositories.
    let log = github.job_log(&repo, jobs[0].id).await;
    println!("log without a token: {:?}", log.as_ref().map(|l| l.len()));
}

#[tokio::test]
#[ignore = "needs network"]
async fn missing_repository_says_so() {
    let err = GitHub::new()
        .runs("winter-test-no-such-owner/no-such-repo", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}
