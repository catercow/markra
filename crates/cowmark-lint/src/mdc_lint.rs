//! Component checks, ported from the editor's `mdc-lint.ts` (`BlockLinter`) and the prop
//! reading in `form-model.ts` (`readMdcProps`). Messages and ranges match the TypeScript.

use indexmap::IndexMap;

use crate::mdc::{AttributeKind, MdcBlock, parse_attributes};
use crate::refs::Context;
use crate::schema::{ComponentSchema, PropSchema, Schemas, is_decimal, kebab_case};
use crate::text::{LineIndex, Range};
use crate::value::Value;
use crate::yaml::{self, Node, NodeKind};
use crate::{Report, Rule, Severity};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Attribute,
    Yaml,
    Default,
}

pub fn check_block(
    text: &str,
    block: &MdcBlock,
    schemas: Option<&Schemas>,
    context: &Context,
    report: &mut Report,
) {
    let messages = report.messages;
    if !block.closed {
        let closer = ":".repeat(block.colons);
        report.add(
            block.name_range,
            Severity::Error,
            Rule::UnclosedComponent,
            &messages.unclosed,
            &[("closer", &closer)],
        );
    }
    if let Some(props) = block.props
        && !block.props_closed
    {
        let line = LineIndex::new(text).line_at(props.from);
        report.add(
            Range::new(props.from, line.to),
            Severity::Error,
            Rule::UnclosedProps,
            &messages.unclosed_props,
            &[],
        );
    }

    let read = PropsText::read(text, block);
    if let Some(schemas) = schemas {
        match schemas.component(&block.name) {
            Some(component) => BlockLinter {
                text,
                block,
                report: &mut *report,
            }
            .props(component, &read),
            None => report.add(
                block.name_range,
                Severity::Error,
                Rule::UnknownComponent,
                &messages.unknown_component,
                &[("name", &block.name)],
            ),
        }
    }
    read.check_references(context, report);
}

