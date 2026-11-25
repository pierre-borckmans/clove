//! clove: copy-on-write git worktrees.
//!
//! A repository gets one *golden* disk image: a worktree of the base branch with
//! dependencies installed and dev environments warmed. A *clone* is a
//! `clonefile(2)` copy of that image file (O(1), no extra disk until written),
//! mounted and registered as a regular git worktree. Only the setup steps whose
//! inputs changed since the golden commit run in the clone.

pub mod backend;
pub mod brand;
pub mod config;
pub mod detect;
pub mod doctor;
pub mod events;
pub mod git;
pub mod names;
pub mod ops;
pub mod repo;
pub mod steps;
pub mod util;

pub use config::{Config, Exec, Step};
pub use events::{Event, Reporter, Silent, Timing};
pub use ops::{Freshness, GoldenStatus, NewOptions, RefreshOptions, RemoveOptions, RepoStatus};
pub use repo::{CloneInfo, CloneMeta, Clove, GoldenMeta, RepoRecord, known_repos, state_home};
