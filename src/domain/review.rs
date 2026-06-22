/// Minimal pull request metadata needed by the review-worktree flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_ref_name: String,
    pub head_ref_oid: String,
    pub url: String,
}

/// GitHub-backed PR filters shown in the review picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestFilter {
    All,
    NotReviewed,
    Mine,
    NotMine,
}

impl PullRequestFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::NotReviewed,
            Self::NotReviewed => Self::Mine,
            Self::Mine => Self::NotMine,
            Self::NotMine => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::NotReviewed => "Not reviewed",
            Self::Mine => "Mine",
            Self::NotMine => "Not Mine",
        }
    }

    pub fn gh_search_query(self) -> &'static str {
        match self {
            Self::All => "is:pr is:open draft:false",
            Self::NotReviewed => "is:pr is:open user-review-requested:@me draft:false",
            Self::Mine => "is:pr is:open author:@me draft:false",
            Self::NotMine => "is:pr is:open -author:@me draft:false",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_filter_cycles_in_picker_order() {
        assert_eq!(
            PullRequestFilter::All.next(),
            PullRequestFilter::NotReviewed
        );
        assert_eq!(
            PullRequestFilter::NotReviewed.next(),
            PullRequestFilter::Mine
        );
        assert_eq!(PullRequestFilter::Mine.next(), PullRequestFilter::NotMine);
        assert_eq!(PullRequestFilter::NotMine.next(), PullRequestFilter::All);
    }

    #[test]
    fn not_reviewed_filter_uses_user_review_requested_query() {
        assert_eq!(
            PullRequestFilter::NotReviewed.gh_search_query(),
            "is:pr is:open user-review-requested:@me draft:false"
        );
    }
}
