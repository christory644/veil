//! Live state types for branches, PRs, and directories.
//!
//! These types represent the current real-world state of metadata
//! associated with conversation sessions. Consumed by `veil-aggregator`
//! (cache/resolution) and `veil-ui` (rendering).

use std::fmt;
use std::str::FromStr;

/// Live state of a git branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchState {
    /// Branch exists in the repository.
    Exists,
    /// Branch has been deleted from the repository.
    Deleted,
    /// Could not determine branch state (git unavailable, repo missing, etc.).
    Unknown,
}

impl fmt::Display for BranchState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Exists => "exists",
            Self::Deleted => "deleted",
            Self::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

impl FromStr for BranchState {
    type Err = ();

    /// Parse a cached branch state string. Unrecognised values map to `Unknown`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "exists" => Self::Exists,
            "deleted" => Self::Deleted,
            _ => Self::Unknown,
        })
    }
}

/// Live state of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrState {
    /// PR is open and accepting reviews/changes.
    Open,
    /// PR has been merged.
    Merged,
    /// PR was closed without merging.
    Closed,
    /// Could not determine PR state (gh unavailable, rate limited, etc.).
    Unknown,
}

impl fmt::Display for PrState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

impl FromStr for PrState {
    type Err = ();

    /// Parse a cached PR state string. Unrecognised values map to `Unknown`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "open" => Self::Open,
            "merged" => Self::Merged,
            "closed" => Self::Closed,
            _ => Self::Unknown,
        })
    }
}

/// Live state of a working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirState {
    /// Directory exists on disk.
    Exists,
    /// Directory no longer exists.
    Missing,
}

impl fmt::Display for DirState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Exists => "exists",
            Self::Missing => "missing",
        };
        f.write_str(s)
    }
}

impl FromStr for DirState {
    type Err = ();

    /// Parse a cached directory state string. Unrecognised values map to `Missing`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "exists" => Self::Exists,
            _ => Self::Missing,
        })
    }
}

