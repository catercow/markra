//! File operations for outside tools, limited to the content folder: reading,
//! writing Markdown under `content/` and images under `public/`, the schemas,
//! new pages and page images. The MCP server is built on these.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde_json::{Map, Value, json};

use crate::root::{self, resolve, resolve_writable};

const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_LISTED: usize = 5000;

// --- files -----------------------------------------------------------------------

/// Root-relative paths of files under `dir` (default: `content`), sorted.
/// `.git`, hidden entries and node_modules are skipped.
pub fn list_files(root: &Path, dir: Option<&str>, extension: Option<&str>, recursive: bool) -> Result<Value, String> {
    let dir = dir.map(str::trim).filter(|dir| !dir.is_empty() && *dir != ".");
    let start = match dir {
        Some(dir) => resolve(root, dir)?.0,
        None => root.join("content"),
    };
    if !start.is_dir() {
        return Err(format!("{} isn't a folder", dir.unwrap_or("content")));
    }
    let wanted = extension.map(|extension| extension.trim_start_matches('.').to_ascii_lowercase());
    let mut files = Vec::new();
    let mut truncated = false;
    walk(&start, recursive, &mut |path| {
        let relative = relative_to(root, path);
        if wanted
            .as_deref()
            .is_none_or(|wanted| root::extension(&relative) == wanted)
        {
            if files.len() >= MAX_LISTED {
                truncated = true;
            } else {
                files.push(relative);
            }
        }
    });
    files.sort();
    Ok(json!({ "files": files, "truncated": truncated }))
}

fn walk(dir: &Path, recursive: bool, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if recursive {
                walk(&entry.path(), recursive, visit);
            }
        } else if kind.is_file() {
            visit(&entry.path());
        }
    }
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A text file inside the root (not `.git`).
pub fn read_file(root: &Path, path: &str) -> Result<String, String> {
    let (absolute, relative) = resolve(root, path)?;
    let meta = std::fs::metadata(&absolute).map_err(|_| format!("{relative} doesn't exist"))?;
    if !meta.is_file() {
        return Err(format!("{relative} isn't a file"));
    }
    if meta.len() > MAX_READ_BYTES {
        return Err(format!("{relative} is larger than {} MB", MAX_READ_BYTES / 1024 / 1024));
    }
    let bytes = std::fs::read(&absolute).map_err(|error| format!("{relative}: {error}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{relative} isn't a text file"))
}

/// Writes a Markdown file under `content/`, creating folders. With
/// `create_only`, refuses to replace an existing file.
pub fn write_file(root: &Path, path: &str, content: &str, create_only: bool) -> Result<Value, String> {
    let (absolute, relative) = resolve_writable(root, path)?;
    if !root::is_markdown(&relative) {
        return Err(format!(
            "{relative}: write_file writes Markdown; use add_image for images"
        ));
    }
    if absolute.is_dir() {
        return Err(format!("{relative} is a folder"));
    }
    let existed = absolute.exists();
    if existed && create_only {
        return Err(format!("{relative} already exists"));
    }
    write_atomically(&absolute, content.as_bytes()).map_err(|error| format!("{relative}: {error}"))?;
    Ok(json!({ "path": relative, "created": !existed, "bytes": content.len() }))
}

