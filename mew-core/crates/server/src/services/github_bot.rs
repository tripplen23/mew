//! GitHub bot orchestration — the @mewcli review flow.
//!
//! Consumed by the webhook route (and, later, any other trigger: CLI,
//! scheduled runs). The route verifies the delivery and acks it; this
//! service fetches the PR diff, runs the `review-pr` skill via the shared
//! review path, and posts the findings as a PR review comment.
//!
//! PR content (titles, comments, diffs) is attacker-controlled and only
//! ever passed to the engine as untrusted data (see `build_prompt` in
//! `routes::review`).

use anyhow::Result;
use mewcode_protocol::event::ReviewRequest;
use serde_json::Value;

use crate::AppState;
use crate::github::GithubClient;

/// Extract the mention target from a webhook payload: the PR number when
/// the event is a comment mentioning `@mew` (app slug `mewcli`) on a pull
/// request, else `None`.
///
/// Test surface only (re-exported for integration tests); the webhook
/// route is the only production caller.
#[doc(hidden)]
pub fn mention_request(event: &str, payload: &Value) -> Option<u64> {
    if event != "issue_comment" || payload["action"].as_str() != Some("created") {
        return None;
    }
    let comment = payload["comment"]["body"].as_str()?;
    if !comment.to_ascii_lowercase().contains("@mew") {
        return None;
    }
    // `issue_comment` fires for issues and PRs alike; only PRs have a
    // non-null `pull_request` key on the issue object.
    if payload["issue"]["pull_request"].is_null() {
        return None;
    }
    payload["issue"]["number"].as_u64()
}

/// Resolve the repository of a delivery to `(owner, repo)`.
///
/// Test surface only; the webhook route is the only production caller.
#[doc(hidden)]
pub fn repository(payload: &Value) -> Option<(&str, &str)> {
    let full_name = payload["repository"]["full_name"].as_str()?;
    let (owner, repo) = full_name.split_once('/')?;
    Some((owner, repo))
}

/// Run a review for a mention delivery: ack first, review detached (an
/// LLM review takes longer than GitHub's delivery timeout; a non-2xx
/// would cause GitHub to redeliver and double review). Failures are
/// logged, the delivery has already been acked.
pub fn handle_mention(state: AppState, owner: String, repo: String, pr_number: u64) {
    tokio::spawn(async move {
        match review_pr(&state, &owner, &repo, pr_number).await {
            Ok(()) => tracing::info!(%owner, %repo, pr_number, "github review posted"),
            Err(error) => tracing::error!(%owner, %repo, pr_number, %error, "github review failed"),
        }
    });
}

async fn review_pr(
    state: &AppState,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<()> {
    let client = GithubClient::from_state(state)?;
    let token = client.installation_token(owner).await?;

    let diff = client.pr_diff(&token, owner, repo, pr_number).await?;
    let (status, response) = crate::routes::review::run_review(
        state.clone(),
        ReviewRequest {
            diff,
            extra: Some(format!("PR {owner}/{repo}#{pr_number}")),
        },
    )
    .await;
    anyhow::ensure!(
        status.is_success(),
        "review engine failed: {}",
        response.findings
    );

    client
        .post_review(&token, owner, repo, pr_number, &response.findings)
        .await
}