/// Aggregated live state for a single conversation/session.
///
/// Each field is `Option` because a session may not have a branch,
/// PR, or working directory to check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveStatus {
    /// Live state of the associated branch, if any.
    pub branch: Option<BranchState>,
    /// Live state of the associated PR, if any.
    pub pr: Option<PrState>,
    /// Live state of the working directory.
    pub dir: Option<DirState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ================================================================
    // BranchState
    // ================================================================

    #[test]
    fn branch_state_exists_equals_itself() {
        assert_eq!(BranchState::Exists, BranchState::Exists);
    }

    #[test]
    fn branch_state_deleted_equals_itself() {
        assert_eq!(BranchState::Deleted, BranchState::Deleted);
    }

    #[test]
    fn branch_state_unknown_equals_itself() {
        assert_eq!(BranchState::Unknown, BranchState::Unknown);
    }

    #[test]
    fn branch_state_variants_not_equal() {
        assert_ne!(BranchState::Exists, BranchState::Deleted);
        assert_ne!(BranchState::Exists, BranchState::Unknown);
        assert_ne!(BranchState::Deleted, BranchState::Unknown);
    }

    #[test]
    fn branch_state_display_exists() {
        assert_eq!(BranchState::Exists.to_string(), "exists");
    }

    #[test]
    fn branch_state_display_deleted() {
        assert_eq!(BranchState::Deleted.to_string(), "deleted");
    }

    #[test]
    fn branch_state_display_unknown() {
        assert_eq!(BranchState::Unknown.to_string(), "unknown");
    }

    #[test]
    fn branch_state_clone_preserves_variant() {
        let original = BranchState::Exists;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn branch_state_debug_output_contains_variant_name() {
        let debug_str = format!("{:?}", BranchState::Exists);
        assert!(debug_str.contains("Exists"), "debug output should contain variant name");
    }

    // ================================================================
    // PrState
    // ================================================================

    #[test]
    fn pr_state_open_equals_itself() {
        assert_eq!(PrState::Open, PrState::Open);
    }

    #[test]
    fn pr_state_merged_equals_itself() {
        assert_eq!(PrState::Merged, PrState::Merged);
    }

    #[test]
    fn pr_state_closed_equals_itself() {
        assert_eq!(PrState::Closed, PrState::Closed);
    }

    #[test]
    fn pr_state_unknown_equals_itself() {
        assert_eq!(PrState::Unknown, PrState::Unknown);
    }

    #[test]
    fn pr_state_variants_not_equal() {
        assert_ne!(PrState::Open, PrState::Merged);
        assert_ne!(PrState::Open, PrState::Closed);
        assert_ne!(PrState::Open, PrState::Unknown);
        assert_ne!(PrState::Merged, PrState::Closed);
        assert_ne!(PrState::Merged, PrState::Unknown);
        assert_ne!(PrState::Closed, PrState::Unknown);
    }

    #[test]
    fn pr_state_display_open() {
        assert_eq!(PrState::Open.to_string(), "open");
    }

    #[test]
    fn pr_state_display_merged() {
        assert_eq!(PrState::Merged.to_string(), "merged");
    }

    #[test]
    fn pr_state_display_closed() {
        assert_eq!(PrState::Closed.to_string(), "closed");
    }

    #[test]
    fn pr_state_display_unknown() {
        assert_eq!(PrState::Unknown.to_string(), "unknown");
    }

    #[test]
    fn pr_state_clone_preserves_variant() {
        let original = PrState::Merged;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    // ================================================================
    // BranchState::FromStr
    // ================================================================

    #[test]
    fn branch_state_from_str_exists() {
        assert_eq!("exists".parse::<BranchState>(), Ok(BranchState::Exists));
    }

    #[test]
    fn branch_state_from_str_deleted() {
        assert_eq!("deleted".parse::<BranchState>(), Ok(BranchState::Deleted));
    }

    #[test]
    fn branch_state_from_str_unknown() {
        assert_eq!("unknown".parse::<BranchState>(), Ok(BranchState::Unknown));
    }

    #[test]
    fn branch_state_from_str_unrecognised_maps_to_unknown() {
        assert_eq!("garbage".parse::<BranchState>(), Ok(BranchState::Unknown));
    }

    #[test]
    fn branch_state_display_roundtrips_through_from_str() {
        for variant in [BranchState::Exists, BranchState::Deleted, BranchState::Unknown] {
            let s = variant.to_string();
            assert_eq!(s.parse::<BranchState>(), Ok(variant));
        }
    }

    // ================================================================
    // PrState::FromStr
    // ================================================================

    #[test]
    fn pr_state_from_str_open() {
        assert_eq!("open".parse::<PrState>(), Ok(PrState::Open));
    }

    #[test]
    fn pr_state_from_str_merged() {
        assert_eq!("merged".parse::<PrState>(), Ok(PrState::Merged));
    }

    #[test]
    fn pr_state_from_str_closed() {
        assert_eq!("closed".parse::<PrState>(), Ok(PrState::Closed));
    }

    #[test]
    fn pr_state_from_str_unknown() {
        assert_eq!("unknown".parse::<PrState>(), Ok(PrState::Unknown));
    }

    #[test]
    fn pr_state_from_str_unrecognised_maps_to_unknown() {
        assert_eq!("draft".parse::<PrState>(), Ok(PrState::Unknown));
    }

    #[test]
    fn pr_state_display_roundtrips_through_from_str() {
        for variant in [PrState::Open, PrState::Merged, PrState::Closed, PrState::Unknown] {
            let s = variant.to_string();
            assert_eq!(s.parse::<PrState>(), Ok(variant));
        }
    }

    // ================================================================
    // DirState
    // ================================================================

    #[test]
    fn dir_state_exists_equals_itself() {
        assert_eq!(DirState::Exists, DirState::Exists);
    }

    #[test]
    fn dir_state_missing_equals_itself() {
        assert_eq!(DirState::Missing, DirState::Missing);
    }

    #[test]
    fn dir_state_variants_not_equal() {
        assert_ne!(DirState::Exists, DirState::Missing);
    }

    #[test]
    fn dir_state_display_exists() {
        assert_eq!(DirState::Exists.to_string(), "exists");
    }

    #[test]
    fn dir_state_display_missing() {
        assert_eq!(DirState::Missing.to_string(), "missing");
    }

    #[test]
    fn dir_state_from_str_exists() {
        assert_eq!("exists".parse::<DirState>(), Ok(DirState::Exists));
    }

    #[test]
    fn dir_state_from_str_missing() {
        assert_eq!("missing".parse::<DirState>(), Ok(DirState::Missing));
    }

    #[test]
    fn dir_state_from_str_unrecognised_maps_to_missing() {
        assert_eq!("garbage".parse::<DirState>(), Ok(DirState::Missing));
    }

    #[test]
    fn dir_state_display_roundtrips_through_from_str() {
        for variant in [DirState::Exists, DirState::Missing] {
            let s = variant.to_string();
            assert_eq!(s.parse::<DirState>(), Ok(variant));
        }
    }

    // ================================================================
    // LiveStatus
    // ================================================================

    #[test]
    fn live_status_default_has_all_none() {
        let status = LiveStatus::default();
        assert!(status.branch.is_none());
        assert!(status.pr.is_none());
        assert!(status.dir.is_none());
    }

    #[test]
    fn live_status_with_all_fields_set() {
        let status = LiveStatus {
            branch: Some(BranchState::Exists),
            pr: Some(PrState::Open),
            dir: Some(DirState::Exists),
        };
        assert_eq!(status.branch, Some(BranchState::Exists));
        assert_eq!(status.pr, Some(PrState::Open));
        assert_eq!(status.dir, Some(DirState::Exists));
    }

    #[test]
    fn live_status_mixed_some_none() {
        let status = LiveStatus {
            branch: Some(BranchState::Deleted),
            pr: None,
            dir: Some(DirState::Missing),
        };
        assert_eq!(status.branch, Some(BranchState::Deleted));
        assert!(status.pr.is_none());
        assert_eq!(status.dir, Some(DirState::Missing));
    }

    #[test]
    fn live_status_equality() {
        let a = LiveStatus {
            branch: Some(BranchState::Exists),
            pr: Some(PrState::Open),
            dir: Some(DirState::Exists),
        };
        let b = LiveStatus {
            branch: Some(BranchState::Exists),
            pr: Some(PrState::Open),
            dir: Some(DirState::Exists),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn live_status_not_equal_different_branch() {
        let a = LiveStatus { branch: Some(BranchState::Exists), ..Default::default() };
        let b = LiveStatus { branch: Some(BranchState::Deleted), ..Default::default() };
        assert_ne!(a, b);
    }

    #[test]
    fn live_status_not_equal_some_vs_none() {
        let a = LiveStatus { branch: Some(BranchState::Exists), ..Default::default() };
        let b = LiveStatus::default();
        assert_ne!(a, b);
    }

    #[test]
    fn live_status_clone_preserves_state() {
        let original = LiveStatus {
            branch: Some(BranchState::Exists),
            pr: Some(PrState::Merged),
            dir: Some(DirState::Missing),
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
