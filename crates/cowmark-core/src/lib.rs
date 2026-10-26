//! CowMark's content-folder logic, usable without Tauri: Google sign-in through
//! the content preview and the session it leaves in the app data dir
//! ([`access`]), git for the managed folder ([`git`]), three-way merges with
//! conflict markers ([`merge`]), the preview's diff and poll ([`preview`]), and
//! finding git ([`git_tool`]).
//!
//! Errors are user-facing strings. Callers match on their prefixes
//! (`conflict:`, `signed_out:`, `not_allowed:`, `offline`), so keep those stable.

pub mod access;
pub mod git;
pub mod git_tool;
pub mod merge;
pub mod preview;