/// Writes through a temporary file in the same folder, so an editor watching
/// the file never reads half of it.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no parent folder"))?;
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = dir.join(format!(".{name}.cowmark-{}", std::process::id()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// Renames or moves a Markdown file or image under `content/`; never replaces one.
pub fn rename_file(root: &Path, from: &str, to: &str) -> Result<Value, String> {
    let (from_absolute, from_relative) = resolve_writable(root, from)?;
    let (to_absolute, to_relative) = resolve_writable(root, to)?;
    if !from_absolute.is_file() {
        return Err(format!("{from_relative} doesn't exist"));
    }
    if root::is_markdown(&from_relative) != root::is_markdown(&to_relative) {
        return Err("a Markdown file must stay Markdown, and an image an image".into());
    }
    if to_absolute.exists() {
        return Err(format!("{to_relative} already exists"));
    }
    if let Some(parent) = to_absolute.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&from_absolute, &to_absolute).map_err(|error| error.to_string())?;
    Ok(json!({ "from": from_relative, "to": to_relative }))
}

/// Deletes one Markdown file or image under `content/`. Git keeps the last
/// saved version, and CowMark shows the deletion in its save checklist.
pub fn delete_file(root: &Path, path: &str) -> Result<Value, String> {
    let (absolute, relative) = resolve_writable(root, path)?;
    if !absolute.is_file() {
        return Err(format!("{relative} doesn't exist"));
    }
    std::fs::remove_file(&absolute).map_err(|error| error.to_string())?;
    Ok(json!({ "deleted": relative }))
}

// --- schemas ---------------------------------------------------------------------

fn schema_file(root: &Path, name: &str) -> Result<Value, String> {
    let path = root.join("schema").join(name);
    let text =
        std::fs::read_to_string(&path).map_err(|_| format!("schema/{name} is missing from the content folder"))?;
    serde_json::from_str(&text).map_err(|error| format!("schema/{name} can't be read: {error}"))
}

fn components(root: &Path) -> Result<Vec<Value>, String> {
    let file = schema_file(root, "components.json")?;
    let list = file.get("components").cloned().unwrap_or(file);
    list.as_array()
        .cloned()
        .ok_or("schema/components.json has no component list".into())
}

fn collections(root: &Path) -> Result<Vec<Value>, String> {
    schema_file(root, "collections.json")?
        .get("collections")
        .and_then(Value::as_array)
        .cloned()
        .ok_or("schema/collections.json has no collection list".into())
}

/// Every component with its group and purpose, without the props.
pub fn list_components(root: &Path) -> Result<Value, String> {
    let list: Vec<Value> = components(root)?
        .iter()
        .map(|component| {
            let mut summary = Map::new();
            for key in ["name", "group", "purpose"] {
                if let Some(value) = component.get(key) {
                    summary.insert(key.into(), value.clone());
                }
            }
            let props = component.get("props").and_then(Value::as_array).map_or(0, Vec::len);
            summary.insert("props".into(), props.into());
            Value::Object(summary)
        })
        .collect();
    Ok(json!({ "components": list }))
}

/// One component's full schema: props with types, defaults, options, item fields.
pub fn get_component_schema(root: &Path, name: &str) -> Result<Value, String> {
    let name = name.trim().trim_start_matches(':');
    components(root)?
        .into_iter()
        .find(|component| {
            component.get("name").and_then(Value::as_str) == Some(name)
                || component.get("component").and_then(Value::as_str) == Some(name)
        })
        .ok_or_else(|| format!("no component named `{name}`; list_components shows them"))
}

fn find_collection(root: &Path, name: &str) -> Result<Value, String> {
    let wanted = name.trim().trim_end_matches('/');
    let all = collections(root)?;
    all.iter()
        .find(|collection| {
            let field = |key: &str| collection.get(key).and_then(Value::as_str).unwrap_or_default();
            field("name") == wanted
                || field("folder") == wanted
                || field("folder").strip_prefix("content/") == Some(wanted)
        })
        .cloned()
        .ok_or_else(|| {
            let names: Vec<&str> = all
                .iter()
                .filter_map(|collection| collection.get("name").and_then(Value::as_str))
                .collect();
            format!("no collection `{wanted}`; there are: {}", names.join(", "))
        })
}

const FRONT_MATTER_NOTES: &str = "Front matter is YAML between `---` lines at the top of the file. \
Required fields must be present and non-empty. Dates are strings in YYYY-MM-DD form. \
Keep titles under 60 characters and descriptions under 160 for search results. \
Image fields hold a site path starting with `/`, e.g. `/blog/my-post/photo.jpg` for `public/blog/my-post/photo.jpg` (see add_image).";

/// A collection's front matter fields, or every collection without `name`.
pub fn get_collection_schema(root: &Path, name: Option<&str>) -> Result<Value, String> {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => {
            let mut collection = find_collection(root, name)?;
            if let Value::Object(object) = &mut collection {
                object.insert("notes".into(), FRONT_MATTER_NOTES.into());
            }
            Ok(collection)
        }
        None => {
            let list: Vec<Value> = collections(root)?
                .iter()
                .map(|collection| {
                    json!({
                        "name": collection.get("name"),
                        "folder": collection.get("folder"),
                        "url": collection.get("url"),
                    })
                })
                .collect();
            Ok(json!({ "collections": list, "notes": FRONT_MATTER_NOTES }))
        }
    }
}

