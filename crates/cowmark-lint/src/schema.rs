//! The content repository's `schema/components.json` and `schema/collections.json`.

use serde::Deserialize;

use crate::value::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct PropSchema {
    pub name: String,
    #[serde(rename = "type", default)]
    pub type_: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    /// JavaScript source of the default value, e.g. `""`, `[]`, `false`.
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub items: Option<Vec<PropSchema>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentSchema {
    pub name: String,
    #[serde(default)]
    pub props: Vec<PropSchema>,
}

impl PropSchema {
    pub fn is_required(&self) -> bool {
        self.required.unwrap_or(false)
    }

    pub fn items(&self) -> &[PropSchema] {
        self.items.as_deref().unwrap_or(&[])
    }

    /// Options for a select field: `options` from the schema, else the members of a literal union.
    pub fn select_options(&self) -> Option<Vec<Value>> {
        if let Some(options) = &self.options
            && !options.is_empty()
        {
            return Some(options.iter().cloned().map(Value::from_json).collect());
        }
        literal_union(self.type_.as_deref().unwrap_or(""))
    }

    /// The schema default as a value; None when there is none or it is not a literal.
    pub fn schema_default(&self) -> Option<Value> {
        let source = self.default.as_deref()?.trim();
        if source.is_empty() {
            return None;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(source) {
            return Some(Value::from_json(value));
        }
        let inner = source.strip_prefix('\'')?.strip_suffix('\'')?;
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                out.push(chars.next()?);
            } else if c == '\'' {
                return None;
            } else {
                out.push(c);
            }
        }
        Some(Value::Str(out))
    }
}

/// Members of a type like `"a" | "b"` or `2 | 3`; None unless every member is a literal.
pub fn literal_union(type_: &str) -> Option<Vec<Value>> {
    let members: Vec<&str> = type_.split('|').map(str::trim).collect();
    if members.len() < 2 {
        return None;
    }
    let mut values = Vec::new();
    for member in members {
        let quoted = member.len() >= 2
            && ((member.starts_with('"') && member.ends_with('"'))
                || (member.starts_with('\'') && member.ends_with('\'')));
        if quoted {
            values.push(Value::Str(member[1..member.len() - 1].to_string()));
        } else if is_decimal(member) {
            values.push(Value::Num(member.parse().ok()?));
        } else {
            return None;
        }
    }
    Some(values)
}

/// `/^-?\d+(\.\d+)?$/`
pub fn is_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (int, frac) = match digits.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (digits, None),
    };
    !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|frac| !frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit()))
}

/// `kebabCase` from the editor: `fooBar` → `foo-bar`.
pub fn kebab_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && i > 0
            && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit())
        {
            out.push('-');
        }
        out.push(c);
    }
    out.to_lowercase()
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    #[serde(rename = "type", default)]
    pub type_: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CollectionSchema {
    pub name: String,
    /// Repository-relative, e.g. `content/blog`.
    pub folder: String,
    /// e.g. `/blog/<file name without .md>`.
    pub url: String,
    #[serde(default)]
    pub fields: Vec<FieldSchema>,
}

/// A front matter field type as `collections.json` writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Text,
    Number,
    Boolean,
    Options(Vec<Value>),
    List(Box<FieldType>),
    /// `object` (any fields) or `{ name: type, … }`.
    Object(Option<Vec<ObjectField>>),
    /// Not understood: not checked.
    Any,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField {
    pub name: String,
    pub required: bool,
    pub type_: FieldType,
}

