//! The `cowmark` command: checks CaterCow content with cowmark-lint and talks
//! to the running CowMark app over its control connection (see docs/mdc.md,
//! "CowMark as a hub"). `cowmark mcp` serves the same as MCP tools.

pub mod app;
pub mod check;
pub mod cli;
pub mod content;
pub mod mcp;
pub mod ops;
pub mod root;
