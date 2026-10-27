//! Checks CaterCow marketing content files: MDC components against `schema/components.json`,
//! front matter against `schema/collections.json`, images, links to content pages, leftover
//! merge conflicts, and what the site refuses to render (raw HTML, scripting links, event
//! handler attributes). Pure: it reads no files itself. The caller passes the document text
//! and a [`FileLookup`] that answers whether a repository-relative path exists.
//!
//! ```
//! use cowmark_lint::{Linter, Rule, Schemas};
//! let linter = Linter::new(Schemas::from_json(None, None).unwrap());
//! let found = linter.lint("content/blog/a.md", "::callout\nHi\n", &|_: &str| true);
//! assert_eq!(found[0].rule, Rule::UnclosedComponent);
//! ```

mod conflict;
pub mod formats;
mod format;
mod frontmatter;
mod input;
#[cfg(feature = "fs")]
pub mod fs;
mod markdown;
mod mdc;
mod mdc_lint;
mod messages;
mod refs;
mod safety;
mod schema;
pub mod text;
mod value;
mod yaml;

use serde::Serialize;

pub use format::{Formatted, format};
pub use input::render_table;
pub use messages::Messages;
pub use schema::{
    page_image_folder, CollectionSchema, ComponentInput, ComponentSchema, FieldSchema, InputColumn, PropSchema,
    SchemaError, Schemas,
};
pub use text::{utf8_to_utf16, utf16_to_utf8};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// What a diagnostic is about. Serialized as its kebab-case id, e.g. `unknown-component`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    // Components (ported from the editor's TypeScript linter).
    UnknownComponent,
    MissingProp,
    MissingItemField,
    UnknownProp,
    UnknownItemField,
    PropType,
    UnclosedComponent,
    UnclosedProps,
    PropsYaml,
    // Front matter.
    FrontmatterYaml,
    FrontmatterUnclosed,
    MissingField,
    EmptyField,
    UnknownField,
    FieldType,
    DateFormat,
    // Images and links.
    MissingImage,
    ImageAlt,
    BrokenLink,
    // Merges.
    UnresolvedConflict,
    // What the site refuses to render.
    RawHtml,
    UnsafeLink,
    UnsafeAttribute,
    // What goes in a component's slot or a link prop (`input` and `format` in the schema).
    ComponentTable,
    CellFormat,
    LinkFormat,
    GalleryImages,
}

impl Rule {
    pub const ALL: [Rule; 27] = [
        Rule::UnknownComponent,
        Rule::MissingProp,
        Rule::MissingItemField,
        Rule::UnknownProp,
        Rule::UnknownItemField,
        Rule::PropType,
        Rule::UnclosedComponent,
        Rule::UnclosedProps,
        Rule::PropsYaml,
        Rule::FrontmatterYaml,
        Rule::FrontmatterUnclosed,
        Rule::MissingField,
        Rule::EmptyField,
        Rule::UnknownField,
        Rule::FieldType,
        Rule::DateFormat,
        Rule::MissingImage,
        Rule::ImageAlt,
        Rule::BrokenLink,
        Rule::UnresolvedConflict,
        Rule::RawHtml,
        Rule::UnsafeLink,
        Rule::UnsafeAttribute,
        Rule::ComponentTable,
        Rule::CellFormat,
        Rule::LinkFormat,
        Rule::GalleryImages,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Rule::UnknownComponent => "unknown-component",
            Rule::MissingProp => "missing-prop",
            Rule::MissingItemField => "missing-item-field",
            Rule::UnknownProp => "unknown-prop",
            Rule::UnknownItemField => "unknown-item-field",
            Rule::PropType => "prop-type",
            Rule::UnclosedComponent => "unclosed-component",
            Rule::UnclosedProps => "unclosed-props",
            Rule::PropsYaml => "props-yaml",
            Rule::FrontmatterYaml => "frontmatter-yaml",
            Rule::FrontmatterUnclosed => "frontmatter-unclosed",
            Rule::MissingField => "missing-field",
            Rule::EmptyField => "empty-field",
            Rule::UnknownField => "unknown-field",
            Rule::FieldType => "field-type",
            Rule::DateFormat => "date-format",
            Rule::MissingImage => "missing-image",
            Rule::ImageAlt => "image-alt",
            Rule::BrokenLink => "broken-link",
            Rule::UnresolvedConflict => "unresolved-conflict",
            Rule::RawHtml => "raw-html",
            Rule::UnsafeLink => "unsafe-link",
            Rule::UnsafeAttribute => "unsafe-attribute",
            Rule::ComponentTable => "component-table",
            Rule::CellFormat => "cell-format",
            Rule::LinkFormat => "link-format",
            Rule::GalleryImages => "gallery-images",
        }
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// One problem. Offsets are given both as UTF-8 bytes (Rust) and UTF-16 code units
/// (JavaScript strings, CodeMirror positions).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// The document's repository-relative path, as passed in.
    pub file: String,
    pub from: usize,
    pub to: usize,
    pub from_utf16: usize,
    pub to_utf16: usize,
    /// 1-based line and column (columns count characters).
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub severity: Severity,
    pub rule: Rule,
    /// Plain-language message; text between backticks is a name to show as code.
    pub message: String,
    /// The edit that writes the value in its canonical form, when the problem is only how it
    /// is written. [`format`] applies these; the editor offers them as a quick fix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

/// Replace `from..to` with `insert`. Offsets as in [`Diagnostic`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub from: usize,
    pub to: usize,
    pub from_utf16: usize,
    pub to_utf16: usize,
    pub insert: String,
}

