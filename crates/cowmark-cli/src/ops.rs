//! What the CLI and the MCP server both do: find the content folder, and
//! ask the running app for status, opening, previewing and proposals.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cowmark_core::schema_source::{self, Loaded, Memo, Request};
use cowmark_lint::Schemas;
use serde_json::{Value, json};

use crate::app::{App, Connection};
use crate::root::{self, ContentRoot};

/// Where the content schema comes from: `--schema-url`, `--schema` and `--offline`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaFlags {
    /// The endpoint; default: the site in the folder's `preview.json`.
    pub url: Option<String>,
    /// A folder with `components.json` / `collections.json`, used instead of everything else.
    pub dir: Option<PathBuf>,
    /// Don't fetch: the cached copy, else the folder's `schema/` files.
    pub offline: bool,
}

/// Where a command runs: an optional `--root`, the working directory and the app.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub root_flag: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub app: Option<App>,
    pub schema: SchemaFlags,
    /// Shared by clones, so the MCP server doesn't fetch the schema on every tool call.
    pub schema_memo: Arc<Memo>,
}

impl Context {
    pub fn from_env(root_flag: Option<PathBuf>) -> Self {
        let root_flag = root_flag.or_else(|| {
            std::env::var_os("COWMARK_ROOT")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        });
        Self {
            root_flag,
            cwd: std::env::current_dir().ok(),
            app: App::from_env(),
            ..Self::default()
        }
    }

    pub fn with_schema(mut self, schema: SchemaFlags) -> Self {
        self.schema = schema;
        self
    }

    /// The content schema for the folder at `root` and where it came from (see
    /// [`cowmark_core::schema_source`]): the site, its cached copy, or the folder's `schema/`.
    pub fn schema(&self, root: &Path) -> Loaded {
        let request = Request {
            url: schema_source::default_url(root, self.schema.url.as_deref(), None),
            offline: self.schema.offline,
            dir: self.schema.dir.clone(),
        };
        schema_source::load_with(root, &request, Some(&self.schema_memo))
    }

    /// The parsed schema; an error when there is none or it can't be read.
    pub fn schemas(&self, root: &Path) -> Result<(Schemas, Loaded), String> {
        let loaded = self.schema(root);
        if !loaded.is_available() {
            return Err(loaded.unavailable_message());
        }
        Ok((loaded.parse()?, loaded))
    }

    pub fn root(&self) -> Result<ContentRoot, String> {
        root::find(self.root_flag.as_deref(), self.cwd.as_deref(), self.app.as_ref())
    }

    fn app(&self) -> Result<&App, String> {
        self.app
            .as_ref()
            .ok_or_else(|| "Couldn't find CowMark's data folder.".to_string())
    }

    /// A connection to CowMark, starting it if needed.
    pub fn connect(&self) -> Result<Connection, String> {
        self.app()?.connect()
    }

    /// `path` made absolute against the content root, so the app resolves it
    /// the same way; absolute paths pass through for the app to check.
    fn absolute(&self, path: &str) -> Result<String, String> {
        if Path::new(path).is_absolute() {
            return Ok(path.to_string());
        }
        let root = self.root()?;
        Ok(root::resolve(&root.path, path)?.0.to_string_lossy().into_owned())
    }

    /// The save checklist's view: from CowMark when it runs on this folder,
    /// else straight from git.
    pub fn status(&self) -> Result<Value, String> {
        let root = self.root()?;
        if let Some(mut connection) = self.app.as_ref().and_then(App::connect_running)
            && app_folder(&connection.ping).as_deref() == Some(root.path.as_path())
        {
            let mut status = connection.call("status", json!({}))?;
            status["folder"] = root.path.to_string_lossy().into();
            status["cowmark"] = true.into();
            return Ok(status);
        }
        let git = cowmark_core::git_tool::require_git()?;
        let status = cowmark_core::git::status(&git, &root.path)?;
        let mut status = serde_json::to_value(status).map_err(|error| error.to_string())?;
        status["folder"] = root.path.to_string_lossy().into();
        status["cowmark"] = false.into();
        Ok(status)
    }

    pub fn open(&self, path: &str) -> Result<Value, String> {
        let absolute = self.absolute(path)?;
        let mut connection = self.connect()?;
        connection.call("open", json!({ "path": absolute }))
    }

    pub fn focus(&self) -> Result<Value, String> {
        self.connect()?.call("focus", json!({}))
    }

    pub fn get_latest(&self) -> Result<Value, String> {
        self.connect()?.call("get_latest", json!({}))
    }

    pub fn propose(&self, paths: &[String], message: &str) -> Result<Value, String> {
        let absolute = paths
            .iter()
            .map(|path| self.absolute(path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut connection = self.connect()?;
        connection.call("propose_publish", json!({ "paths": absolute, "message": message }))
    }

    /// The browser preview's address (with its token), at the page for `path` when given.
    pub fn preview_url(&self, path: Option<&str>) -> Result<String, String> {
        let page = match path {
            Some(path) => Some(self.page_path(path)?),
            None => None,
        };
        let connection = self.connect()?;
        let preview = connection.ping.get("previewUrl").and_then(Value::as_str).ok_or(
            "CowMark's browser preview isn't running. Another program may be using port 4100; CowMark's preview panel says.",
        )?;
        Ok(with_page(preview, page.as_deref()))
    }

    /// The site path for a content file, from its collection's URL pattern.
    pub fn page_path(&self, path: &str) -> Result<String, String> {
        let root = self.root()?;
        let (_, relative) = root::resolve(&root.path, path)?;
        let (schemas, _) = self.schemas(&root.path)?;
        let (collection, slug) = schemas
            .collection_for(&relative)
            .ok_or_else(|| format!("{relative} isn't a page of a collection in the content schema"))?;
        Ok(collection.url_of(&slug))
    }
}

fn app_folder(ping: &Value) -> Option<PathBuf> {
    ping.get("folder")
        .and_then(Value::as_str)
        .and_then(|folder| Path::new(folder).canonicalize().ok())
}

/// `http://localhost:4100/?token=t` → `http://localhost:4100<page>?token=t`.
pub fn with_page(preview: &str, page: Option<&str>) -> String {
    let Some(page) = page.filter(|page| page.starts_with('/')) else {
        return preview.to_string();
    };
    let scheme_end = preview.find("://").map_or(0, |index| index + 3);
    let origin_end = preview[scheme_end..]
        .find('/')
        .map_or(preview.len(), |index| scheme_end + index);
    let token = preview
        .split_once("token=")
        .map(|(_, rest)| rest.split('&').next().unwrap_or(""));
    match token {
        Some(token) => format!("{}{page}?token={token}", &preview[..origin_end]),
        None => format!("{}{page}", &preview[..origin_end]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_urls_keep_the_token() {
        let preview = "http://localhost:4100/?token=abc";
        assert_eq!(with_page(preview, None), preview);
        assert_eq!(
            with_page(preview, Some("/blog/a")),
            "http://localhost:4100/blog/a?token=abc"
        );
        assert_eq!(with_page(preview, Some("blog/a")), preview);
    }
}
