//! Whether images and links point at files that exist. Mirrors the site: an image written
//! relative to its page (`my-post/photo.jpg` in `content/blog/my-post.md`) is served from
//! `content/blog/my-post/photo.jpg` (js/nuxt4/modules/content-media/rewrite.ts), and a
//! site-absolute path (`/blog/my-post/photo.jpg`) from `public/` or `content/`. Links are
//! checked only when a collection's URL pattern covers them; other routes come from the app.

use crate::schema::{Collection, Schemas};
use crate::text::Range;
use crate::{FileLookup, Report, Rule, Severity};

pub const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg"];
const SITE_HOSTS: [&str; 2] = ["www.catercow.com", "catercow.com"];

pub struct Context<'a> {
    /// Repository-relative path of the document.
    pub path: &'a str,
    pub schemas: &'a Schemas,
    pub collection: Option<&'a Collection>,
    /// The page's own site path, e.g. `/blog/my-post`.
    page_url: Option<String>,
    files: &'a dyn FileLookup,
}

enum Target {
    /// Not checkable here (another site, the main repository's `/images/…`, a data URL).
    Skip,
    /// Relative to the page: served only from this path under `content/`.
    Content(String),
    /// A site path served from `public/` or `content/`.
    Site(String),
    Outside,
}

impl<'a> Context<'a> {
    pub fn new(path: &'a str, schemas: &'a Schemas, files: &'a dyn FileLookup) -> Context<'a> {
        let found = schemas.collection_for(path);
        Context {
            path,
            schemas,
            collection: found.as_ref().map(|(collection, _)| *collection),
            page_url: found.map(|(collection, slug)| collection.url_of(&slug)),
            files,
        }
    }

    fn image_target(&self, src: &str) -> Target {
        let src = src.trim();
        if src.is_empty() || src.starts_with("//") {
            return Target::Skip;
        }
        if has_scheme(src) {
            return match site_path(src) {
                Some(path) => Target::Site(path),
                None => Target::Skip,
            };
        }
        if src.starts_with('/') {
            return Target::Site(decode(strip_suffixes(src)));
        }
        // Relative to the page's folder under content/ (or, outside content/, to its folder).
        let (root, inner) = match self.path.strip_prefix("content/") {
            Some(inner) => ("content", inner),
            None => ("", self.path),
        };
        let mut parts: Vec<&str> = inner.split('/').collect();
        parts.pop();
        let src = decode(strip_suffixes(src));
        for segment in src.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Target::Outside;
                    }
                }
                _ => parts.push(segment),
            }
        }
        let joined = parts.join("/");
        Target::Content(if root.is_empty() {
            joined
        } else {
            format!("{root}/{joined}")
        })
    }

    fn site_file_exists(&self, path: &str) -> Option<bool> {
        let top = path.trim_start_matches('/').split('/').next().unwrap_or("");
        if top.is_empty()
            || !(self.files.exists(&format!("public/{top}"))
                || self.files.exists(&format!("content/{top}")))
        {
            return None;
        }
        Some(
            self.files.exists(&format!("public{path}"))
                || self.files.exists(&format!("content{path}")),
        )
    }

    /// Reports `src` when the image it names doesn't exist.
    pub fn check_image(&self, src: &str, range: Range, report: &mut Report) {
        let missing = match self.image_target(src) {
            Target::Skip => false,
            Target::Content(path) => !self.files.exists(&path),
            Target::Site(path) => self.site_file_exists(&path) == Some(false),
            Target::Outside => {
                let messages = report.messages;
                report.add(
                    range,
                    Severity::Error,
                    Rule::MissingImage,
                    &messages.image_outside_content,
                    &[("src", src.trim())],
                );
                return;
            }
        };
        if missing {
            let messages = report.messages;
            report.add(
                range,
                Severity::Error,
                Rule::MissingImage,
                &messages.missing_image,
                &[("src", src.trim())],
            );
        }
    }

    /// Reports `href` when it is a link to a content page that doesn't exist.
    pub fn check_link(&self, href: &str, range: Range, report: &mut Report) {
        let href = href.trim();
        let path = if href.is_empty() || href.starts_with('#') {
            return;
        } else if has_scheme(href) {
            match site_path(href) {
                Some(path) => path,
                None => return,
            }
        } else if let Some(rest) = href.strip_prefix("//") {
            match site_path(&format!("https://{rest}")) {
                Some(path) => path,
                None => return,
            }
        } else if href.starts_with('/') {
            decode(strip_suffixes(href))
        } else {
            let Some(page) = &self.page_url else { return };
            resolve_relative(page, &decode(strip_suffixes(href)))
        };
        let path = if path.len() > 1 {
            path.trim_end_matches('/').to_string()
        } else {
            path
        };
        for collection in &self.schemas.collections {
            if let Some(file) = collection.file_for_url(&path) {
                if !self.files.exists(&file) {
                    let messages = report.messages;
                    report.add(
                        range,
                        Severity::Warning,
                        Rule::BrokenLink,
                        &messages.broken_link,
                        &[("path", &path)],
                    );
                }
                return;
            }
        }
    }

    /// A component prop or front matter string: checked as an image or a link when it looks
    /// like one.
    pub fn check_value(&self, value: &str, range: Range, report: &mut Report) {
        let value = value.trim();
        if is_image_ref(value) {
            self.check_image(value, range, report);
        } else if value.starts_with('/') || site_path(value).is_some() {
            self.check_link(value, range, report);
        }
    }
}

/// A value the site treats as an image: a relative or site path ending in an image extension.
pub fn is_image_ref(value: &str) -> bool {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return false;
    }
    let path = if has_scheme(value) {
        match site_path(value) {
            Some(path) => path,
            None => return false,
        }
    } else {
        strip_suffixes(value).to_string()
    };
    path.rsplit_once('.').is_some_and(|(_, ext)| {
        IMAGE_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(ext))
    })
}

/// `scheme:` as in the site's RELATIVE_IMAGE test.
fn has_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme = &value[..colon];
    scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// The path of a catercow.com URL, without query or fragment.
fn site_path(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if !SITE_HOSTS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(host))
    {
        return None;
    }
    let path = strip_suffixes(path);
    Some(decode(if path.is_empty() { "/" } else { path }))
}

fn strip_suffixes(value: &str) -> &str {
    value.split(['?', '#']).next().unwrap_or("")
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push(u8::from_str_radix(&value[i + 1..i + 3], 16).unwrap_or(b'%'));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn resolve_relative(page: &str, href: &str) -> String {
    let mut parts: Vec<&str> = page.split('/').filter(|part| !part.is_empty()).collect();
    parts.pop();
    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(segment),
        }
    }
    let trailing = if href.ends_with('/') { "/" } else { "" };
    format!("/{}{trailing}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_images_and_site_urls() {
        assert!(is_image_ref("my-post/photo.JPG"));
        assert!(is_image_ref("/blog/a/b.webp?x=1"));
        assert!(!is_image_ref("https://example.com/a"));
        assert!(!is_image_ref("Search"));
        assert_eq!(
            site_path("https://www.catercow.com/blog/a?x#y").as_deref(),
            Some("/blog/a")
        );
        assert_eq!(site_path("https://catercow.com").as_deref(), Some("/"));
        assert_eq!(site_path("https://example.com/blog/a"), None);
        assert_eq!(resolve_relative("/blog/a", "b"), "/blog/b");
        assert_eq!(
            resolve_relative("/blog/a", "../dishes/pizza"),
            "/dishes/pizza"
        );
        assert_eq!(decode("caf%C3%A9.jpg"), "café.jpg");
    }
}
