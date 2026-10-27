//! The `cowmark` command line.

use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use crate::ops::Context;
use crate::{check, mcp};

#[derive(Debug, Parser)]
#[command(
    name = "cowmark",
    version,
    about = "Check CaterCow content and hand changes to CowMark for publishing.",
    long_about = "Check CaterCow content and hand changes to CowMark for publishing.\n\n\
The content folder is --root, else the content checkout around the working directory, else CowMark's folder. \
Publishing is always a person clicking Save to studio in CowMark; `propose` only ticks files there."
)]
pub struct Cli {
    /// The content folder (default: found as described above). Also COWMARK_ROOT.
    #[arg(long, global = true, value_name = "FOLDER")]
    pub root: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check pages for component, front matter, image and link problems. Works without CowMark.
    Check {
        /// Files or folders (default: everything under content/).
        paths: Vec<String>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
        /// Fail on warnings too.
        #[arg(long)]
        strict: bool,
        /// Also print errors as GitHub Actions annotations (warnings with --strict).
        #[arg(long)]
        github: bool,
        /// First write fixable values in their canonical form, as `cowmark fmt` does.
        #[arg(long)]
        fix: bool,
    },
    /// Write component tables, prices, codes and links in their canonical form. Works without CowMark.
    Fmt {
        /// Files or folders (default: everything under content/).
        paths: Vec<String>,
        /// Change nothing; exit 1 if any file would change.
        #[arg(long)]
        check: bool,
    },
    /// Turn CSV (or tab-separated text) into a Markdown table. Works without CowMark.
    Table {
        /// The CSV file (default: standard input).
        file: Option<String>,
        /// Wrap the table in this table component and format it, e.g. `pricing-grid`.
        #[arg(long, value_name = "NAME")]
        component: Option<String>,
    },
    /// List changed files that aren't published yet.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Open a page in CowMark (starting it if needed).
    Open { path: String },
    /// Tick files in CowMark's save checklist with a message. Doesn't publish.
    Propose {
        #[arg(required = true)]
        paths: Vec<String>,
        /// What changed.
        #[arg(short, long)]
        message: String,
    },
    /// Print the browser preview's address, at a page's URL when given.
    PreviewUrl { path: Option<String> },
    /// Bring CowMark to the front.
    Focus,
    /// Ask CowMark to get other people's published changes.
    GetLatest,
    /// Run the MCP server on stdin/stdout (for Claude Desktop and other agents).
    Mcp,
    /// Print the Claude Desktop config entry for this cowmark.
    McpConfig,
}

/// Runs a parsed command and returns the exit code: 0 ok, 1 check failed,
/// 2 couldn't run.
pub fn run(cli: Cli, context: &Context, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match execute(cli.command, context, out, err) {
        Ok(code) => code,
        Err(message) => {
            let _ = writeln!(err, "cowmark: {message}");
            2
        }
    }
}