impl FieldType {
    pub fn parse(source: &str) -> FieldType {
        let source = source.trim();
        match source {
            "string" => return FieldType::Text,
            "number" => return FieldType::Number,
            "boolean" => return FieldType::Boolean,
            "object" => return FieldType::Object(None),
            _ => {}
        }
        if let Some(inner) = source.strip_prefix("list of ") {
            return FieldType::List(Box::new(FieldType::parse(inner)));
        }
        if let Some(inner) = source.strip_suffix("[]") {
            return FieldType::List(Box::new(FieldType::parse(inner)));
        }
        if let Some(body) = source
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
        {
            let mut fields = Vec::new();
            for part in split_top_level(body) {
                let Some((name, type_)) = part.split_once(':') else {
                    return FieldType::Any;
                };
                let name = name.trim();
                let (name, required) = match name.strip_suffix('?') {
                    Some(name) => (name.trim(), false),
                    None => (name, true),
                };
                fields.push(ObjectField {
                    name: name.to_string(),
                    required,
                    type_: FieldType::parse(type_),
                });
            }
            return FieldType::Object(Some(fields));
        }
        if let Some(options) = literal_union(source) {
            return FieldType::Options(options);
        }
        FieldType::Any
    }
}

fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in body.char_indices() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect()
}

/// The site path of a page's image folder, served from `public/`: the collection's URL prefix
/// and the page's file name (`/blog/<file name without .md>` → `/blog/my-post`). Without a
/// prefix, the collection's folder under `content/` stands in.
pub fn page_image_folder(url_pattern: &str, folder: &str, slug: &str) -> String {
    let prefix = match url_pattern.find('<') {
        Some(open) => &url_pattern[..open],
        None => "",
    };
    let prefix = prefix.trim_matches('/');
    let prefix = if prefix.is_empty() {
        let folder = folder.trim_matches('/');
        folder.strip_prefix("content/").unwrap_or(folder)
    } else {
        prefix
    };
    if prefix.is_empty() {
        format!("/{slug}")
    } else {
        format!("/{prefix}/{slug}")
    }
}

#[derive(Debug, Clone)]
pub struct Collection {
    pub schema: CollectionSchema,
    pub fields: Vec<(FieldSchema, FieldType)>,
    url_prefix: String,
    url_suffix: String,
}

impl Collection {
    fn new(schema: CollectionSchema) -> Collection {
        let fields = schema
            .fields
            .iter()
            .map(|field| (field.clone(), FieldType::parse(&field.type_)))
            .collect();
        let (url_prefix, url_suffix) = match (schema.url.find('<'), schema.url.rfind('>')) {
            (Some(open), Some(close)) if open < close => (
                schema.url[..open].to_string(),
                schema.url[close + 1..].to_string(),
            ),
            _ => (schema.url.clone(), String::new()),
        };
        Collection {
            schema,
            fields,
            url_prefix,
            url_suffix,
        }
    }

    fn folder_prefix(&self) -> String {
        format!("{}/", self.schema.folder.trim_end_matches('/'))
    }

    /// The page name of a document in this collection (`content/blog/a.md` → `a`).
    pub fn slug_of(&self, path: &str) -> Option<String> {
        let rest = path.strip_prefix(&self.folder_prefix())?;
        Some(rest.strip_suffix(".md")?.to_string())
    }

    pub fn url_of(&self, slug: &str) -> String {
        format!("{}{}{}", self.url_prefix, slug, self.url_suffix)
    }

    /// Where a page's images go, as a site path (`/blog/my-post`); see [`page_image_folder`].
    pub fn image_folder(&self, slug: &str) -> String {
        page_image_folder(&self.schema.url, &self.schema.folder, slug)
    }

