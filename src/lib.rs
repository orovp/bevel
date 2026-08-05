//! Spec-driven development harness for coding agents.
//!
//! The design this implements lives in `DESIGN.md`. Two invariants are worth
//! knowing before reading any module:
//!
//! - Judgments the model would otherwise make are exit codes here. Approval,
//!   validation and completion are all decided by this binary, not by prose in
//!   a skill file.
//! - Nothing that describes a project's business logic belongs in the global
//!   layers; those hold the method only.

pub mod affected;
pub mod archive;
pub mod board;
pub mod config;
pub mod context;
pub mod docs;
pub mod gate;
pub mod html;
pub mod http;
pub mod inbox;
pub mod index;
pub mod lifecycle;
pub mod lockfile;
pub mod method;
pub mod migrate;
pub mod packs;
pub mod paths;
pub mod project;
pub mod review;
pub mod shell;
pub mod spec;
pub mod summary;
pub mod sync;
pub mod templates;
pub mod validate;
pub mod verify;
pub mod workspace;

/// Version of this binary, used for `bevel_version` in `gates.lock` and for
/// the version-drift check between machines.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
