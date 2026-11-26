//! clove: copy-on-write git worktrees.
//!
//! A repository gets one *golden* disk image: a worktree of the base branch with
//! dependencies installed and dev environments warmed. A *clone* is a
//! `clonefile(2)` copy of that image file (O(1), no extra disk until written),
//! mounted and registered as a regular git worktree. Only the setup steps whose
//! inputs changed since the golden commit run in the clone. On Linux the golden
//! image and clones are btrfs subvolumes in one image per repo (see `backend`).

pub mod agents;
pub mod backend;
pub mod brand;
pub mod candidates;
pub mod config;
pub mod detect;
pub mod doctor;
pub mod events;
pub mod git;
pub mod history;
pub mod memo;
pub mod names;
pub mod ops;
pub mod preview;
pub mod prune;
pub mod repo;
pub mod service;
pub mod settings;
pub mod shellrc;
pub mod steps;
pub mod update;
pub mod util;
pub mod watch;
pub mod worktrees;

pub use candidates::RepoCandidate;
pub use config::{Config, Exec, Step};
pub use events::{Event, Reporter, Silent, Timing};
pub use ops::{
    BranchCandidate, Freshness, GoldenStatus, NewOptions, RefreshOptions, Removal, RemoveOptions, RepoStatus,
    RepoSummary,
};
pub use preview::{InitPreview, PreviewStep};
pub use prune::{Checks, Cleanup, MergeCheck, Pr, PrState, PruneCache, Verdict, WorktreeCheck};
pub use repo::{CloneInfo, CloneMeta, Clove, GoldenMeta, RepoRecord, known_repos, state_home};
pub use worktrees::{AdoptOptions, OtherWorktree};
