//! Where the content schema (`components` and `collections`) comes from. The site serves the
//! schema its own build generated at `<site>/_cowmark/schema.json` (`{ format: 1, build,
//! components, collections }`), so CowMark, `cowmark check` and the MCP server read what the site
//! renders with. First match wins:
//!
//! 1. a fresh fetch of that URL, written to a cache file in the folder's git dir;
//! 2. the cached copy, when the fetch fails (offline), with its age;
//! 3. the folder's own `schema/components.json` and `schema/collections.json` (legacy, until the
//!    content repository stops carrying them);
//! 4. nothing.
//!
//! An explicit schema folder (`cowmark --schema <dir>`) replaces all of these.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cowmark_lint::Schemas;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The endpoint's path on the site.
pub const ENDPOINT_PATH: &str = "/_cowmark/schema.json";
/// The envelope format this build understands; a higher one is a breaking change.
pub const FORMAT: u64 = 1;
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// A cached copy older than this is worth telling the person about.
pub const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Overrides where cached copies are kept (tests, read-only checkouts).
pub const CACHE_DIR_ENV: &str = "COWMARK_SCHEMA_CACHE_DIR";
const MAX_BODY: usize = 8 * 1024 * 1024;
const REPO_FILES: [&str; 2] = ["schema/components.json", "schema/collections.json"];

/// The two schema texts, as `Schemas::from_json` takes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaTexts {
    pub components: Option<String>,
    pub collections: Option<String>,
}

impl SchemaTexts {
    pub fn is_empty(&self) -> bool {
        self.components.is_none() && self.collections.is_none()
    }

    fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        for text in [&self.components, &self.collections] {
            match text {
                Some(text) => {
                    hasher.update(b"1");
                    hasher.update(text.len().to_le_bytes());
                    hasher.update(text.as_bytes());
                }
                None => hasher.update(b"0"),
            }
        }
        hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// Which source the schema came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Origin {
    /// Fetched just now (or within the memo's window) from `url`.
    Site { url: String },
    /// The copy cached when `url` was last fetched, `age_secs` ago.
    #[serde(rename_all = "camelCase")]
    Cache { url: String, age_secs: u64 },
    /// An explicit schema folder.
    Dir { path: String },
    /// The content folder's own `schema/` files.
    Repo,
    None,
}

/// The schema texts and where they came from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    #[serde(skip)]
    pub texts: SchemaTexts,
    pub origin: Origin,
    /// Why a better source wasn't used, e.g. a failed fetch. Plain sentences.
    pub warnings: Vec<String>,
    /// Changes exactly when the texts do.
    pub hash: String,
    /// The endpoint tried (or that would be tried), if any.
    pub url: Option<String>,
}

impl Loaded {
    fn new(texts: SchemaTexts, origin: Origin, warnings: Vec<String>, url: Option<String>) -> Self {
        let hash = texts.hash();
        Self { texts, origin, warnings, hash, url }
    }

    pub fn is_available(&self) -> bool {
        self.origin != Origin::None
    }

    /// A cached copy older than [`STALE_AFTER`].
    pub fn is_stale(&self) -> bool {
        matches!(self.origin, Origin::Cache { age_secs, .. } if age_secs > STALE_AFTER.as_secs())
    }

    /// Parsed; a text that can't be read is an error.
    pub fn parse(&self) -> Result<Schemas, String> {
        Schemas::from_json(self.texts.components.as_deref(), self.texts.collections.as_deref())
            .map_err(|error| format!("{error} ({})", self.describe()))
    }

    /// Parsed leniently: a text that can't be read is left out, as if absent, so a broken
    /// `components.json` still leaves front matter, images and links checked.
    pub fn schemas(&self) -> Schemas {
        lenient(&self.texts)
    }