fn execute(command: Command, context: &Context, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32, String> {
    let io = |error: std::io::Error| error.to_string();
    match command {
        Command::Check {
            paths,
            json,
            strict,
            github,
            fix,
        } => {
            let root = context.root()?;
            let paths: Vec<String> = paths.iter().map(|path| cwd_relative(context, path)).collect();
            if fix {
                for file in check::format_files(&root.path, &paths, true)? {
                    writeln!(err, "Fixed {file}").map_err(io)?;
                }
            }
            let report = check::check(&root.path, &paths)?;
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&report.to_json()).unwrap_or_default()
                )
                .map_err(io)?;
            } else {
                write!(out, "{}", report.to_text()).map_err(io)?;
            }
            if github {
                write!(out, "{}", report.to_github(strict)).map_err(io)?;
            }
            Ok(report.exit_code(strict))
        }
        Command::Fmt { paths, check: dry_run } => {
            let root = context.root()?;
            let paths: Vec<String> = paths.iter().map(|path| cwd_relative(context, path)).collect();
            let changed = check::format_files(&root.path, &paths, !dry_run)?;
            let verb = if dry_run { "Would format" } else { "Formatted" };
            for file in &changed {
                writeln!(out, "{verb} {file}").map_err(io)?;
            }
            if changed.is_empty() {
                writeln!(out, "Nothing to format.").map_err(io)?;
            } else if !dry_run {
                writeln!(
                    out,
                    "Run `cowmark check` for anything that couldn't be fixed automatically."
                )
                .map_err(io)?;
            }
            Ok(i32::from(dry_run && !changed.is_empty()))
        }
        Command::Table { file, component } => {
            let text = match file {
                Some(file) => {
                    let path = cwd_relative(context, &file);
                    std::fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?
                }
                None => std::io::read_to_string(std::io::stdin()).map_err(io)?,
            };
            match component {
                None => {
                    writeln!(out, "{}", cowmark_core::csv_table::to_markdown(&text, None)?).map_err(io)?;
                    Ok(0)
                }
                Some(component) => {
                    let root = context.root()?;
                    let schemas = check::load_schemas(&root.path)?;
                    let table = cowmark_core::csv_table::for_component(&text, None, &component, &schemas, true)?;
                    write!(out, "{}", table.markdown).map_err(io)?;
                    for problem in &table.problems {
                        writeln!(err, "{problem}").map_err(io)?;
                    }
                    Ok(i32::from(!table.problems.is_empty()))
                }
            }
        }
        Command::Status { json } => {
            let status = context.status()?;
            if json {
                writeln!(out, "{}", serde_json::to_string_pretty(&status).unwrap_or_default()).map_err(io)?;
            } else {
                write!(out, "{}", status_text(&status)).map_err(io)?;
            }
            Ok(0)
        }
        Command::Open { path } => {
            let opened = context.open(&cwd_relative(context, &path))?;
            writeln!(out, "Opened {} in CowMark.", opened["path"].as_str().unwrap_or(&path)).map_err(io)?;
            Ok(0)
        }
        Command::Propose { paths, message } => {
            let paths: Vec<String> = paths.iter().map(|path| cwd_relative(context, path)).collect();
            let proposal = context.propose(&paths, &message)?;
            let files: Vec<&str> = proposal["paths"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            writeln!(out, "Ticked in CowMark's save checklist:").map_err(io)?;
            for file in files {
                writeln!(out, "  {file}").map_err(io)?;
            }
            writeln!(
                out,
                "Nothing is published until someone clicks Save to studio in CowMark."
            )
            .map_err(io)?;
            Ok(0)
        }
        Command::PreviewUrl { path } => {
            let path = path.map(|path| cwd_relative(context, &path));
            writeln!(out, "{}", context.preview_url(path.as_deref())?).map_err(io)?;
            Ok(0)
        }
        Command::Focus => {
            context.focus()?;
            Ok(0)
        }
        Command::GetLatest => {
            context.get_latest()?;
            writeln!(
                out,
                "CowMark is getting the latest changes; its save bar shows the result."
            )
            .map_err(io)?;
            Ok(0)
        }
        Command::Mcp => {
            mcp::serve_stdio(context.clone())?;
            Ok(0)
        }
        Command::McpConfig => {
            let exe = std::env::current_exe().and_then(|exe| exe.canonicalize()).map_err(io)?;
            writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&mcp_config(&exe.to_string_lossy())).unwrap_or_default()
            )
            .map_err(io)?;
            writeln!(
                err,
                "Add the \"cowmark\" entry above under \"mcpServers\" in Claude Desktop's config ({}), then restart Claude Desktop.",
                claude_config_hint()
            )
            .map_err(io)?;
            Ok(0)
        }
    }
}

/// The Claude Desktop `mcpServers` entry that runs `exe mcp`.
pub fn mcp_config(exe: &str) -> Value {
    json!({ "mcpServers": { "cowmark": { "command": exe, "args": ["mcp"] } } })
}

fn claude_config_hint() -> &'static str {
    if cfg!(windows) {
        r"%APPDATA%\Claude\claude_desktop_config.json; Settings → Developer → Edit Config opens it"
    } else {
        "~/Library/Application Support/Claude/claude_desktop_config.json; Settings → Developer → Edit Config opens it"
    }
}

/// A path typed on the command line: relative to the working directory when
/// it exists there, else relative to the content folder.
fn cwd_relative(context: &Context, path: &str) -> String {
    match &context.cwd {
        Some(cwd) if !std::path::Path::new(path).is_absolute() => {
            let candidate = cwd.join(path);
            if candidate.exists() {
                candidate.to_string_lossy().into_owned()
            } else {
                path.to_string()
            }
        }
        _ => path.to_string(),
    }
}

fn status_text(status: &Value) -> String {
    let changes = status["changes"].as_array().cloned().unwrap_or_default();
    let mut out = format!(
        "{} on {}: {}\n",
        status["folder"].as_str().unwrap_or(""),
        status["branch"].as_str().unwrap_or("?"),
        match changes.len() {
            0 => "no unpublished changes".to_string(),
            1 => "1 changed file".to_string(),
            count => format!("{count} changed files"),
        }
    );
    for change in &changes {
        out.push_str(&format!(
            "  {:<9} {}\n",
            change["status"].as_str().unwrap_or(""),
            change["path"].as_str().unwrap_or("")
        ));
    }
    if let Some(ahead) = status["ahead"].as_u64().filter(|ahead| *ahead > 0) {
        out.push_str(&format!(
            "{ahead} saved change(s) waiting to be sent; Save to studio in CowMark sends them.\n"
        ));
    }
    out
}
