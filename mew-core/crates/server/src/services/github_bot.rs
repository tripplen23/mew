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

use anyhow::{Result, anyhow};

use mewcode_protocol::event::ReviewRequest;
use serde_json::Value;

use crate::AppState;

/// Extract the mention target from a webhook payload: the PR number when
/// the event is a comment mentioning `@mew` (app slug `mewcli`) on an open
/// pull request, else `None`.
///
/// A mention matches `@mew` or `@mewcli` on word boundaries — `@mewbot`,
/// `@mewtwo`, or `user@mew.example` do not trigger a review. Fenced code
/// blocks are ignored, since code samples often contain `@mew`.
///
/// Test surface only (re-exported for integration tests); the webhook
/// route is the only production caller.
#[doc(hidden)]
pub fn mention_request(event: &str, payload: &Value) -> Option<u64> {
    if event != "issue_comment" || payload["action"].as_str() != Some("created") {
        return None;
    }
    let comment = payload["comment"]["body"].as_str()?;
    // `issue_comment` fires for issues and PRs alike; only PRs have a
    // non-null `pull_request` key on the issue object.
    if payload["issue"]["pull_request"].is_null() {
        return None;
    }
    if payload["issue"]["state"].as_str() != Some("open") {
        return None;
    }
    let text = strip_code_fences(comment).to_ascii_lowercase();
    if !has_complete_mention(&text) {
        return None;
    }
    payload["issue"]["number"].as_u64()
}

/// Drop fenced code blocks (` ``` ` spans) from a comment before mention
/// matching, so `@mew` inside code samples does not trigger a review.
fn strip_code_fences(comment: &str) -> String {
    let mut out = String::with_capacity(comment.len());
    let mut in_fence = false;
    for line in comment.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// True when `text` contains `@mew` or `@mewcli` on word boundaries: the
/// character before `@` must not be alphanumeric (kills `user@mew.…`
/// emails) and the character after the mention must not be alphanumeric or
/// `.` (kills `@mewbot`, `@mew.example`).
fn has_complete_mention(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] != b"@mew" {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let after = if bytes.get(i + 4..i + 7) == Some(b"cli") {
            i + 7
        } else {
            i + 4
        };
        let after_ok = match bytes.get(after) {
            None => true,
            Some(b) => !b.is_ascii_alphanumeric() && (after == i + 7 || *b != b'.'),
        };
        if before_ok && after_ok {
            return true;
        }
        i += 1;
    }
    false
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
/// would cause GitHub to redeliver and double review). On failure the
/// author gets an issue comment explaining why, so a silent no-show is
/// never mistaken for "no findings".
pub fn handle_mention(state: AppState, owner: String, repo: String, pr_number: u64) {
    tokio::spawn(async move {
        if let Err(error) = review_pr(&state, &owner, &repo, pr_number).await {
            tracing::error!(%owner, %repo, pr_number, %error, "github review failed");
            report_failure(&state, &owner, &repo, pr_number, &error).await;
        } else {
            tracing::info!(%owner, %repo, pr_number, "github review posted");
        }
    });
}

async fn report_failure(
    state: &AppState,
    owner: &str,
    repo: &str,
    pr_number: u64,
    error: &anyhow::Error,
) {
    let Some(client) = state.github_client.as_ref() else {
        return;
    };
    let token = match client.installation_token(owner).await {
        Ok(token) => token,
        Err(_) => return,
    };
    let body = format!("Mew review failed to run: {error}");
    if let Err(post_error) = client
        .post_issue_comment(&token, owner, repo, pr_number, &body)
        .await
    {
        tracing::error!(%owner, %repo, pr_number, %post_error, "failed to post review-failure comment");
    }
}

async fn review_pr(state: &AppState, owner: &str, repo: &str, pr_number: u64) -> Result<()> {
    let client = state
        .github_client
        .as_ref()
        .ok_or_else(|| anyhow!("github client not configured"))?;
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