    /// The file a site path is served from, when the path is one of this collection's pages.
    pub fn file_for_url(&self, path: &str) -> Option<String> {
        if !self.schema.url.contains('<') {
            return None;
        }
        let slug = path
            .strip_prefix(&self.url_prefix)?
            .strip_suffix(&self.url_suffix)?;
        if slug.is_empty() || slug.contains('/') {
            return None;
        }
        Some(format!("{}{slug}.md", self.folder_prefix()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ComponentsFile {
    Wrapped { components: Vec<ComponentSchema> },
    Bare(Vec<ComponentSchema>),
}

#[derive(Debug, Deserialize)]
struct CollectionsFile {
    collections: Vec<CollectionSchema>,
}

/// Both schemas, parsed once and reused for every document.
#[derive(Debug, Clone, Default)]
pub struct Schemas {
    /// Empty without `components.json`: then only the component structure is checked.
    pub components: Vec<ComponentSchema>,
    pub collections: Vec<Collection>,
}

#[derive(Debug)]
pub struct SchemaError {
    pub file: &'static str,
    pub message: String,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "schema/{} can't be read: {}", self.file, self.message)
    }
}

impl std::error::Error for SchemaError {}

impl Schemas {
    /// From the texts of `schema/components.json` and `schema/collections.json`; either may be absent.
    pub fn from_json(
        components: Option<&str>,
        collections: Option<&str>,
    ) -> Result<Schemas, SchemaError> {
        let components = match components {
            None => Vec::new(),
            Some(text) => match serde_json::from_str::<ComponentsFile>(text) {
                Ok(ComponentsFile::Wrapped { components })
                | Ok(ComponentsFile::Bare(components)) => components,
                Err(error) => {
                    return Err(SchemaError {
                        file: "components.json",
                        message: error.to_string(),
                    });
                }
            },
        };
        let collections = match collections {
            None => Vec::new(),
            Some(text) => serde_json::from_str::<CollectionsFile>(text)
                .map_err(|error| SchemaError {
                    file: "collections.json",
                    message: error.to_string(),
                })?
                .collections
                .into_iter()
                .map(Collection::new)
                .collect(),
        };
        Ok(Schemas {
            components,
            collections,
        })
    }

    pub fn component(&self, name: &str) -> Option<&ComponentSchema> {
        self.components
            .iter()
            .find(|component| component.name == name)
    }

    pub fn collection_for(&self, path: &str) -> Option<(&Collection, String)> {
        self.collections
            .iter()
            .find_map(|collection| collection.slug_of(path).map(|slug| (collection, slug)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_folders_follow_the_url_prefix() {
        assert_eq!(page_image_folder("/blog/<file name without .md>", "content/blog", "a"), "/blog/a");
        assert_eq!(
            page_image_folder("/cuisines/<file name without .md>-catering", "content/cuisines", "thai"),
            "/cuisines/thai"
        );
        assert_eq!(page_image_folder("/guides/<x>", "content/catering-guides", "a"), "/guides/a");
        assert_eq!(page_image_folder("", "content/dishes", "pizza"), "/dishes/pizza");
    }

    #[test]
    fn parses_collection_types() {
        assert_eq!(
            FieldType::parse("list of { category: string, items: list of string }"),
            FieldType::Object(None).clone().pipe_list(vec![
                ObjectField {
                    name: "category".into(),
                    required: true,
                    type_: FieldType::Text
                },
                ObjectField {
                    name: "items".into(),
                    required: true,
                    type_: FieldType::List(Box::new(FieldType::Text))
                },
            ])
        );
        assert_eq!(
            FieldType::parse("\"a\" | \"b\""),
            FieldType::Options(vec![Value::Str("a".into()), Value::Str("b".into())])
        );
        assert_eq!(
            FieldType::parse("list of object"),
            FieldType::List(Box::new(FieldType::Object(None)))
        );
    }

    impl FieldType {
        fn pipe_list(self, fields: Vec<ObjectField>) -> FieldType {
            FieldType::List(Box::new(FieldType::Object(Some(fields))))
        }
    }

    #[test]
    fn kebab_cases_like_the_editor() {
        assert_eq!(kebab_case("labelHeading"), "label-heading");
        assert_eq!(kebab_case("structured-data"), "structured-data");
        assert_eq!(kebab_case("h1Title"), "h1-title");
    }

    #[test]
    fn maps_urls_to_files() {
        let schemas = Schemas::from_json(
            None,
            Some(r#"{"collections":[{"name":"cuisines","folder":"content/cuisines","url":"/cuisines/<file name without .md>-catering","fields":[]}]}"#),
        )
        .unwrap();
        let cuisines = &schemas.collections[0];
        assert_eq!(
            cuisines.file_for_url("/cuisines/thai-catering").as_deref(),
            Some("content/cuisines/thai.md")
        );
        assert_eq!(cuisines.file_for_url("/cuisines/thai"), None);
        assert_eq!(cuisines.url_of("thai"), "/cuisines/thai-catering");
    }
}
