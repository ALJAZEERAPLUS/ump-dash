//! Pure JIRA key extraction from git branch names.
//!
//! This is a pure domain helper — no I/O, no HTTP, no infra dependencies.
//! It was relocated here from `infra::jira` per AUDIT F-107 / F-300 / F-301
//! (Phase 13, Plan 13-01) so `ui/` and `app/` can call it without reaching
//! across the layer boundary into `infra::`.

/// Extracts a JIRA ticket key from a git branch name using the given project prefix.
///
/// Supports branch formats like:
///   - "feature/UMP-1234-some-description"  → Some("UMP-1234")  (with prefix "UMP")
///   - "UMP-5678"                           → Some("UMP-5678")  (with prefix "UMP")
///   - "feature/PROJ-42-thing"              → Some("PROJ-42")   (with prefix "PROJ")
///   - "main"                               → None
///   - "feature/no-ticket"                  → None
///
/// The function splits the branch by `/`, then examines each segment.
/// A segment matches if splitting it by `-` (up to 3 parts) yields
/// `project_prefix` as the first part and an all-ASCII-digits string as the second.
/// No regex crate is required.
pub fn extract_jira_key(branch: &str, project_prefix: &str) -> Option<String> {
    for segment in branch.split('/') {
        let mut parts = segment.splitn(3, '-');
        let first = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let second = match parts.next() {
            Some(v) => v,
            None => continue,
        };

        if first == project_prefix && !second.is_empty() && second.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("{project_prefix}-{second}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_key_from_feature_branch() {
        assert_eq!(
            extract_jira_key("feature/UMP-1234-login", "UMP"),
            Some("UMP-1234".to_string())
        );
    }

    #[test]
    fn extracts_key_from_bare_ticket_segment() {
        assert_eq!(
            extract_jira_key("UMP-5678", "UMP"),
            Some("UMP-5678".to_string())
        );
    }

    #[test]
    fn returns_none_for_main_branch() {
        assert_eq!(extract_jira_key("main", "UMP"), None);
    }

    #[test]
    fn returns_none_for_branch_without_ticket() {
        assert_eq!(extract_jira_key("feature/no-ticket", "UMP"), None);
    }

    #[test]
    fn extracts_key_with_single_digit_number() {
        assert_eq!(
            extract_jira_key("fix/UMP-9-short", "UMP"),
            Some("UMP-9".to_string())
        );
    }

    #[test]
    fn extracts_key_with_custom_prefix() {
        assert_eq!(
            extract_jira_key("feature/PROJ-42-thing", "PROJ"),
            Some("PROJ-42".to_string())
        );
    }
}