    /// One line for people, e.g. `schema: https://…/_cowmark/schema.json`.
    pub fn describe(&self) -> String {
        match &self.origin {
            Origin::Site { url } => format!("schema: {url}"),
            Origin::Cache { url, age_secs } => {
                format!("schema: cached copy of {url} from {}", age_text(*age_secs))
            }
            Origin::Dir { path } => format!("schema: {path}"),
            Origin::Repo => "schema: the folder's schema/ files (legacy)".into(),
            Origin::None => "schema: none".into(),
        }
    }

    /// A note for results built from the schema when it may be out of date, else None.
    pub fn note(&self) -> Option<String> {
        let source = match &self.origin {
            Origin::Cache { .. } if self.is_stale() => self.describe(),
            Origin::Repo if !self.warnings.is_empty() => self.describe(),
            Origin::None => return Some(self.unavailable_message()),
            _ => return None,
        };
        Some(format!(
            "The component list may be out of date ({source}). {}",
            self.warnings.join(" ")
        )
        .trim_end()
        .to_string())
    }

    /// Why there is no schema, for a command that needs one.
    pub fn unavailable_message(&self) -> String {
        let mut message = match &self.url {
            Some(url) => format!("No content schema: couldn't get {url}, and the folder has no schema/ files."),
            None => "No content schema: no site to fetch it from (no --schema-url, no preview.json), and the folder has no schema/ files.".to_string(),
        };
        if !self.warnings.is_empty() {
            message.push(' ');
            message.push_str(&self.warnings.join(" "));
        }
        message.push_str(" Pass --schema-url <url> or --schema <folder>.");
        message
    }

    /// For the app: what the editor needs, with the texts.
    pub fn to_json(&self) -> Value {
        json!({
            "components": self.texts.components,
            "collections": self.texts.collections,
            "origin": self.origin,
            "warnings": self.warnings,
            "hash": self.hash,
            "url": self.url,
            "stale": self.is_stale(),
            "description": self.describe(),
        })
    }
}

fn lenient(texts: &SchemaTexts) -> Schemas {
    let mut components = texts.components.as_deref();
    let mut collections = texts.collections.as_deref();
    loop {
        match Schemas::from_json(components, collections) {
            Ok(schemas) => return schemas,
            Err(error) if error.file == "components.json" && components.is_some() => components = None,
            Err(_) if collections.is_some() => collections = None,
            Err(_) => return Schemas::default(),
        }
    }
}

fn age_text(secs: u64) -> String {
    let plural = |count: u64, unit: &str| format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" });
    match secs {
        0..60 => "just now".into(),
        60..3_600 => plural(secs / 60, "minute"),
        3_600..86_400 => plural(secs / 3_600, "hour"),
        _ => plural(secs / 86_400, "day"),
    }
}

// --- the site --------------------------------------------------------------------

/// The endpoint for a site: `https://x.example.com/anything` → `https://x.example.com/_cowmark/schema.json`.
pub fn endpoint_for_site(site: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(site.trim()).map_err(|error| format!("{site} isn't a URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{site} isn't an http(s) site"));
    }
    url.join(ENDPOINT_PATH).map(String::from).map_err(|error| error.to_string())
}