// --- new pages -------------------------------------------------------------------

/// Lowercase, hyphenated, ASCII; like the app's `slugify`.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        let c = fold_accent(c);
        if c == '\'' || c == '’' {
            continue;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let slug: String = out.trim_matches('-').chars().take(80).collect();
    slug.trim_end_matches('-').to_string()
}

fn fold_accent(c: char) -> char {
    match c {
        'à'..='å' => 'a',
        'ç' => 'c',
        'è'..='ë' => 'e',
        'ì'..='ï' => 'i',
        'ñ' => 'n',
        'ò'..='ö' | 'ø' => 'o',
        'ù'..='ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Today (UTC) as YYYY-MM-DD.
pub fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    civil_date(days)
}

/// Days since 1970-01-01 to YYYY-MM-DD (Howard Hinnant's `civil_from_days`).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A YAML scalar for a front matter value: plain when safe, else quoted.
/// Lists and objects are written as JSON, which YAML reads too.
pub fn yaml_value(value: &Value) -> String {
    match value {
        Value::Null => "''".into(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        Value::String(text) => yaml_string(text),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn yaml_string(text: &str) -> String {
    if text.contains('\n') || text.contains('\r') || text.contains('\t') {
        return Value::String(text.into()).to_string();
    }
    let lower = text.to_ascii_lowercase();
    let reserved = ["true", "false", "yes", "no", "on", "off", "null", "~", "y", "n"].contains(&lower.as_str());
    let numeric = text.parse::<f64>().is_ok() || text.starts_with("0x") || text.starts_with("0o");
    let date_like = text.len() >= 10
        && text.as_bytes()[..10].iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    let first = text.chars().next();
    let plain = !text.is_empty()
        && !reserved
        && !numeric
        && !date_like
        && text.trim() == text
        && !first.is_some_and(|c| "-?:,[]{}#&*!|>'\"%@`".contains(c))
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':');
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "''"))
    }
}

fn field_is_date(name: &str) -> bool {
    name.ends_with("_at") || name.ends_with("_on") || name == "date"
}