/// The two places a block's props are written in, parsed.
struct PropsText<'a> {
    /// Between the braces, and the offset of its first byte.
    attributes: Option<(&'a str, usize)>,
    /// Between the `---` fences (empty for an empty block), and the offset of its first byte.
    yaml: Option<(&'a str, usize)>,
    parsed: Option<Result<yaml::Parsed, yaml::YamlError>>,
}

impl<'a> PropsText<'a> {
    fn read(text: &'a str, block: &MdcBlock) -> PropsText<'a> {
        let attributes = block
            .attributes
            .map(|range| (&text[range.from + 1..range.to - 1], range.from + 1));
        let yaml = match (block.props_content, block.props) {
            (Some(content), _) => Some((&text[content.from..content.to], content.from)),
            (None, Some(_)) => Some(("", 0)),
            _ => None,
        };
        let parsed = yaml
            .filter(|(source, _)| !source.trim().is_empty())
            .map(|(source, _)| yaml::parse(source));
        PropsText {
            attributes,
            yaml,
            parsed,
        }
    }

    fn yaml_root(&self) -> Option<&Node> {
        match &self.parsed {
            Some(Ok(parsed)) => parsed.root.as_ref(),
            _ => None,
        }
    }

    /// Images and site links in prop values, wherever they are written.
    fn check_references(&self, context: &Context, report: &mut Report) {
        if let Some((source, base)) = self.attributes {
            for attribute in parse_attributes(source) {
                let range = Range::new(base + attribute.from, base + attribute.to);
                match attribute.kind {
                    AttributeKind::Text => context.check_value(&attribute.value, range, report),
                    AttributeKind::Bound => {
                        if let Some(value) = bound_value(&attribute.value) {
                            for_each_string(&value, &mut |text| {
                                context.check_value(text, range, report)
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        if let (Some(root), Some((_, base))) = (self.yaml_root(), self.yaml) {
            for_each_scalar(root, &mut |node, text| {
                context.check_value(text, Range::new(base + node.from, base + node.to), report)
            });
        }
    }
}

fn for_each_string(value: &Value, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::Str(text) => visit(text),
        Value::Arr(items) => items.iter().for_each(|item| for_each_string(item, visit)),
        Value::Obj(entries) => entries
            .iter()
            .for_each(|(_, item)| for_each_string(item, visit)),
        _ => {}
    }
}

fn for_each_scalar(node: &Node, visit: &mut dyn FnMut(&Node, &str)) {
    match &node.kind {
        NodeKind::Scalar {
            value: Value::Str(text),
            ..
        } => visit(node, text),
        NodeKind::Scalar { .. } => {}
        NodeKind::Seq(items) => items.iter().for_each(|item| for_each_scalar(item, visit)),
        NodeKind::Map(pairs) => pairs
            .iter()
            .for_each(|(_, value)| for_each_scalar(value, visit)),
    }
}

/// `:key='…'` holds JSON, else YAML.
fn bound_value(source: &str) -> Option<Value> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(source) {
        return Some(Value::from_json(json));
    }
    match yaml::parse(source) {
        Ok(parsed) => Some(parsed.root.map_or(Value::Null, |root| root.to_value())),
        Err(_) => None,
    }
}

/// A string attribute's value, coerced only where the schema expects a number or boolean.
fn attribute_value(kind: &AttributeKind, text: &str, prop: Option<&PropSchema>) -> Option<Value> {
    match kind {
        AttributeKind::Bare => return Some(Value::Bool(true)),
        AttributeKind::Bound => return bound_value(text),
        _ => {}
    }
    let numeric = is_decimal(text);
    let type_ = prop.and_then(|prop| prop.type_.as_deref());
    if type_ == Some("number") && numeric {
        return Some(Value::Num(text.parse().unwrap_or(f64::NAN)));
    }
    if type_ == Some("boolean") && (text == "true" || text == "false") {
        return Some(Value::Bool(text == "true"));
    }
    if let Some(prop) = prop
        && numeric
        && let Ok(number) = text.parse::<f64>()
        && prop
            .select_options()
            .is_some_and(|options| options.contains(&Value::Num(number)))
    {
        return Some(Value::Num(number));
    }
    Some(Value::Str(text.to_string()))
}

enum Expectation {
    Text,
    Number,
    Boolean,
    List,
    Option(String),
}

/// What a value should have been, or None when it fits the prop (or the type is not checked).
fn expectation(prop: &PropSchema, value: &Value) -> Option<Expectation> {
    if value.is_nullish() {
        return None;
    }
    if let Some(options) = prop.select_options() {
        let text = value.js_string();
        if options.iter().any(|option| option.js_string() == text) {
            return None;
        }
        let listed = options
            .iter()
            .map(|option| {
                if *option == Value::Str(String::new()) {
                    "\"\"".to_string()
                } else {
                    option.js_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Some(Expectation::Option(listed));
    }
    let type_ = prop.type_.as_deref().unwrap_or("").trim();
    let ok = match type_ {
        "string" => matches!(value, Value::Str(_)),
        "number" => matches!(value, Value::Num(_)),
        "boolean" => matches!(value, Value::Bool(_)),
        "string[]" => {
            matches!(value, Value::Arr(items) if items.iter().all(|item| matches!(item, Value::Str(_))))
        }
        _ if type_.ends_with("[]") => matches!(value, Value::Arr(_)),
        _ => return None,
    };
    if ok {
        return None;
    }
    Some(match type_ {
        "string" => Expectation::Text,
        "number" => Expectation::Number,
        "boolean" => Expectation::Boolean,
        _ => Expectation::List,
    })
}

struct BlockLinter<'a, 'r, 'm> {
    text: &'a str,
    block: &'a MdcBlock,
    report: &'r mut Report<'m>,
}

/// Where each prop is written, for pointing diagnostics at it.
struct Locations<'n> {
    attributes: IndexMap<String, Range>,
    /// Top-level YAML pairs by kebab-case key; None when the YAML could not be read as a map.
    yaml: Option<IndexMap<String, (&'n Node, &'n Node)>>,
    yaml_base: usize,
}

fn node_range(node: &Node, base: usize) -> Range {
    Range::new(base + node.from, base + node.to)
}

fn pair_range(key: &Node, value: &Node, base: usize) -> Range {
    let key_range = node_range(key, base);
    let to = if value.is_map() || value.is_seq() {
        key_range.to
    } else {
        base + value.to
    };
    Range::new(key_range.from, to)
}

impl BlockLinter<'_, '_, '_> {
    fn report_expectation(&mut self, range: Range, expected: Expectation, prop: &str) {
        let messages = self.report.messages;
        let (template, options) = match &expected {
            Expectation::Text => (&messages.expect_text, ""),
            Expectation::Number => (&messages.expect_number, ""),
            Expectation::Boolean => (&messages.expect_boolean, ""),
            Expectation::List => (&messages.expect_list, ""),
            Expectation::Option(options) => (&messages.expect_option, options.as_str()),
        };
        self.report.add(
            range,
            Severity::Warning,
            Rule::PropType,
            template,
            &[("options", options), ("prop", prop)],
        );
    }

    fn props(mut self, component: &ComponentSchema, read: &PropsText) {
        let messages = self.report.messages;
        let props: IndexMap<String, &PropSchema> = component
            .props
            .iter()
            .map(|prop| (kebab_case(&prop.name), prop))
            .collect();

        // readMdcProps: attributes first, then YAML, which wins when both set a prop.
        let mut values: IndexMap<String, Value> = IndexMap::new();
        let mut source: IndexMap<String, Source> = IndexMap::new();
        if let Some((attributes, _)) = read.attributes {
            for attribute in parse_attributes(attributes) {
                if matches!(attribute.kind, AttributeKind::Id | AttributeKind::Class) {
                    continue;
                }
                let name = kebab_case(&attribute.key);
                let Some(value) =
                    attribute_value(&attribute.kind, &attribute.value, props.get(&name).copied())
                else {
                    continue;
                };
                values.insert(name.clone(), value);
                source.insert(name, Source::Attribute);
            }
        }
        let yaml_error = match &read.parsed {
            Some(Err(_)) => true,
            Some(Ok(parsed)) => match &parsed.root {
                Some(root) if root.is_map() => {
                    if let Value::Obj(entries) = root.to_value() {
                        for (key, value) in entries {
                            let name = kebab_case(&key);
                            values.insert(name.clone(), value);
                            source.insert(name, Source::Yaml);
                        }
                    }
                    false
                }
                Some(_) => true,
                None => false,
            },
            None => false,
        };
        let unknown: Vec<String> = source
            .keys()
            .filter(|name| !props.contains_key(*name))
            .cloned()
            .collect();
        for (name, prop) in &props {
            if source.contains_key(name) {
                continue;
            }
            if let Some(fallback) = prop.schema_default() {
                values.insert(name.clone(), fallback);
                source.insert(name.clone(), Source::Default);
            }
        }

        let locations = self.locate(read);
        if yaml_error {
            self.invalid_yaml(read);
        }

        for name in &unknown {
            let range = self.locate_prop(&locations, name, source.get(name).copied());
            self.report.add(
                range,
                Severity::Warning,
                Rule::UnknownProp,
                &messages.unknown_prop,
                &[("prop", name)],
            );
        }

        for prop in &component.props {
            let name = kebab_case(&prop.name);
            let from = source.get(&name).copied();
            if !matches!(from, Some(Source::Attribute | Source::Yaml)) {
                if prop.is_required() && prop.schema_default().is_none() && !yaml_error {
                    self.report.add(
                        self.block.name_range,
                        Severity::Error,
                        Rule::MissingProp,
                        &messages.missing_prop,
                        &[("prop", &name)],
                    );
                }
                continue;
            }

            let range = self.locate_prop(&locations, &name, from);
            let value = &values[&name];
            if value.is_nullish() {
                if prop.is_required() {
                    self.report.add(
                        range,
                        Severity::Error,
                        Rule::MissingProp,
                        &messages.missing_prop,
                        &[("prop", &name)],
                    );
                }
                continue;
            }
            if let Some(expected) = expectation(prop, value) {
                self.report_expectation(range, expected, &name);
                continue;
            }
            if !prop.items().is_empty()
                && let Value::Arr(items) = value
            {
                let node = match from {
                    Some(Source::Yaml) => locations
                        .yaml
                        .as_ref()
                        .and_then(|yaml| yaml.get(&name))
                        .map(|(_, value)| *value),
                    _ => None,
                };
                self.items(prop, items, range, node, locations.yaml_base);
            }
        }
    }

    fn items(
        &mut self,
        prop: &PropSchema,
        items: &[Value],
        fallback: Range,
        node: Option<&Node>,
        base: usize,
    ) {
        let messages = self.report.messages;
        let fields = prop.items();
        let sequence = match node.map(|node| &node.kind) {
            Some(NodeKind::Seq(nodes)) => Some(nodes),
            _ => None,
        };

        for (index, item) in items.iter().enumerate() {
            let item_node = sequence.and_then(|nodes| nodes.get(index));
            let item_range = item_node.map_or(fallback, |node| node_range(node, base));
            let Value::Obj(entries) = item else {
                self.report.add(
                    item_range,
                    Severity::Warning,
                    Rule::PropType,
                    &messages.expect_item,
                    &[
                        ("field", &fields[0].name),
                        ("prop", &kebab_case(&prop.name)),
                    ],
                );
                continue;
            };

            let mut pairs: IndexMap<String, (&Node, &Node)> = IndexMap::new();
            if let Some(node) = item_node {
                for (key, value) in node.pairs() {
                    if let Some(text) = key.key_text() {
                        pairs.insert(text, (key, value));
                    }
                }
            }
            let field_for = |key: &str| {
                fields.iter().position(|field| {
                    field.name == key || kebab_case(&field.name) == kebab_case(key)
                })
            };
            let key_range = |key: &str| {
                pairs
                    .get(key)
                    .map_or(item_range, |(key, value)| pair_range(key, value, base))
            };

            for (field_index, field) in fields.iter().enumerate() {
                let present = entries
                    .iter()
                    .find(|(key, _)| field_for(key) == Some(field_index));
                let missing = present.is_none_or(|(_, value)| value.is_nullish());
                if field.is_required() && missing {
                    let range = self.first_line(item_range);
                    self.report.add(
                        range,
                        Severity::Error,
                        Rule::MissingItemField,
                        &messages.missing_item_field,
                        &[("prop", &field.name)],
                    );
                    continue;
                }
                if let Some((key, value)) = present
                    && let Some(expected) = expectation(field, value)
                {
                    self.report_expectation(key_range(key), expected, &field.name);
                }
            }
            for (key, _) in entries {
                if field_for(key).is_none() {
                    self.report.add(
                        key_range(key),
                        Severity::Warning,
                        Rule::UnknownItemField,
                        &messages.unknown_item_field,
                        &[("prop", key)],
                    );
                }
            }
        }
    }

    fn locate<'n>(&self, read: &'n PropsText) -> Locations<'n> {
        let mut locations = Locations {
            attributes: IndexMap::new(),
            yaml: None,
            yaml_base: self.block.props_content.map_or(0, |content| content.from),
        };
        if let Some((attributes, base)) = read.attributes {
            for attribute in parse_attributes(attributes) {
                if matches!(attribute.kind, AttributeKind::Id | AttributeKind::Class) {
                    continue;
                }
                locations.attributes.insert(
                    kebab_case(&attribute.key),
                    Range::new(base + attribute.from, base + attribute.to),
                );
            }
        }
        if let Some(root) = read.yaml_root()
            && root.is_map()
        {
            let mut yaml = IndexMap::new();
            for (key, value) in root.pairs() {
                if let Some(text) = key.key_text() {
                    yaml.insert(kebab_case(&text), (key, value));
                }
            }
            locations.yaml = Some(yaml);
        }
        locations
    }

    fn locate_prop(&self, locations: &Locations, name: &str, source: Option<Source>) -> Range {
        if source == Some(Source::Attribute)
            && let Some(range) = locations.attributes.get(name)
        {
            return *range;
        }
        if let Some((key, value)) = locations.yaml.as_ref().and_then(|yaml| yaml.get(name)) {
            return pair_range(key, value, locations.yaml_base);
        }
        self.block.props.unwrap_or(self.block.name_range)
    }

    fn invalid_yaml(&mut self, read: &PropsText) {
        let messages = self.report.messages;
        if let (Some(Err(error)), Some(content)) = (&read.parsed, self.block.props_content) {
            let index = LineIndex::new(self.text);
            let line = index.line_at((content.from + error.pos).min(self.text.len()));
            let indent = self.text[line.from..line.to].len()
                - self.text[line.from..line.to].trim_start().len();
            self.report.add(
                Range::new(line.from + indent, line.to),
                Severity::Error,
                Rule::PropsYaml,
                &messages.invalid_yaml,
                &[],
            );
            return;
        }
        let range = self.block.props.unwrap_or(self.block.name_range);
        self.report.add(
            range,
            Severity::Error,
            Rule::PropsYaml,
            &messages.invalid_yaml,
            &[],
        );
    }

    fn first_line(&self, range: Range) -> Range {
        let line = LineIndex::new(self.text).line_at(range.from);
        Range::new(range.from, range.to.min(line.to))
    }
}