/// The site in the folder's `preview.json` (`{ "site": "https://…" }`), if any.
pub fn preview_site(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("preview.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let site = value.get("site")?.as_str()?.trim();
    endpoint_for_site(site).ok().map(|_| site.to_string())
}

/// The endpoint to use: an explicit one, else the folder's `preview.json` site, else `fallback_site`.
pub fn default_url(root: &Path, explicit: Option<&str>, fallback_site: Option<&str>) -> Option<String> {
    if let Some(url) = explicit.map(str::trim).filter(|url| !url.is_empty()) {
        return Some(url.to_string());
    }
    preview_site(root)
        .as_deref()
        .or(fallback_site)
        .and_then(|site| endpoint_for_site(site).ok())
}

/// Reads the endpoint's envelope. Unknown keys are ignored; a missing or higher `format` and
/// texts the linter can't read are refused, so a bad deploy falls back to the last good copy.
pub fn parse_envelope(body: &str) -> Result<SchemaTexts, String> {
    let value: Value = serde_json::from_str(body).map_err(|error| format!("the response isn't JSON ({error})"))?;
    let format = value
        .get("format")
        .and_then(Value::as_u64)
        .ok_or("the response has no schema format")?;
    if format > FORMAT {
        return Err(format!(
            "the site serves schema format {format}, newer than this CowMark understands ({FORMAT}); update CowMark"
        ));
    }
    if format < 1 {
        return Err(format!("the site serves an unknown schema format {format}"));
    }
    let text = |key: &str| -> Result<Option<String>, String> {
        match value.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(part) => serde_json::to_string(part).map(Some).map_err(|error| error.to_string()),
        }
    };
    let texts = SchemaTexts {
        components: text("components")?,
        collections: text("collections")?,
    };
    if texts.is_empty() {
        return Err("the response has no components or collections".into());
    }
    Schemas::from_json(texts.components.as_deref(), texts.collections.as_deref())
        .map_err(|error| error.to_string().replace("schema/", "the served "))?;
    Ok(texts)
}

/// GETs `url` with `timeout`. Runs on its own thread so it works from async code too.
pub fn fetch(url: &str, timeout: Duration) -> Result<String, String> {
    let url = url.to_string();
    let worker = std::thread::spawn(move || -> Result<String, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .user_agent(concat!("cowmark/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| error.to_string())?;
        let response = client
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    return format!("no answer within {}s", timeout.as_secs());
                }
                // reqwest's own message only names the URL; the cause is at the end of the chain.
                let mut cause: &dyn std::error::Error = &error;
                while let Some(source) = cause.source() {
                    cause = source;
                }
                cause.to_string()
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the site answered {status}"));
        }
        let bytes = response.bytes().map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BODY {
            return Err("the response is too large".into());
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| "the response isn't UTF-8".to_string())
    });
    worker.join().map_err(|_| "the fetch failed".to_string())?
}

// --- the cache -------------------------------------------------------------------

/// The folder's git dir: `.git`, or where a `.git` file (`gitdir: …`) points.
fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.lines().find_map(|line| line.strip_prefix("gitdir:"))?.trim();
    let path = root.join(target);
    path.is_dir().then_some(path)
}

/// Where cached copies live: [`CACHE_DIR_ENV`], else `<git dir>/cowmark`, so nothing tracked changes.
pub fn cache_dir(root: &Path) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(CACHE_DIR_ENV).filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    git_dir(root).map(|dir| dir.join("cowmark"))
}

fn cache_file(dir: &Path, url: &str) -> PathBuf {
    let digest = Sha256::digest(url.as_bytes());
    let name: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
    dir.join(format!("schema-{name}.json"))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|now| now.as_secs()).unwrap_or(0)
}

/// Writes the fetched envelope with its URL and time. Failure only loses the offline copy.
pub fn write_cache(root: &Path, url: &str, body: &str, fetched_at: u64) -> Result<(), String> {
    let dir = cache_dir(root).ok_or("the folder has no git dir to cache the schema in")?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let envelope: Value = serde_json::from_str(body).map_err(|error| error.to_string())?;
    let file = json!({ "url": url, "fetchedAt": fetched_at, "envelope": envelope });
    let path = cache_file(&dir, url);
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec(&file).map_err(|error| error.to_string())?)
        .and_then(|()| std::fs::rename(&temp, &path))
        .map_err(|error| error.to_string())
}

struct Cached {
    url: String,
    fetched_at: u64,
    texts: SchemaTexts,
}

fn read_cache_file(path: &Path) -> Option<Cached> {
    let value: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let texts = parse_envelope(&value.get("envelope")?.to_string()).ok()?;
    Some(Cached {
        url: value.get("url")?.as_str()?.to_string(),
        fetched_at: value.get("fetchedAt")?.as_u64()?,
        texts,
    })
}