/// Creates `<collection folder>/<slug>.md` with the title, the collection's
/// other required fields (dates set to today, the rest empty unless `fields`
/// gives them), any extra `fields`, and `body`.
pub fn new_page(
    root: &Path,
    collection: &str,
    title: &str,
    slug: Option<&str>,
    fields: Option<&Map<String, Value>>,
    body: Option<&str>,
) -> Result<Value, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("`title` is empty".into());
    }
    let collection = find_collection(root, collection)?;
    let folder = collection
        .get("folder")
        .and_then(Value::as_str)
        .ok_or("the collection has no folder")?
        .trim_end_matches('/');
    let slug = slugify(slug.filter(|slug| !slug.trim().is_empty()).unwrap_or(title));
    if slug.is_empty() {
        return Err("couldn't make a file name from that title; pass `slug`".into());
    }
    let path = format!("{folder}/{slug}.md");

    let given = fields.cloned().unwrap_or_default();
    let mut lines: Vec<(String, Value)> = vec![("title".into(), Value::String(title.into()))];
    let schema_fields = collection
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for field in &schema_fields {
        let Some(name) = field.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name == "title" {
            continue;
        }
        let required = field.get("required").and_then(Value::as_bool).unwrap_or(false);
        let value = if let Some(value) = given.get(name) {
            value.clone()
        } else if !required {
            continue;
        } else {
            let kind = field.get("type").and_then(Value::as_str).unwrap_or_default();
            match kind {
                "boolean" => Value::Bool(field.get("default").and_then(Value::as_bool).unwrap_or(false)),
                "string" if field_is_date(name) => Value::String(today()),
                "string" => Value::String(String::new()),
                _ => match field.get("default") {
                    Some(default) => default.clone(),
                    None => continue,
                },
            }
        };
        lines.push((name.into(), value));
    }
    for (name, value) in &given {
        if name != "title" && !lines.iter().any(|(existing, _)| existing == name) {
            lines.push((name.clone(), value.clone()));
        }
    }
    let mut text = String::from("---\n");
    for (name, value) in &lines {
        text.push_str(&format!("{name}: {}\n", yaml_value(value)));
    }
    text.push_str("---\n\n");
    if let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) {
        text.push_str(body);
        text.push('\n');
    }

    write_file(root, &path, &text, true)?;
    let url = collection
        .get("url")
        .and_then(Value::as_str)
        .map(|url| page_url(url, &slug));
    let empty: Vec<&str> = lines
        .iter()
        .filter(|(_, value)| value.as_str() == Some(""))
        .map(|(name, _)| name.as_str())
        .collect();
    Ok(json!({ "path": path, "url": url, "fieldsToFill": empty, "text": text }))
}

/// The site path of a page from the collection's `url` pattern.
pub fn page_url(pattern: &str, slug: &str) -> String {
    match (pattern.find('<'), pattern.rfind('>')) {
        (Some(open), Some(close)) if open < close => format!("{}{slug}{}", &pattern[..open], &pattern[close + 1..]),
        _ => pattern.to_string(),
    }
}

// --- images ----------------------------------------------------------------------

const GENERIC_NAMES: [&str; 5] = ["image", "img", "photo", "picture", "blob"];
const GENERIC_PREFIXES: [&str; 4] = ["pasted", "clipboard", "untitled", "download"];

fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if starts(b"\xff\xd8\xff") {
        Some("jpg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() > 12 && &bytes[4..8] == b"ftyp" && (&bytes[8..12] == b"avif" || &bytes[8..12] == b"avis") {
        Some("avif")
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
        head.contains("<svg").then_some("svg")
    }
}

/// The site path of a page's image folder (`content/blog/my-post.md` →
/// `/blog/my-post`), served from `public/`. Follows the collection's URL prefix;
/// a page outside every collection uses its path under `content/`.
fn page_image_folder(root: &Path, page_relative: &str) -> Result<String, String> {
    let without_extension = page_relative
        .rsplit_once('.')
        .map_or(page_relative, |(stem, _)| stem);
    let all = collections(root).unwrap_or_default();
    let found = all.iter().find_map(|collection| {
        let field = |key: &str| collection.get(key).and_then(Value::as_str).unwrap_or_default();
        let folder = field("folder").trim_end_matches('/');
        let slug = without_extension.strip_prefix(folder)?.strip_prefix('/')?;
        (!slug.is_empty() && !slug.contains('/'))
            .then(|| cowmark_lint::page_image_folder(field("url"), folder, slug))
    });
    match found {
        Some(folder) => Ok(folder),
        None => without_extension
            .strip_prefix("content/")
            .map(|inner| format!("/{inner}"))
            .ok_or_else(|| format!("{page_relative} isn't a page under content/")),
    }
}

