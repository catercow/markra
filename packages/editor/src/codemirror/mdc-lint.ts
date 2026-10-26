import { ensureSyntaxTree } from "@codemirror/language";
import { linter, type Diagnostic } from "@codemirror/lint";
import type { EditorState } from "@codemirror/state";
import { isMap, isScalar, isSeq, parseDocument, type Node as YamlNode, type Pair } from "yaml";
import { parseMdcAttributes } from "../mdc/attributes.ts";
import type { MdcCatalogComponent } from "../mdc/components.ts";
import { kebabCase, readMdcProps, schemaDefault, selectOptions, type MdcPropSchema } from "../mdc/form-model.ts";
import { readMdcBlocksFromTree, type MdcBlock, type MdcRange } from "./mdc-blocks.ts";
import { readMdcPropsText } from "./mdc-edit.ts";
import { defineMarkraPlugin } from "./plugin.ts";

/** Message templates; `{name}`, `{prop}`, `{field}`, `{options}` and `{closer}` are filled in. */
export interface MdcLintMessages {
  unknownComponent: string;
  missingProp: string;
  missingItemField: string;
  unknownProp: string;
  unknownItemField: string;
  expectText: string;
  expectNumber: string;
  expectBoolean: string;
  expectList: string;
  expectOption: string;
  expectItem: string;
  unclosed: string;
  unclosedProps: string;
  invalidYaml: string;
}

export const defaultMdcLintMessages: MdcLintMessages = {
  expectBoolean: "`{prop}` should be true or false.",
  expectItem: "Each item in `{prop}` needs its own fields, like `{field}: …`.",
  expectList: "`{prop}` should be a list.",
  expectNumber: "`{prop}` should be a number.",
  expectOption: "`{prop}` should be one of: {options}.",
  expectText: "`{prop}` should be text. If it is a number or true/false, put quotes around it.",
  invalidYaml: "The props block can't be read. Check the indentation and quotes around here.",
  missingItemField: "This component needs a `{prop}` for each item.",
  missingProp: "This component needs a `{prop}`.",
  unclosed: "This component isn't closed. Add a line with just `{closer}` where it ends.",
  unclosedProps: "The props block isn't closed. Add a line with just `---` after the last prop.",
  unknownComponent: "There is no “{name}” component. Check the spelling, or pick one from the Components list.",
  unknownItemField: "Items here don't use `{prop}`, so the site ignores it.",
  unknownProp: "This component doesn't use `{prop}`, so the site ignores it.",
};

type MessageKey = keyof MdcLintMessages;
type Expectation = { key: MessageKey; options?: string };

