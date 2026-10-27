//! `cowmark mcp`: an MCP server over stdio for Claude Desktop and other agents.
//! It edits files in the content folder and hands publishing to a person:
//! `propose_publish` only fills CowMark's save checklist.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, JsonObject, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::{Value, json};

use crate::ops::Context;
use crate::{check, content};

pub const INSTRUCTIONS: &str = "\
This server edits CaterCow's marketing website content: Markdown pages with MDC components and YAML front matter, \
in a git checkout that the CowMark app manages. Paths are relative to the content folder, e.g. `content/blog/my-post.md`.

Workflow:
1. Look around with list_files and read_file. Before writing components or front matter, read the rules with \
list_components / get_component_schema and get_collection_schema. Components use MDC syntax: `::name` … `::` blocks \
with optional YAML props between `---` lines, or `:name{prop=\"value\"}` inline.
2. Create pages with new_page, edit them with write_file (whole-file writes), add pictures with add_image and put the \
returned Markdown in the page. Images live in public/ and are written as site paths starting with `/`, e.g. \
`/blog/my-post/photo.jpg` for `public/blog/my-post/photo.jpg`. Only Markdown and images under content/, and images \
under public/, can be changed.
Table components (pricing-grid, facts-grid, promo-codes) take a Markdown table in their slot, and gallery takes \
Markdown images (one paragraph per row); get_component_schema shows the columns under `input`. csv_to_table turns \
CSV into such a table. `link` props of packages/caterers take a CaterCow search URL, and of reviews a caterer or \
package page URL. write_file stores values in their canonical form (e.g. `$7.25`), so the saved file can differ from \
what you sent; format_file does the same for a file already on disk.
3. Run check on the files you changed and fix every error. Warnings (such as missing alt text) should be fixed too.
4. Optionally open_in_cowmark or preview_link so the person can see the page in the live preview.
5. Call propose_publish with the changed files and a short message. This does NOT publish: it ticks the files in \
CowMark's save checklist. Tell the person to review the changes in CowMark and click \"Save to studio\" themselves.

Never try to publish, commit or push another way. If the person has unsaved edits open in CowMark, writing the file \
is still fine: CowMark merges outside changes into open tabs. Use get_latest to bring in other people's saved changes.";

/// The server; each tool call finds the content folder afresh, so it follows
/// CowMark being opened, closed or set up while the server runs.
#[derive(Clone)]
pub struct CowMarkMcp {
    context: Arc<Context>,
    tools: Arc<Vec<Tool>>,
}

impl CowMarkMcp {
    pub fn new(context: Context) -> Self {
        Self {
            context: Arc::new(context),
            tools: Arc::new(tools()),
        }
    }
}

/// Serves MCP on stdin/stdout until the client disconnects.
pub fn serve_stdio(context: Context) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let service = CowMarkMcp::new(context)
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| format!("MCP start failed: {error}"))?;
        service.waiting().await.map_err(|error| error.to_string())?;
        Ok(())
    })
}

impl ServerHandler for CowMarkMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("cowmark", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if self.get_tool(&request.name).is_none() {
            return Err(ErrorData::invalid_params(
                format!("unknown tool `{}`", request.name),
                None,
            ));
        }
        let context = self.context.clone();
        let name = request.name.to_string();
        let arguments = request.arguments.unwrap_or_default();
        // File and socket work blocks; keep it off the transport's thread.
        let outcome = tokio::task::spawn_blocking(move || call(&context, &name, &arguments))
            .await
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(match outcome {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
        }
        .into())
    }
}

// --- tools -----------------------------------------------------------------------

struct Args<'a>(&'a JsonObject);