/// The cached copy for `url`, or without one the most recently fetched copy of any URL.
fn read_cache(root: &Path, url: Option<&str>) -> Option<Cached> {
    let dir = cache_dir(root)?;
    if let Some(url) = url {
        return read_cache_file(&cache_file(&dir, url)).filter(|cached| cached.url == url);
    }
    std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("schema-") && name.ends_with(".json")
        })
        .filter_map(|entry| read_cache_file(&entry.path()))
        .max_by_key(|cached| cached.fetched_at)
}

// --- loading ---------------------------------------------------------------------

/// What to load from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Request {
    /// The endpoint (see [`default_url`]); None leaves the site out.
    pub url: Option<String>,
    /// Don't fetch; use the cache or the folder's files.
    pub offline: bool,
    /// An explicit schema folder with `components.json` / `collections.json`; replaces the rest.
    pub dir: Option<PathBuf>,
}

fn read_dir_texts(dir: &Path) -> SchemaTexts {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    SchemaTexts {
        components: read("components.json"),
        collections: read("collections.json"),
    }
}

fn read_repo_texts(root: &Path) -> SchemaTexts {
    let read = |file: &str| std::fs::read_to_string(root.join(file)).ok();
    SchemaTexts {
        components: read(REPO_FILES[0]),
        collections: read(REPO_FILES[1]),
    }
}

/// Loads the schema in the order described in the module docs.
pub fn load(root: &Path, request: &Request) -> Loaded {
    load_with(root, request, None)
}

/// [`load`], remembering fetches in `memo`.
pub fn load_with(root: &Path, request: &Request, memo: Option<&Memo>) -> Loaded {
    if let Some(dir) = &request.dir {
        let texts = read_dir_texts(dir);
        let origin = if texts.is_empty() { Origin::None } else { Origin::Dir { path: dir.display().to_string() } };
        let mut warnings = Vec::new();
        if texts.is_empty() {
            warnings.push(format!("{} has no components.json or collections.json.", dir.display()));
        }
        return Loaded::new(texts, origin, warnings, None);
    }

    let mut warnings = Vec::new();
    let url = request.url.clone();
    if let Some(url) = &url {
        if !request.offline {
            match fetch_remembered(root, url, memo) {
                Ok(texts) => return Loaded::new(texts, Origin::Site { url: url.clone() }, Vec::new(), Some(url.clone())),
                Err(error) => warnings.push(format!("Couldn't get the schema from {url}: {error}.")),
            }
        }
    }
    if let Some(cached) = read_cache(root, url.as_deref()) {
        let age_secs = now_secs().saturating_sub(cached.fetched_at);
        let origin = Origin::Cache { url: cached.url.clone(), age_secs };
        return Loaded::new(cached.texts, origin, warnings, url);
    }
    let texts = read_repo_texts(root);
    if !texts.is_empty() {
        return Loaded::new(texts, Origin::Repo, warnings, url);
    }
    Loaded::new(SchemaTexts::default(), Origin::None, warnings, url)
}

/// Fetches, validates and caches; with a memo, reuses a recent result instead of asking again.
fn fetch_remembered(root: &Path, url: &str, memo: Option<&Memo>) -> Result<SchemaTexts, String> {
    if let Some(remembered) = memo.and_then(|memo| memo.recent(url)) {
        return remembered;
    }
    let result = fetch(url, memo.map_or(FETCH_TIMEOUT, |memo| memo.timeout))
        .and_then(|body| parse_envelope(&body).map(|texts| (texts, body)));
    let result = match result {
        Ok((texts, body)) => {
            // Without a git dir there's just no offline copy.
            let _ = write_cache(root, url, &body, now_secs());
            Ok(texts)
        }
        Err(error) => Err(error),
    };
    if let Some(memo) = memo {
        memo.remember(url, &result);
    }
    result
}

type FetchResult = Result<SchemaTexts, String>;