function fill(template: string, values: Record<string, string>) {
  return template.replace(/\{(\w+)\}/gu, (match, key: string) => values[key] ?? match);
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** What a value should have been, or null when it fits the prop (or the type is not checked). */
function expectation(prop: MdcPropSchema, value: unknown): Expectation | null {
  if (value === null || value === undefined) return null;
  const options = selectOptions(prop);
  if (options) {
    return options.some((option) => String(option) === String(value))
      ? null
      : { key: "expectOption", options: options.map((option) => (option === "" ? '""' : String(option))).join(", ") };
  }

  const type = (prop.type ?? "").trim();
  const text = (item: unknown) => typeof item === "string";
  if (type === "string") return text(value) ? null : { key: "expectText" };
  if (type === "number") return typeof value === "number" ? null : { key: "expectNumber" };
  if (type === "boolean") return typeof value === "boolean" ? null : { key: "expectBoolean" };
  if (type === "string[]") return Array.isArray(value) && value.every(text) ? null : { key: "expectList" };
  if (type.endsWith("[]")) return Array.isArray(value) ? null : { key: "expectList" };
  return null;
}

/** Positions of what a component's props are written in, for pointing diagnostics at them. */
interface PropLocations {
  attributes: Map<string, MdcRange>;
  /** Top-level YAML pairs by kebab-case key; null when the YAML could not be mapped back. */
  yaml: Map<string, Pair<YamlNode, YamlNode>> | null;
  yamlBase: number;
}

function nodeRange(node: YamlNode | null | undefined, base: number): MdcRange | null {
  const range = node?.range;
  return range ? { from: base + range[0], to: base + range[1] } : null;
}

function pairRange(pair: Pair<YamlNode, YamlNode>, base: number): MdcRange | null {
  const key = nodeRange(pair.key, base);
  const value = nodeRange(pair.value, base);
  if (!key) return value;
  return { from: key.from, to: value && !isMap(pair.value) && !isSeq(pair.value) ? value.to : key.to };
}

function pairKey(pair: Pair<YamlNode, YamlNode>) {
  const key = isScalar(pair.key) ? pair.key.value : pair.key;
  return typeof key === "string" || typeof key === "number" ? String(key) : null;
}

class BlockLinter {
  readonly diagnostics: Diagnostic[] = [];

  constructor(
    private readonly state: EditorState,
    private readonly block: MdcBlock,
    private readonly messages: MdcLintMessages,
  ) {}

  report(range: MdcRange, severity: Diagnostic["severity"], key: MessageKey, values: Record<string, string> = {}) {
    const message = fill(this.messages[key], values);
    this.diagnostics.push({
      from: range.from,
      message,
      renderMessage: (view) => renderMessage(view.dom.ownerDocument, message),
      severity,
      source: "MDC",
      to: Math.max(range.to, range.from),
    });
  }

  structure() {
    const { block } = this;
    if (!block.close) {
      this.report(block.nameRange, "error", "unclosed", { closer: ":".repeat(block.colons) });
    }
    if (block.props && !block.propsClosed) {
      const fence = this.state.doc.lineAt(block.props.from);
      this.report({ from: block.props.from, to: fence.to }, "error", "unclosedProps");
    }
  }

  props(component: MdcCatalogComponent) {
    const { block, state } = this;
    const text = readMdcPropsText(state, block);
    const read = readMdcProps({ ...text, schema: component });
    const yamlError = read.errors.find((error) => error.startsWith("Invalid YAML"));
    const locations = this.locate(text.attributes, text.yaml);

    if (yamlError) {
      this.invalidYaml(text.yaml ?? "");
    }

    for (const name of read.unknown) {
      this.report(this.where(locations, name, read.source[name]), "warning", "unknownProp", { prop: name });
    }

    for (const prop of component.props) {
      const name = kebabCase(prop.name);
      const source = read.source[name];
      const value = read.values[name];
      if (source !== "attribute" && source !== "yaml") {
        if (prop.required && schemaDefault(prop) === undefined && !yamlError) {
          this.report(block.nameRange, "error", "missingProp", { prop: name });
        }
        continue;
      }

      const range = this.where(locations, name, source);
      if (value === null || value === undefined) {
        if (prop.required) this.report(range, "error", "missingProp", { prop: name });
        continue;
      }
      const expected = expectation(prop, value);
      if (expected) {
        this.report(range, "warning", expected.key, { options: expected.options ?? "", prop: name });
        continue;
      }
      if (prop.items && prop.items.length > 0 && Array.isArray(value)) {
        const pair = source === "yaml" ? locations.yaml?.get(name) : undefined;
        this.items(prop, value, range, pair?.value ?? null, locations.yamlBase);
      }
    }
  }

  private items(prop: MdcPropSchema, value: unknown[], fallback: MdcRange, node: YamlNode | null, base: number) {
    const fields = prop.items ?? [];
    const sequence = node && isSeq(node) ? node : null;

    value.forEach((item, index) => {
      const itemNode = (sequence?.items[index] ?? null) as YamlNode | null;
      const itemRange = nodeRange(itemNode, base) ?? fallback;
      if (!isPlainObject(item)) {
        this.report(itemRange, "warning", "expectItem", { field: fields[0]!.name, prop: kebabCase(prop.name) });
        return;
      }

      const pairs = new Map<string, Pair<YamlNode, YamlNode>>();
      if (itemNode && isMap(itemNode)) {
        for (const pair of itemNode.items as Array<Pair<YamlNode, YamlNode>>) {
          const key = pairKey(pair);
          if (key !== null) pairs.set(key, pair);
        }
      }
      const fieldFor = (key: string) =>
        fields.find((field) => field.name === key || kebabCase(field.name) === kebabCase(key));
      const keyRange = (key: string) => {
        const pair = pairs.get(key);
        return (pair && pairRange(pair, base)) ?? itemRange;
      };

      for (const field of fields) {
        const present = Object.keys(item).find((key) => fieldFor(key) === field);
        const fieldValue = present === undefined ? undefined : item[present];
        if (field.required && (fieldValue === undefined || fieldValue === null)) {
          this.report(firstLine(this.state, itemRange), "error", "missingItemField", { prop: field.name });
          continue;
        }
        const expected = present === undefined ? null : expectation(field, fieldValue);
        if (expected) {
          this.report(keyRange(present!), "warning", expected.key, { options: expected.options ?? "", prop: field.name });
        }
      }
      for (const key of Object.keys(item)) {
        if (!fieldFor(key)) this.report(keyRange(key), "warning", "unknownItemField", { prop: key });
      }
    });
  }

  private locate(attributes: string | null, yaml: string | null): PropLocations {
    const { block } = this;
    const locations: PropLocations = { attributes: new Map(), yaml: null, yamlBase: block.propsContent?.from ?? 0 };

    if (attributes !== null && block.attributes) {
      const base = block.attributes.from + 1;
      for (const attribute of parseMdcAttributes(attributes).attributes) {
        if (attribute.kind === "id" || attribute.kind === "class") continue;
        if (attribute.from === undefined || attribute.to === undefined) continue;
        locations.attributes.set(kebabCase(attribute.key), { from: base + attribute.from, to: base + attribute.to });
      }
    }

    if (yaml) {
      const doc = parseDocument(yaml);
      if (doc.errors.length === 0 && isMap(doc.contents)) {
        locations.yaml = new Map();
        for (const pair of doc.contents.items as Array<Pair<YamlNode, YamlNode>>) {
          const key = pairKey(pair);
          if (key !== null) locations.yaml.set(kebabCase(key), pair);
        }
      }
    }
    return locations;
  }

  private where(locations: PropLocations, name: string, source: string | undefined): MdcRange {
    if (source === "attribute") {
      const range = locations.attributes.get(name);
      if (range) return range;
    }
    const pair = locations.yaml?.get(name);
    const range = pair && pairRange(pair, locations.yamlBase);
    if (range) return range;
    return this.block.props ?? this.block.nameRange;
  }

  private invalidYaml(yaml: string) {
    const { block } = this;
    const error = parseDocument(yaml).errors[0];
    const base = block.propsContent?.from;
    if (error && base !== undefined) {
      const line = this.state.doc.lineAt(Math.min(base + error.pos[0], this.state.doc.length));
      this.report({ from: line.from + /^\s*/u.exec(line.text)![0].length, to: line.to }, "error", "invalidYaml");
      return;
    }
    this.report(block.props ?? block.nameRange, "error", "invalidYaml");
  }
}

/** The message with each `name` set as code. */
function renderMessage(document: Document, message: string) {
  const element = document.createElement("span");
  message.split("`").forEach((part, index) => {
    if (index % 2 === 0) {
      element.append(part);
      return;
    }
    const code = document.createElement("code");
    code.textContent = part;
    element.append(code);
  });
  return element;
}

function firstLine(state: EditorState, range: MdcRange): MdcRange {
  const line = state.doc.lineAt(range.from);
  return { from: range.from, to: Math.min(range.to, line.to) };
}

/**
 * Problems with the MDC components in a document: unknown components, missing, unknown and
 * mistyped props, and unclosed blocks. Component and prop checks need `components`; without a
 * schema only the structure is checked.
 */
export function lintMdcDocument(
  state: EditorState,
  components: readonly MdcCatalogComponent[] | null | undefined,
  messages: Partial<MdcLintMessages> = {},
): Diagnostic[] {
  // A partial tree would report every block past the parsed part as unclosed.
  const tree = ensureSyntaxTree(state, state.doc.length, 2000);
  if (!tree) return [];

  const allMessages = { ...defaultMdcLintMessages, ...messages };
  const byName = new Map((components ?? []).map((component) => [component.name, component]));
  const diagnostics: Diagnostic[] = [];

  for (const block of readMdcBlocksFromTree(tree, state.doc)) {
    const linter = new BlockLinter(state, block, allMessages);
    linter.structure();
    const component = byName.get(block.name);
    if (component) linter.props(component);
    else if (byName.size > 0) linter.report(block.nameRange, "error", "unknownComponent", { name: block.name });
    diagnostics.push(...linter.diagnostics);
  }

  return diagnostics.sort((left, right) => left.from - right.from || left.to - right.to);
}

export interface MdcLintPluginOptions {
  enabled?: boolean;
  /** The workspace's components, read on each lint pass. */
  components?: () => readonly MdcCatalogComponent[] | null | undefined;
  messages?: Partial<MdcLintMessages>;
}

/** Underlines MDC problems and explains them on hover. Never changes the text. */
export function mdcLintPlugin({ components, enabled = true, messages }: MdcLintPluginOptions = {}) {
  return defineMarkraPlugin({
    id: "markra.mdc-lint",
    extension: enabled
      ? linter((view) => lintMdcDocument(view.state, components?.(), messages), { delay: 500 })
      : [],
  });
}