/// Answers whether a path exists in the content repository. Paths are repository-relative with
/// `/` separators, e.g. `public/blog/a/photo.jpg` or `content/blog/a.md`; directories count.
/// Matching should be case-sensitive, as on the site's servers.
pub trait FileLookup {
    fn exists(&self, path: &str) -> bool;
}

impl<F: Fn(&str) -> bool> FileLookup for F {
    fn exists(&self, path: &str) -> bool {
        self(path)
    }
}

/// A problem before its offsets are converted.
#[derive(Debug, Clone)]
pub(crate) struct Found {
    pub from: usize,
    pub to: usize,
    pub severity: Severity,
    pub rule: Rule,
    pub message: String,
    pub fix: Option<(text::Range, String)>,
}

pub(crate) struct Report<'m> {
    pub messages: &'m Messages,
    pub found: Vec<Found>,
}

impl Report<'_> {
    pub fn add(
        &mut self,
        range: text::Range,
        severity: Severity,
        rule: Rule,
        template: &str,
        values: &[(&str, &str)],
    ) {
        self.found.push(Found {
            from: range.from,
            to: range.to.max(range.from),
            severity,
            rule,
            message: messages::fill(template, values),
            fix: None,
        });
    }

    /// An error whose `fix` replaces `fix.0` with `fix.1`.
    pub fn add_fix(
        &mut self,
        range: text::Range,
        rule: Rule,
        template: &str,
        values: &[(&str, &str)],
        fix: (text::Range, String),
    ) {
        self.add(range, Severity::Error, rule, template, values);
        if let Some(found) = self.found.last_mut() {
            found.fix = Some(fix);
        }
    }
}

/// Schemas and messages, prepared once and reused for every document.
#[derive(Debug, Clone, Default)]
pub struct Linter {
    pub schemas: Schemas,
    pub messages: Messages,
}

impl Linter {
    pub fn new(schemas: Schemas) -> Linter {
        Linter {
            schemas,
            messages: Messages::default(),
        }
    }

    pub fn with_messages(mut self, messages: Messages) -> Linter {
        self.messages = messages;
        self
    }

    /// All problems in one document, sorted by position. `path` is repository-relative
    /// (`content/blog/my-post.md`); it decides the collection and how relative images resolve.
    pub fn lint(&self, path: &str, text: &str, files: &dyn FileLookup) -> Vec<Diagnostic> {
        run(&self.schemas, &self.messages, path, text, files)
    }

    /// The document with every fix applied (values in their canonical form), and the problems
    /// left in it. See [`format()`].
    pub fn format(&self, path: &str, text: &str, files: &dyn FileLookup) -> Formatted {
        format::format_with(&self.schemas, &self.messages, path, text, files)
    }
}

/// [`Linter::lint`] with the default messages.
pub fn lint_document(
    path: &str,
    text: &str,
    schemas: &Schemas,
    files: &dyn FileLookup,
) -> Vec<Diagnostic> {
    run(schemas, &Messages::default(), path, text, files)
}

pub(crate) fn run(
    schemas: &Schemas,
    messages: &Messages,
    path: &str,
    text: &str,
    files: &dyn FileLookup,
) -> Vec<Diagnostic> {
    let mut report = Report {
        messages,
        found: Vec::new(),
    };
    let context = refs::Context::new(path, schemas, files);

    conflict::check(text, &mut report);
    let body_start = frontmatter::check(text, &context, &mut report);
    let structure = mdc::read_structure(text, body_start);
    let components = (!schemas.components.is_empty()).then_some(schemas);
    // Components inside others, from opener to closer: not part of the outer one's slot.
    let extents: Vec<text::Range> = structure
        .blocks
        .iter()
        .filter_map(|block| block.closer.map(|closer| text::Range::new(block.line_from, closer.to)))
        .collect();
    for block in &structure.blocks {
        let nested: Vec<text::Range> = match block.body() {
            Some(body) => extents
                .iter()
                .filter(|extent| extent.from >= body.from && extent.from < body.to)
                .copied()
                .collect(),
            None => Vec::new(),
        };
        mdc_lint::check_block(text, block, &nested, components, &context, &mut report);
    }
    markdown::check_prose(text, &structure.prose, &context, &mut report);
    safety::check_prose(text, &structure.prose, &mut report);

    let index = text::LineIndex::new(text);
    let mut found = report.found;
    found.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    found
        .into_iter()
        .map(|found| {
            let (line, column) = index.line_column(found.from);
            let (end_line, end_column) = index.line_column(found.to);
            Diagnostic {
                file: path.to_string(),
                from: found.from,
                to: found.to,
                from_utf16: index.utf16(found.from),
                to_utf16: index.utf16(found.to),
                line,
                column,
                end_line,
                end_column,
                severity: found.severity,
                rule: found.rule,
                message: found.message,
                fix: found.fix.map(|(range, insert)| Fix {
                    from: range.from,
                    to: range.to,
                    from_utf16: index.utf16(range.from),
                    to_utf16: index.utf16(range.to),
                    insert,
                }),
            }
        })
        .collect()
}