/// Remembers fetches for a while, so callers that ask often (each MCP tool call, each window
/// focus) don't hit the site every time, and an unreachable site isn't retried on every call.
#[derive(Debug)]
pub struct Memo {
    /// How long a good fetch is reused.
    pub fresh_for: Duration,
    /// How long after a failed fetch to wait before trying again.
    pub retry_after: Duration,
    pub timeout: Duration,
    entries: Mutex<HashMap<String, (Instant, FetchResult)>>,
}

impl Default for Memo {
    fn default() -> Self {
        Self::new(Duration::from_secs(10 * 60), Duration::from_secs(60))
    }
}

impl Memo {
    pub fn new(fresh_for: Duration, retry_after: Duration) -> Self {
        Self { fresh_for, retry_after, timeout: FETCH_TIMEOUT, entries: Mutex::default() }
    }

    fn recent(&self, url: &str) -> Option<Result<SchemaTexts, String>> {
        let entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let (at, result) = entries.get(url)?;
        let window = if result.is_ok() { self.fresh_for } else { self.retry_after };
        (at.elapsed() < window).then(|| result.clone())
    }

    fn remember(&self, url: &str, result: &Result<SchemaTexts, String>) {
        let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.insert(url.to_string(), (Instant::now(), result.clone()));
    }

