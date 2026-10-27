//! CowMark's content-folder logic, usable without Tauri: Google sign-in through
//! the content preview and the session it leaves in the app data dir
//! ([`access`]), git for the managed folder ([`git`]), three-way merges with
//! conflict markers ([`merge`]), the lint gate on saving ([`content_check`]), the
//! preview's diff and poll ([`preview`]), and finding git ([`git_tool`]). While the app runs it also serves the browser
//! preview ([`preview_server`]) and the control connection the `cowmark`
//! command talks to ([`control`]).
//!
//! Errors are user-facing strings. Callers match on their prefixes
//! (`conflict:`, `lint:`, `signed_out:`, `not_allowed:`, `offline`), so keep those stable.

pub mod access;
pub mod content_check;
pub mod control;
pub mod git;
pub mod git_tool;
pub mod merge;
pub mod preview;
pub mod preview_server;
pub mod token;