/// Copies an image into the page's folder under `public/` (`content/blog/my-post.md` →
/// `public/blog/my-post/<name>.<ext>`) and returns the site path to use in the
/// page (`/blog/my-post/<name>.<ext>`). From a local file (`source`) or base64 `data`.
pub fn add_image(
    root: &Path,
    page: &str,
    source: Option<&Path>,
    data_base64: Option<&str>,
    name: Option<&str>,
    alt: Option<&str>,
) -> Result<Value, String> {
    let (page_absolute, page_relative) = resolve_writable(root, page)?;
    if !root::is_markdown(&page_relative) {
        return Err(format!("{page_relative} isn't a Markdown page"));
    }
    if !page_absolute.is_file() {
        return Err(format!("{page_relative} doesn't exist; create the page first"));
    }
    let (bytes, source_name) = match (source, data_base64) {
        (Some(source), None) => {
            let meta = std::fs::metadata(source).map_err(|_| format!("{} doesn't exist", source.display()))?;
            if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
                return Err(format!("{} isn't an image file under 25 MB", source.display()));
            }
            let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
            (
                bytes,
                source.file_name().map(|name| name.to_string_lossy().into_owned()),
            )
        }
        (None, Some(data)) => {
            let data = data.split_once(";base64,").map_or(data, |(_, data)| data);
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .map_err(|error| format!("`data` isn't base64: {error}"))?;
            (bytes, None)
        }
        _ => return Err("give either `source` (a local file path) or `data` (base64)".into()),
    };
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("the image is larger than 25 MB".into());
    }
    let extension = sniff_image(&bytes).ok_or("that isn't a PNG, JPEG, GIF, WebP, AVIF or SVG image")?;

    let stem = Path::new(&page_relative)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let wanted = name.map(str::to_string).or(source_name).unwrap_or_default();
    let wanted = Path::new(&wanted)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut base = slugify(&wanted);
    if base.is_empty()
        || GENERIC_NAMES.contains(&base.as_str())
        || GENERIC_PREFIXES.iter().any(|prefix| base.starts_with(prefix))
    {
        base = format!("{}-image", if stem.is_empty() { "page".into() } else { slugify(&stem) });
    }
    let site_folder = page_image_folder(root, &page_relative)?;
    let folder = root.join("public").join(site_folder.trim_start_matches('/'));
    let mut file_name = format!("{base}.{extension}");
    let mut n = 2;
    while folder.join(&file_name).exists() {
        file_name = format!("{base}-{n}.{extension}");
        n += 1;
    }
    let reference = format!("{site_folder}/{file_name}");
    let (absolute, relative) = resolve_writable(root, &format!("public{reference}"))?;
    write_atomically(&absolute, &bytes).map_err(|error| format!("{relative}: {error}"))?;

    let alt = alt.unwrap_or("").replace(['[', ']'], "");
    Ok(json!({
        "path": relative,
        "reference": reference,
        "markdown": format!("![{alt}]({reference})"),
        "note": "Images are copied as they are; keep photos under 2400 px on the longest side. Describe the image in the alt text. Include the image's path in propose_publish."
    }))
}