impl Args<'_> {
    fn str(&self, name: &str) -> Result<Option<&str>, String> {
        match self.0.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(_) => Err(format!("`{name}` must be a string")),
        }
    }

    fn required(&self, name: &str) -> Result<&str, String> {
        self.str(name)?.ok_or_else(|| format!("`{name}` is required"))
    }

    fn bool(&self, name: &str, default: bool) -> Result<bool, String> {
        match self.0.get(name) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(value)) => Ok(*value),
            Some(_) => Err(format!("`{name}` must be true or false")),
        }
    }

    fn strings(&self, name: &str) -> Result<Vec<String>, String> {
        match self.0.get(name) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("`{name}` must be a list of strings"))
                })
                .collect(),
            Some(Value::String(one)) => Ok(vec![one.clone()]),
            Some(_) => Err(format!("`{name}` must be a list of strings")),
        }
    }
}

fn pretty(value: Value) -> String {
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// Runs one tool; `Err` is shown to the agent as a tool error.
pub fn call(context: &Context, name: &str, arguments: &JsonObject) -> Result<String, String> {
    let args = Args(arguments);
    let root = || context.root().map(|root| root.path);
    match name {
        "read_file" => content::read_file(&root()?, args.required("path")?),
        "list_files" => content::list_files(&root()?, args.str("dir")?, args.str("extension")?, args.bool("recursive", true)?).map(pretty),
        "write_file" => {
            let root = root()?;
            let path = args.required("path")?;
            let text = args.required("content")?;
            // Stored content is canonical: tables, prices, codes and links are formatted first.
            let formatted = crate::root::resolve(&root, path)
                .ok()
                .filter(|(_, relative)| relative.starts_with("content/") && crate::root::is_markdown(relative))
                .and_then(|(_, relative)| {
                    let linter = cowmark_lint::Linter::new(check::load_schemas(&root).ok()?);
                    Some(linter.format(&relative, text, &|_: &str| true))
                })
                .filter(|formatted| formatted.changed);
            let written = content::write_file(
                &root,
                path,
                formatted.as_ref().map_or(text, |formatted| formatted.text.as_str()),
                args.bool("create_only", false)?,
            )?;
            let mut result = json!({ "written": written, "check": check_summary(&root, written["path"].as_str().unwrap_or(path)) });
            if formatted.is_some() {
                result["formatted"] = json!(FORMATTED_NOTE);
            }
            Ok(pretty(result))
        }
        "format_file" => {
            let root = root()?;
            let paths = vec![args.required("path")?.to_string()];
            let changed = check::format_files(&root, &paths, true)?;
            Ok(pretty(json!({
                "path": paths[0],
                "changed": !changed.is_empty(),
                "check": check_summary(&root, &paths[0]),
            })))
        }
        "csv_to_table" => {
            let csv = args.required("csv")?;
            match args.str("component")? {
                None => cowmark_core::csv_table::to_markdown(csv, None),
                Some(component) => {
                    let schemas = check::load_schemas(&root()?)?;
                    let table = cowmark_core::csv_table::for_component(csv, None, component, &schemas, true)?;
                    Ok(if table.problems.is_empty() {
                        table.markdown
                    } else {
                        format!("{}\nProblems to fix in the table:\n{}", table.markdown, table.problems.join("\n"))
                    })
                }
            }
        }
        "rename_file" => content::rename_file(&root()?, args.required("from")?, args.required("to")?).map(pretty),
        "delete_file" => content::delete_file(&root()?, args.required("path")?).map(pretty),
        "list_components" => content::list_components(&root()?).map(pretty),
        "get_component_schema" => content::get_component_schema(&root()?, args.required("name")?).map(pretty),
        "get_collection_schema" => content::get_collection_schema(&root()?, args.str("name")?).map(pretty),
        "new_page" => {
            let fields = match arguments.get("fields") {
                None | Some(Value::Null) => None,
                Some(Value::Object(fields)) => Some(fields),
                Some(_) => return Err("`fields` must be an object".into()),
            };
            content::new_page(
                &root()?,
                args.required("collection")?,
                args.required("title")?,
                args.str("slug")?,
                fields,
                args.str("body")?,
            )
            .map(pretty)
        }
        "add_image" => {
            let source = args.str("source")?.map(expand_home);
            content::add_image(
                &root()?,
                args.required("page")?,
                source.as_deref(),
                args.str("data")?,
                args.str("name")?,
                args.str("alt")?,
            )
            .map(pretty)
        }
        "check" => {
            let report = check::check(&root()?, &args.strings("paths")?)?;
            let strict = args.bool("strict", false)?;
            let verdict = if report.exit_code(strict) == 0 { "PASSED" } else { "FAILED" };
            Ok(format!("{verdict}\n{}", report.to_text()))
        }
        "status" => context.status().map(pretty),
        "get_latest" => context.get_latest().map(|_| {
            "CowMark started Get latest. It runs in the app (open files are saved first and other people's saved \
changes are merged in); call status in a few seconds to see the result, and tell the person if CowMark reports conflicts."
                .to_string()
        }),
        "open_in_cowmark" => context.open(args.required("path")?).map(pretty),
        "preview_link" => context.preview_url(args.str("path")?).map(|url| {
            format!("{url}\n\nThis link shows the page with the unpublished changes in the content folder. It works only on this computer while CowMark is open.")
        }),
        "propose_publish" => {
            let paths = args.strings("paths")?;
            let message = args.required("message")?;
            context.propose(&paths, message).map(|proposal| {
                format!(
                    "{}\n\nNothing has been published. CowMark now shows these files ticked in its save checklist with \
this message. Ask the person to review the changes in CowMark and click \"Save to studio\" to publish them.",
                    pretty(proposal)
                )
            })
        }
        _ => Err(format!("unknown tool `{name}`")),
    }
}

const FORMATTED_NOTE: &str = "Some values were rewritten in their canonical form before saving (table columns, prices like $7.25, \
promo codes, CaterCow links as site paths). read_file shows the saved text.";

/// A short lint result for a file just written, so the agent sees mistakes at once.
fn check_summary(root: &Path, path: &str) -> Value {
    match check::check(root, &[path.to_string()]) {
        Ok(report) if report.diagnostics.is_empty() => json!("no problems found"),
        Ok(report) => json!(report.to_text().lines().map(str::to_string).collect::<Vec<_>>()),
        Err(error) => json!(error),
    }
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map_or_else(|| PathBuf::from(path), |home| PathBuf::from(home).join(rest)),
        None => PathBuf::from(path),
    }
}

