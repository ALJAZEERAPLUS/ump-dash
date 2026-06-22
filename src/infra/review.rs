use crate::domain::ports::review_port::ReviewPort;
use crate::domain::review::{PullRequest, PullRequestFilter};
use anyhow::Context;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default)]
pub struct GitHubCliReviewAdapter;

#[async_trait::async_trait]
impl ReviewPort for GitHubCliReviewAdapter {
    async fn list_pull_requests(
        &self,
        repo_root: &Path,
        filter: PullRequestFilter,
    ) -> anyhow::Result<Vec<PullRequest>> {
        list_pull_requests(repo_root, filter).await
    }
}

pub async fn list_pull_requests(
    repo_root: &Path,
    filter: PullRequestFilter,
) -> anyhow::Result<Vec<PullRequest>> {
    let output = tokio::process::Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,author,headRefName,headRefOid,url,isDraft",
            "--search",
            filter.gh_search_query(),
        ])
        .current_dir(repo_root)
        .output()
        .await
        .context("failed to run GitHub CLI (`gh`)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr list failed: {}", stderr.trim());
    }

    parse_pull_requests_json(&output.stdout)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequest {
    number: u64,
    title: String,
    author: GhAuthor,
    head_ref_name: String,
    head_ref_oid: String,
    url: String,
    is_draft: bool,
}

#[derive(Debug, Deserialize)]
struct GhAuthor {
    login: String,
}

pub(crate) fn parse_pull_requests_json(bytes: &[u8]) -> anyhow::Result<Vec<PullRequest>> {
    let prs: Vec<GhPullRequest> =
        serde_json::from_slice(bytes).context("failed to parse gh pr list JSON")?;
    Ok(prs
        .into_iter()
        .filter(|pr| !pr.is_draft)
        .map(|pr| PullRequest {
            number: pr.number,
            title: pr.title,
            author: pr.author.login,
            head_ref_name: pr.head_ref_name,
            head_ref_oid: pr.head_ref_oid,
            url: pr.url,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gh_pr_list_json() {
        let json = br#"[{
            "number": 3832,
            "title": "Review feature",
            "author": { "login": "octocat" },
            "headRefName": "UMP-8868",
            "headRefOid": "0123456789abcdef0123456789abcdef01234567",
            "url": "https://github.com/ALJAZEERAPLUS/ump/pull/3832",
            "isDraft": false
        }]"#;

        let prs = parse_pull_requests_json(json).unwrap();

        assert_eq!(
            prs,
            vec![PullRequest {
                number: 3832,
                title: "Review feature".into(),
                author: "octocat".into(),
                head_ref_name: "UMP-8868".into(),
                head_ref_oid: "0123456789abcdef0123456789abcdef01234567".into(),
                url: "https://github.com/ALJAZEERAPLUS/ump/pull/3832".into(),
            }]
        );
    }

    #[test]
    fn drops_draft_prs_defensively() {
        let json = br#"[{
            "number": 1,
            "title": "Draft",
            "author": { "login": "octocat" },
            "headRefName": "draft-branch",
            "headRefOid": "abc",
            "url": "https://example.invalid/pr/1",
            "isDraft": true
        }]"#;

        assert!(parse_pull_requests_json(json).unwrap().is_empty());
    }
}