/// Absolute path of `path`, for messages.
pub fn display(root: &Path, relative: &str) -> PathBuf {
    root.join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("cm-cli-content-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("content/blog")).unwrap();
        std::fs::create_dir_all(root.join("schema")).unwrap();
        std::fs::write(
            root.join("schema/collections.json"),
            r#"{"collections":[{"name":"blog","folder":"content/blog","url":"/blog/<file name without .md>","fields":[
                {"name":"title","type":"string","required":true},
                {"name":"description","type":"string","required":true},
                {"name":"published_at","type":"string","required":true},
                {"name":"draft","type":"boolean","required":true,"default":true},
                {"name":"category","type":"\"a\" | \"b\"","required":false}]}]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("schema/components.json"),
            r#"{"components":[{"name":"callout","component":"Callout","group":"Text","purpose":"A box","props":[{"name":"type","type":"string"}]}]}"#,
        )
        .unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn dates_and_slugs() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_752), "2026-10-26");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(slugify("Papa John's Café: 10 Tips!"), "papa-johns-cafe-10-tips");
        assert_eq!(slugify("  "), "");
    }

    #[test]
    fn yaml_values_are_quoted_only_when_needed() {
        assert_eq!(yaml_value(&json!("Hello world")), "Hello world");
        assert_eq!(yaml_value(&json!("Papa John's: deals")), "'Papa John''s: deals'");
        assert_eq!(yaml_value(&json!("2026-10-06")), "'2026-10-06'");
        assert_eq!(yaml_value(&json!("true")), "'true'");
        assert_eq!(yaml_value(&json!("42")), "'42'");
        assert_eq!(yaml_value(&json!("")), "''");
        assert_eq!(yaml_value(&json!("- item")), "'- item'");
        assert_eq!(yaml_value(&json!("two\nlines")), "\"two\\nlines\"");
        assert_eq!(yaml_value(&json!(3)), "3");
        assert_eq!(yaml_value(&json!(["a", 1])), "[\"a\",1]");
    }

    #[test]
    fn new_pages_have_valid_front_matter() {
        let root = repo("new");
        let fields: Map<String, Value> =
            serde_json::from_value(json!({ "description": "About lunch", "category": "a" })).unwrap();
        let made = new_page(&root, "blog", "Lunch Ideas: 2026", None, Some(&fields), Some("Hello")).unwrap();
        assert_eq!(made["path"], "content/blog/lunch-ideas-2026.md");
        assert_eq!(made["url"], "/blog/lunch-ideas-2026");
        let text = std::fs::read_to_string(root.join("content/blog/lunch-ideas-2026.md")).unwrap();
        assert_eq!(
            text,
            format!(
                "---\ntitle: 'Lunch Ideas: 2026'\ndescription: About lunch\npublished_at: '{}'\ndraft: true\ncategory: a\n---\n\nHello\n",
                today()
            )
        );
        let report = crate::check::check(&root, &[made["path"].as_str().unwrap().into()]).unwrap();
        assert_eq!(report.errors(), 0, "{}", report.to_text());

        // Without a description the field is left empty for the writer to fill.
        let made = new_page(&root, "content/blog", "Second", Some("custom-slug"), None, None).unwrap();
        assert_eq!(made["path"], "content/blog/custom-slug.md");
        assert_eq!(made["fieldsToFill"], json!(["description"]));
        assert!(
            new_page(&root, "blog", "Second", Some("custom-slug"), None, None)
                .unwrap_err()
                .contains("already exists")
        );
        assert!(
            new_page(&root, "nope", "X", None, None, None)
                .unwrap_err()
                .contains("blog")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn files_are_read_written_renamed_and_deleted_inside_content_only() {
        let root = repo("files");
        assert_eq!(
            write_file(&root, "content/blog/a.md", "# A\n", false).unwrap()["created"],
            true
        );
        assert_eq!(
            write_file(&root, "content/blog/a.md", "# A2\n", false).unwrap()["created"],
            false
        );
        assert!(write_file(&root, "content/blog/a.md", "x", true).is_err());
        assert_eq!(read_file(&root, "content/blog/a.md").unwrap(), "# A2\n");
        assert!(read_file(&root, "schema/components.json").is_ok());
        assert!(write_file(&root, "schema/components.json", "{}", false).is_err());
        assert!(write_file(&root, "content/blog/a.png", "x", false).is_err());
        assert!(write_file(&root, "../a.md", "x", false).is_err());

        let listed = list_files(&root, None, Some("md"), true).unwrap();
        assert_eq!(listed["files"], json!(["content/blog/a.md"]));
        let listed = list_files(&root, Some("."), None, false).unwrap();
        assert_eq!(listed["files"], json!([]));
        assert!(list_files(&root, Some("../"), None, true).is_err());

        assert_eq!(
            rename_file(&root, "content/blog/a.md", "content/blog/b.md").unwrap()["to"],
            "content/blog/b.md"
        );
        assert!(rename_file(&root, "content/blog/b.md", "content/blog/b.png").is_err());
        assert!(rename_file(&root, "content/blog/b.md", "README.md").is_err());
        assert_eq!(
            delete_file(&root, "content/blog/b.md").unwrap()["deleted"],
            "content/blog/b.md"
        );
        assert!(delete_file(&root, "content/blog/b.md").is_err());
        assert!(delete_file(&root, "schema/collections.json").is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn schemas_are_served_from_the_repo() {
        let root = repo("schemas");
        assert_eq!(
            list_components(&root).unwrap()["components"][0],
            json!({ "name": "callout", "group": "Text", "purpose": "A box", "props": 1 })
        );
        assert_eq!(get_component_schema(&root, "Callout").unwrap()["name"], "callout");
        assert_eq!(
            get_component_schema(&root, "::callout").unwrap()["props"][0]["name"],
            "type"
        );
        assert!(get_component_schema(&root, "nope").is_err());
        assert_eq!(
            get_collection_schema(&root, Some("blog")).unwrap()["fields"][0]["name"],
            "title"
        );
        assert_eq!(
            get_collection_schema(&root, None).unwrap()["collections"][0]["url"],
            "/blog/<file name without .md>"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn images_are_copied_into_the_pages_public_folder() {
        let root = repo("images");
        write_file(&root, "content/blog/my-post.md", "# Post\n", false).unwrap();
        let png = b"\x89PNG\r\n\x1a\n0000";
        let source = root.join("Team Lunch.PNG");
        std::fs::write(&source, png).unwrap();

        let added = add_image(
            &root,
            "content/blog/my-post.md",
            Some(&source),
            None,
            None,
            Some("Team [lunch]"),
        )
        .unwrap();
        assert_eq!(added["path"], "public/blog/my-post/team-lunch.png");
        assert_eq!(added["markdown"], "![Team lunch](/blog/my-post/team-lunch.png)");
        assert!(root.join("public/blog/my-post/team-lunch.png").is_file());
        let again = add_image(&root, "content/blog/my-post.md", Some(&source), None, None, None).unwrap();
        assert_eq!(again["reference"], "/blog/my-post/team-lunch-2.png");

        let data = base64::engine::general_purpose::STANDARD.encode(b"\xff\xd8\xff\xe0jpeg");
        let pasted = add_image(
            &root,
            "content/blog/my-post.md",
            None,
            Some(&data),
            Some("image.png"),
            None,
        )
        .unwrap();
        assert_eq!(pasted["reference"], "/blog/my-post/my-post-image.jpg");

        std::fs::write(&source, "not an image").unwrap();
        assert!(add_image(&root, "content/blog/my-post.md", Some(&source), None, None, None).is_err());
        assert!(add_image(&root, "content/blog/missing.md", None, Some(&data), None, None).is_err());
        assert!(add_image(&root, "content/blog/my-post.md", None, None, None, None).is_err());
        // The references resolve for the linter as they do on the site.
        let page = format!(
            "---\ntitle: Post\ndescription: D\npublished_at: '2026-01-01'\ndraft: true\n---\n\n{}\n\n![Pasted]({})\n",
            added["markdown"].as_str().unwrap(),
            pasted["reference"].as_str().unwrap()
        );
        write_file(&root, "content/blog/my-post.md", &page, false).unwrap();
        let report = crate::check::check(&root, &[]).unwrap();
        assert!(report.diagnostics.is_empty(), "{}", report.to_text());
        write_file(
            &root,
            "content/blog/my-post.md",
            &page.replace("team-lunch.png", "gone.png"),
            false,
        )
        .unwrap();
        assert_eq!(crate::check::check(&root, &[]).unwrap().errors(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