struct Hints {
    read_only: bool,
    destructive: bool,
    idempotent: bool,
    open_world: bool,
}

const READ: Hints = Hints {
    read_only: true,
    destructive: false,
    idempotent: true,
    open_world: false,
};

fn tool(name: &'static str, title: &str, description: &'static str, schema: Value, hints: Hints) -> Tool {
    let Value::Object(schema) = schema else {
        unreachable!("tool schemas are objects")
    };
    let annotations = ToolAnnotations::with_title(title)
        .read_only(hints.read_only)
        .destructive(hints.destructive)
        .idempotent(hints.idempotent)
        .open_world(hints.open_world);
    Tool::new(name, description, Arc::new(schema))
        .with_title(title)
        .with_annotations(annotations)
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

const PATH: &str = "Path relative to the content folder, e.g. `content/blog/my-post.md`.";

pub fn tools() -> Vec<Tool> {
    vec![
        tool(
            "list_files",
            "List content files",
            "Lists files in the content folder (default: everything under content/), as paths relative to the folder. Hidden files and .git are left out.",
            object(
                json!({
                    "dir": { "type": "string", "description": "Folder to list, relative to the content folder, e.g. `content/blog`. Default `content`." },
                    "extension": { "type": "string", "description": "Only files with this extension, e.g. `md`." },
                    "recursive": { "type": "boolean", "description": "Include subfolders (default true)." }
                }),
                &[],
            ),
            READ,
        ),
        tool(
            "read_file",
            "Read a file",
            "Reads a text file from the content folder: a page, or schema/components.json, README.md and the like.",
            object(json!({ "path": { "type": "string", "description": PATH } }), &["path"]),
            READ,
        ),
        tool(
            "write_file",
            "Write a page",
            "Writes a whole Markdown file under content/, replacing it if it exists (use create_only to refuse that). Component tables, prices, codes and links are written in their canonical form first (`formatted` says when that changed anything). Returns the check result for the file. Only .md files under content/ can be written; .git and schema/ never.",
            object(
                json!({
                    "path": { "type": "string", "description": PATH },
                    "content": { "type": "string", "description": "The complete file text, front matter included." },
                    "create_only": { "type": "boolean", "description": "Fail instead of replacing an existing file." }
                }),
                &["path", "content"],
            ),
            Hints {
                read_only: false,
                destructive: true,
                idempotent: true,
                open_world: false,
            },
        ),
        tool(
            "format_file",
            "Format a page",
            "Rewrites a Markdown page under content/ in canonical form, the fixes check offers: component tables with canonical column names, order and spacing, prices like $7.25, promo codes in capitals, CaterCow links as site paths, one line per gallery row. Never removes text. Returns whether it changed and the problems left.",
            object(json!({ "path": { "type": "string", "description": PATH } }), &["path"]),
            Hints {
                read_only: false,
                destructive: false,
                idempotent: true,
                open_world: false,
            },
        ),
        tool(
            "csv_to_table",
            "CSV to table",
            "Turns CSV text (or tab-separated text from a spreadsheet) into a Markdown table; the first row is the header. With `component` (a table component such as `pricing-grid`), wraps the table in it and formats it, and lists any values that can't be read. Writes nothing.",
            object(
                json!({
                    "csv": { "type": "string", "description": "The CSV text." },
                    "component": { "type": "string", "description": "A table component to wrap the table in, e.g. `pricing-grid`." }
                }),
                &["csv"],
            ),
            READ,
        ),
        tool(
            "rename_file",
            "Rename a file",
            "Renames or moves a Markdown file or image under content/, or an image under public/. Never replaces an existing file. Moving a page changes its URL; its images in public/ stay where they are, so move them too and update the references.",
            object(
                json!({
                    "from": { "type": "string", "description": PATH },
                    "to": { "type": "string", "description": "New path, relative to the content folder." }
                }),
                &["from", "to"],
            ),
            Hints {
                read_only: false,
                destructive: true,
                idempotent: false,
                open_world: false,
            },
        ),
        tool(
            "delete_file",
            "Delete a file",
            "Deletes one Markdown file or image under content/, or an image under public/. The deletion shows in CowMark's save checklist and is only published when a person saves it.",
            object(json!({ "path": { "type": "string", "description": PATH } }), &["path"]),
            Hints {
                read_only: false,
                destructive: true,
                idempotent: true,
                open_world: false,
            },
        ),
        tool(
            "list_components",
            "List MDC components",
            "Lists the MDC components pages can use (from schema/components.json), with each one's group and purpose. Use get_component_schema for props.",
            object(json!({}), &[]),
            READ,
        ),
        tool(
            "get_component_schema",
            "Get a component's props",
            "The full schema of one MDC component: its props with types, whether required, defaults, options and list item fields, plus any examples. Write props exactly as named here.",
            object(
                json!({ "name": { "type": "string", "description": "Component name as used in Markdown, e.g. `address-cta`." } }),
                &["name"],
            ),
            READ,
        ),
        tool(
            "get_collection_schema",
            "Get front matter fields",
            "The front matter fields for a collection (blog, dishes, cuisines, …): names, types, which are required, and the page URL pattern. Without a name, lists the collections and their folders.",
            object(
                json!({ "name": { "type": "string", "description": "Collection name or folder, e.g. `blog` or `content/blog`." } }),
                &[],
            ),
            READ,
        ),
        tool(
            "new_page",
            "Create a page",
            "Creates `<collection folder>/<slug>.md` with valid front matter: the title, the collection's required fields (dates set to today, others empty unless given in `fields`) and an optional body. Fails if the file exists. Fill any fields it lists as empty, then run check.",
            object(
                json!({
                    "collection": { "type": "string", "description": "Collection name or folder, e.g. `blog`." },
                    "title": { "type": "string", "description": "Page title (under 60 characters is best)." },
                    "slug": { "type": "string", "description": "File name without .md; default: made from the title." },
                    "fields": { "type": "object", "description": "Other front matter fields to set, by name, e.g. {\"description\": \"…\"}." },
                    "body": { "type": "string", "description": "Markdown body after the front matter." }
                }),
                &["collection", "title"],
            ),
            Hints {
                read_only: false,
                destructive: false,
                idempotent: false,
                open_world: false,
            },
        ),
        tool(
            "add_image",
            "Add an image to a page",
            "Copies an image into the page's image folder in public/ (`content/blog/my-post.md` → `public/blog/my-post/`), named in lowercase with hyphens, and returns its site path (`/blog/my-post/<name>`) and a Markdown snippet to put in the page. Give either `source` (a file on this computer) or `data` (base64). PNG, JPEG, GIF, WebP, AVIF and SVG.",
            object(
                json!({
                    "page": { "type": "string", "description": "The page the image belongs to, e.g. `content/blog/my-post.md`." },
                    "source": { "type": "string", "description": "Absolute path of an image file on this computer." },
                    "data": { "type": "string", "description": "The image as base64 (a data: URL is fine)." },
                    "name": { "type": "string", "description": "File name to use, e.g. `team-lunch` (the extension comes from the image)." },
                    "alt": { "type": "string", "description": "Alt text describing the image, for the Markdown snippet." }
                }),
                &["page"],
            ),
            Hints {
                read_only: false,
                destructive: false,
                idempotent: false,
                open_world: false,
            },
        ),
        tool(
            "check",
            "Check content",
            "Checks pages the way CowMark and the site's CI do: MDC components and props, front matter, images, links and leftover merge conflicts. Without paths, checks everything under content/. Errors must be fixed before publishing; warnings should be.",
            object(
                json!({
                    "paths": { "type": "array", "items": { "type": "string" }, "description": "Files or folders to check, relative to the content folder." },
                    "strict": { "type": "boolean", "description": "Count warnings as failures." }
                }),
                &[],
            ),
            READ,
        ),
        tool(
            "status",
            "Changed files",
            "The files changed in the content folder and not yet published, as CowMark's save checklist lists them, plus the branch and how many saved commits wait to be sent.",
            object(json!({}), &[]),
            READ,
        ),
        tool(
            "get_latest",
            "Get latest",
            "Asks CowMark to get other people's published changes (its Get latest button). Local edits are kept; overlapping edits are shown as conflicts in CowMark. Opens CowMark if it isn't running.",
            object(json!({}), &[]),
            Hints {
                read_only: false,
                destructive: false,
                idempotent: true,
                open_world: true,
            },
        ),
        tool(
            "open_in_cowmark",
            "Open in CowMark",
            "Opens a Markdown page in CowMark's editor and live preview, starting CowMark if needed, so the person can see it.",
            object(json!({ "path": { "type": "string", "description": PATH } }), &["path"]),
            Hints {
                read_only: false,
                destructive: false,
                idempotent: true,
                open_world: false,
            },
        ),
        tool(
            "preview_link",
            "Preview link",
            "A link to CowMark's browser preview of the site with the unpublished changes, at the page for `path` when given. Works on this computer while CowMark runs.",
            object(json!({ "path": { "type": "string", "description": PATH } }), &[]),
            READ,
        ),
        tool(
            "propose_publish",
            "Propose publishing",
            "Proposes changed files for publishing: CowMark ticks exactly these files in its save checklist, fills in the message and comes to the front. NOTHING IS PUBLISHED by this tool; a person reviews the changes in CowMark and clicks Save to studio. Every path must have unpublished changes. Run check first.",
            object(
                json!({
                    "paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Changed files to propose, relative to the content folder (include new images)." },
                    "message": { "type": "string", "description": "What changed, in a sentence, e.g. \"New post: holiday party catering\"." }
                }),
                &["paths", "message"],
            ),
            Hints {
                read_only: false,
                destructive: false,
                idempotent: true,
                open_world: false,
            },
        ),
    ]
}