    /// Forgets everything, so the next load fetches.
    pub fn clear(&self) {
        self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const COMPONENTS: &str = r#"{"components":[{"name":"banner","props":[]}]}"#;
    const COLLECTIONS: &str = r#"{"collections":[{"name":"blog","folder":"content/blog","url":"/blog/<file name without .md>","fields":[]}]}"#;

    pub(crate) fn envelope(format: u64, component: &str) -> String {
        format!(
            r#"{{"format":{format},"build":{{"revision":null}},"extra":true,"components":{{"components":[{{"name":"{component}","props":[]}}]}},"collections":{COLLECTIONS}}}"#
        )
    }

    /// Serves `body` with `status` to every request until dropped; counts requests.
    pub(crate) struct Server {
        pub url: String,
        pub hits: Arc<AtomicUsize>,
        body: Arc<Mutex<(String, String)>>,
    }

    impl Server {
        pub(crate) fn start(status: &str, body: &str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://127.0.0.1:{}{ENDPOINT_PATH}", listener.local_addr().unwrap().port());
            let hits = Arc::new(AtomicUsize::new(0));
            let body = Arc::new(Mutex::new((status.to_string(), body.to_string())));
            let (thread_hits, thread_body) = (hits.clone(), body.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buffer = [0u8; 4096];
                    let mut request = Vec::new();
                    while let Ok(read) = stream.read(&mut buffer) {
                        request.extend_from_slice(&buffer[..read]);
                        if read == 0 || request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    thread_hits.fetch_add(1, Ordering::SeqCst);
                    let (status, body) = thread_body.lock().unwrap().clone();
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            Self { url, hits, body }
        }

        pub(crate) fn set(&self, status: &str, body: &str) {
            *self.body.lock().unwrap() = (status.to_string(), body.to_string());
        }
    }

    /// A URL nothing listens on.
    pub(crate) fn dead_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{port}{ENDPOINT_PATH}")
    }

    fn folder(name: &str) -> tempfile::TempDir {
        let dir = tempfile::Builder::new().prefix(&format!("cowmark-schema-{name}")).tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        dir
    }

    fn component_names(loaded: &Loaded) -> Vec<String> {
        loaded.schemas().components.into_iter().map(|component| component.name).collect()
    }

    #[test]
    fn site_urls_and_preview_json() {
        assert_eq!(
            endpoint_for_site("https://content-preview.catercow.com").unwrap(),
            "https://content-preview.catercow.com/_cowmark/schema.json"
        );
        assert_eq!(
            endpoint_for_site("https://x.example.com/blog/a?b=1").unwrap(),
            "https://x.example.com/_cowmark/schema.json"
        );
        assert!(endpoint_for_site("file:///etc").is_err());

        let dir = folder("preview");
        assert_eq!(default_url(dir.path(), None, None), None);
        assert_eq!(
            default_url(dir.path(), None, Some("https://app.example.com")).as_deref(),
            Some("https://app.example.com/_cowmark/schema.json")
        );
        std::fs::write(dir.path().join("preview.json"), r#"{"site":"https://content-preview.catercow.com"}"#).unwrap();
        assert_eq!(
            default_url(dir.path(), None, Some("https://app.example.com")).as_deref(),
            Some("https://content-preview.catercow.com/_cowmark/schema.json")
        );
        assert_eq!(
            default_url(dir.path(), Some("http://localhost:3000/_cowmark/schema.json"), None).as_deref(),
            Some("http://localhost:3000/_cowmark/schema.json")
        );
    }

    #[test]
    fn envelopes_are_validated() {
        let texts = parse_envelope(&envelope(1, "banner")).unwrap();
        let schemas = Schemas::from_json(texts.components.as_deref(), texts.collections.as_deref()).unwrap();
        assert_eq!(schemas.components[0].name, "banner");
        assert_eq!(schemas.collections.len(), 1);

        assert!(parse_envelope(&envelope(2, "banner")).unwrap_err().contains("newer than this CowMark understands"));
        assert!(parse_envelope(r#"{"components":[]}"#).unwrap_err().contains("no schema format"));
        assert!(parse_envelope("<html>").unwrap_err().contains("isn't JSON"));
        assert!(parse_envelope(r#"{"format":1}"#).is_err());
        assert!(parse_envelope(r#"{"format":1,"components":{"components":"nope"}}"#).is_err());
    }

    #[test]
    fn fetches_then_falls_back_to_the_cache_then_the_folder() {
        let dir = folder("order");
        let root = dir.path();
        std::fs::create_dir_all(root.join("schema")).unwrap();
        std::fs::write(root.join("schema/components.json"), COMPONENTS).unwrap();

        // Nothing to fetch from: the folder's files.
        let loaded = load(root, &Request::default());
        assert_eq!(loaded.origin, Origin::Repo);
        assert!(loaded.warnings.is_empty() && loaded.note().is_none());
        assert_eq!(component_names(&loaded), ["banner"]);

        // The site wins, and is cached in the git dir.
        let server = Server::start("200 OK", &envelope(1, "from-site"));
        let request = Request { url: Some(server.url.clone()), ..Request::default() };
        let loaded = load(root, &request);
        assert_eq!(loaded.origin, Origin::Site { url: server.url.clone() });
        assert_eq!(component_names(&loaded), ["from-site"]);
        assert!(loaded.note().is_none());
        assert_eq!(std::fs::read_dir(root.join(".git/cowmark")).unwrap().count(), 1);

        // Offline: the cache, with its age.
        let offline = load(root, &Request { offline: true, ..request.clone() });
        assert!(matches!(offline.origin, Origin::Cache { age_secs, .. } if age_secs < 60), "{:?}", offline.origin);
        assert_eq!(offline.hash, loaded.hash);
        assert!(offline.warnings.is_empty());
        // Without a URL, the newest cached copy.
        assert!(matches!(load(root, &Request { offline: true, ..Request::default() }).origin, Origin::Cache { .. }));

        // The site fails: the cache, with the reason.
        server.set("500 Internal Server Error", "oops");
        let failed = load(root, &request);
        assert!(matches!(failed.origin, Origin::Cache { .. }));
        assert!(failed.warnings[0].contains("500"), "{:?}", failed.warnings);
        assert_eq!(component_names(&failed), ["from-site"]);

        // A newer format than this build knows: refused the same way, the cache stays.
        server.set("200 OK", &envelope(2, "future"));
        let future = load(root, &request);
        assert!(matches!(future.origin, Origin::Cache { .. }));
        assert!(future.warnings[0].contains("format 2"), "{:?}", future.warnings);

        // A cache for another URL isn't used for this one: the folder's files.
        let other = Request { url: Some(dead_url()), ..Request::default() };
        let loaded = load(root, &other);
        assert_eq!(loaded.origin, Origin::Repo);
        assert!(loaded.warnings[0].to_lowercase().contains("refused"), "{:?}", loaded.warnings);
        assert!(loaded.note().unwrap().contains("may be out of date"));

        // Nothing at all.
        std::fs::remove_dir_all(root.join("schema")).unwrap();
        let none = load(root, &other);
        assert_eq!(none.origin, Origin::None);
        assert!(!none.is_available());
        assert!(none.unavailable_message().contains("no schema/ files"), "{}", none.unavailable_message());
    }

    #[test]
    fn an_explicit_folder_replaces_everything() {
        let dir = folder("explicit");
        let schema = dir.path().join("elsewhere");
        std::fs::create_dir_all(&schema).unwrap();
        std::fs::write(schema.join("components.json"), COMPONENTS).unwrap();
        let request = Request { url: Some(dead_url()), dir: Some(schema.clone()), offline: false };
        let loaded = load(dir.path(), &request);
        assert_eq!(loaded.origin, Origin::Dir { path: schema.display().to_string() });
        assert_eq!(component_names(&loaded), ["banner"]);
        let empty = load(dir.path(), &Request { dir: Some(dir.path().join("nope")), ..Request::default() });
        assert_eq!(empty.origin, Origin::None);
    }

    #[test]
    fn stale_caches_are_flagged() {
        let dir = folder("stale");
        let url = "https://site.example.com/_cowmark/schema.json";
        let old = now_secs() - STALE_AFTER.as_secs() - 3_600;
        write_cache(dir.path(), url, &envelope(1, "old"), old).unwrap();
        let loaded = load(dir.path(), &Request { url: Some(url.into()), offline: true, dir: None });
        assert!(loaded.is_stale());
        assert!(loaded.describe().contains("from 7 days ago"), "{}", loaded.describe());
        assert!(loaded.note().unwrap().contains("may be out of date"));
        assert_eq!(loaded.to_json()["stale"], true);
        assert_eq!(loaded.to_json()["origin"]["kind"], "cache");
    }

    #[test]
    fn a_git_file_points_at_the_cache_dir() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("modules/content");
        std::fs::create_dir_all(&real).unwrap();
        let root = dir.path().join("checkout");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        assert_eq!(cache_dir(&root), Some(real.join("cowmark")));
        assert_eq!(cache_dir(dir.path()), None);
    }

    #[test]
    fn the_memo_skips_repeat_fetches_and_retries_failures_later() {
        let dir = folder("memo");
        let server = Server::start("200 OK", &envelope(1, "a"));
        let memo = Memo::new(Duration::from_secs(600), Duration::from_millis(0));
        let request = Request { url: Some(server.url.clone()), ..Request::default() };
        assert!(matches!(load_with(dir.path(), &request, Some(&memo)).origin, Origin::Site { .. }));
        server.set("200 OK", &envelope(1, "b"));
        let again = load_with(dir.path(), &request, Some(&memo));
        assert_eq!(component_names(&again), ["a"]);
        assert_eq!(server.hits.load(Ordering::SeqCst), 1);
        memo.clear();
        assert_eq!(component_names(&load_with(dir.path(), &request, Some(&memo))), ["b"]);
        assert_eq!(server.hits.load(Ordering::SeqCst), 2);

        // A failure is retried once `retry_after` passes (zero here).
        server.set("503 Service Unavailable", "");
        assert!(matches!(load_with(dir.path(), &request, Some(&memo)).origin, Origin::Site { .. }));
        memo.clear();
        assert!(matches!(load_with(dir.path(), &request, Some(&memo)).origin, Origin::Cache { .. }));
        server.set("200 OK", &envelope(1, "c"));
        assert_eq!(component_names(&load_with(dir.path(), &request, Some(&memo))), ["c"]);
    }
}
