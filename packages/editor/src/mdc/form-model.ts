// Form model over an MDC component's props, which live in two places: the opener's inline
// attributes and the YAML block between `---` fences right after it. Reads merge both; writes
// make the smallest text edit, keeping untouched keys, order, comments and quoting as they were.

import { Document, isMap, isScalar, isSeq, parseDocument, Scalar, type ToStringOptions } from "yaml";
import { type MdcAttribute, type MdcAttributeList, parseMdcAttributes, serializeMdcAttributes } from "./attributes.ts";

export interface MdcPropSchema {
  /** Kebab-case for component props; item fields keep their authored spelling. */
  name: string;
  /** TypeScript type source, e.g. `string`, `"a" | "b"`, `2 | 3`, `string[]`, `Faq[]`; absent edits as JSON. */
  type?: string;
  required?: boolean;
  description?: string | null;
  /** JavaScript source of the default value, e.g. `""`, `[]`, `false`. */
  default?: string | null;
  options?: Array<string | number> | null;
  /** Field definitions for an array of objects. */
  items?: MdcPropSchema[] | null;
}

/** Structurally compatible with `MdcComponentSchema` from `@markra/shared`. */
export interface MdcComponentSchema {
  name: string;
  props: MdcPropSchema[];
}

export type MdcFieldKind = "text" | "textarea" | "number" | "boolean" | "select" | "string-list" | "object-list" | "json";
export type MdcPropSource = "attribute" | "yaml" | "default";
export type MdcPropPath = Array<string | number>;
/** A top-level prop name, a path like `items[1].answer`, or a path array like `["items", 1, "answer"]`. */
export type MdcPropKey = string | MdcPropPath;

export interface MdcPropsInput {
  /** Text between the opener's `{` and `}`, or null when the opener has none. */
  attributes: string | null;
  /** Text between the `---` fences, or null when there is no YAML block. */
  yaml: string | null;
  schema?: MdcComponentSchema;
}

export interface MdcPropsRead {
  /** Props by kebab-case name; `#id` / `.class` attributes are not props and are left out. */
  values: Record<string, unknown>;
  source: Record<string, MdcPropSource>;
  /** Props present in the source but not in the schema (every present prop when there is no schema). */
  unknown: string[];
  /** YAML parse errors, and bound attributes whose value could not be parsed. */
  errors: string[];
}

export interface MdcPropsWrite {
  attributes: string | null;
  yaml: string | null;
  /** Set when the edit was refused (invalid YAML block); the texts are then returned unchanged. */
  error?: string;
}

/** Longest string kept as an inline attribute; longer text moves to the YAML block. */
export const MDC_INLINE_STRING_MAX = 80;

const yamlOutput: ToStringOptions = {
  // `\n` escapes inside double quotes stay escapes instead of becoming folded lines.
  doubleQuotedAsJSON: true,
  lineWidth: 0
};

const proseNames = /(^|[-_])(description|answer|body|note|caption|subhead|summary|quote)$/;
const proseDescription = /\b(paragraph|multi-?line)\b/i;

export function kebabCase(name: string) {
  return name.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
}

/** Options for a select field: `options` from the schema, else the members of a literal union type. */
export function selectOptions(prop: MdcPropSchema): Array<string | number> | null {
  if (prop.options && prop.options.length > 0) return prop.options;

  const members = (prop.type ?? "").split("|").map((member) => member.trim());
  if (members.length < 2) return null;

  const values: Array<string | number> = [];
  for (const member of members) {
    const quoted = /^(["'])(.*)\1$/.exec(member);
    if (quoted) values.push(quoted[2]!);
    else if (/^-?\d+(\.\d+)?$/.test(member)) values.push(Number(member));
    else return null;
  }
  return values;
}

export function fieldKind(prop: MdcPropSchema): MdcFieldKind {
  const type = (prop.type ?? "").trim();
  if (selectOptions(prop)) return "select";
  if (type === "boolean") return "boolean";
  if (type === "number") return "number";
  if (type === "string") {
    return proseNames.test(prop.name) || proseDescription.test(prop.description ?? "") ? "textarea" : "text";
  }
  if (type === "string[]") return "string-list";
  if (/^[A-Za-z_$][\w$]*\[\]$/.test(type) && prop.items && prop.items.length > 0) return "object-list";
  return "json";
}

/** The schema default as a value, or undefined when there is none or it is not a literal. */
export function schemaDefault(prop: MdcPropSchema | undefined): unknown {
  const source = prop?.default?.trim();
  if (!source) return undefined;
  try {
    return JSON.parse(source);
  } catch {
    const single = /^'((?:[^'\\]|\\.)*)'$/.exec(source);
    return single ? single[1]!.replace(/\\(.)/g, "$1") : undefined;
  }
}

export function readMdcProps(input: MdcPropsInput): MdcPropsRead {
  const values: Record<string, unknown> = {};
  const source: Record<string, MdcPropSource> = {};
  const errors: string[] = [];
  const props = propsByName(input.schema);

  for (const attribute of parseAttributes(input.attributes).attributes) {
    if (attribute.kind === "id" || attribute.kind === "class") continue;
    const name = kebabCase(attribute.key);
    const value = attributeValue(attribute, props.get(name), errors);
    if (value === undefined) continue;
    values[name] = value;
    source[name] = "attribute";
  }

  // YAML is applied after the attributes, so it wins when both set a prop.
  const yaml = loadYaml(input.yaml);
  if (yaml.error) {
    errors.push(yaml.error);
  } else if (yaml.doc && isMap(yaml.doc.contents)) {
    const data = yaml.doc.toJS() as Record<string, unknown> | null;
    for (const [key, value] of Object.entries(data ?? {})) {
      const name = kebabCase(key);
      values[name] = value;
      source[name] = "yaml";
    }
  }

  const unknown = Object.keys(source).filter((name) => !props.has(name));

  for (const [name, prop] of props) {
    if (name in source) continue;
    const fallback = schemaDefault(prop);
    if (fallback === undefined) continue;
    values[name] = fallback;
    source[name] = "default";
  }

  return { errors, source, unknown, values };
}

export function writeMdcProp(input: MdcPropsInput, key: MdcPropKey, value: unknown): MdcPropsWrite {
  const unchanged = { attributes: input.attributes, yaml: input.yaml };
  const path = normalizePath(key);
  if (path.length === 0) return unchanged;

  const yaml = loadYaml(input.yaml);
  if (yaml.error) return { ...unchanged, error: yaml.error };

  const name = path[0] as string;
  const prop = propsByName(input.schema).get(name);
  const list = parseAttributes(input.attributes);
  const attributeIndex = findLastIndex(
    list.attributes,
    (attribute) => attribute.kind !== "id" && attribute.kind !== "class" && kebabCase(attribute.key) === name
  );
  const doc = yaml.doc ?? new Document({});
  const yamlKey = findYamlKey(doc, name);

  let topValue: unknown;
  if (path.length === 1) {
    topValue = value;
  } else {
    const current = readMdcProps(input).values[name];
    topValue = setAtPath(structuredClone(current ?? (typeof path[1] === "number" ? [] : {})), path.slice(1), value);
  }

  const removing = topValue === undefined || (prop !== undefined && deepEqual(topValue, schemaDefault(prop)));

  if (removing) {
    if (attributeIndex >= 0) removeAttribute(list, attributeIndex);
    if (yamlKey !== undefined) doc.deleteIn([yamlKey]);
    return finish(list, doc, input, attributeIndex >= 0, yamlKey !== undefined);
  }

  if (attributeIndex >= 0 && yamlKey === undefined && path.length === 1 && isInlineValue(value)) {
    list.attributes[attributeIndex] = inlineAttribute(list.attributes[attributeIndex]!, value);
    return finish(list, doc, input, true, false);
  }

  if (attributeIndex >= 0) removeAttribute(list, attributeIndex);

  if (yamlKey !== undefined && path.length > 1 && canEditInPlace(doc, [yamlKey, ...path.slice(1)])) {
    applyValue(doc, [yamlKey, ...path.slice(1)], value);
  } else {
    applyValue(doc, [yamlKey ?? name], topValue);
  }
  return finish(list, doc, input, attributeIndex >= 0, true);
}

function finish(list: MdcAttributeList, doc: Document, input: MdcPropsInput, attributesChanged: boolean, yamlChanged: boolean): MdcPropsWrite {
  return {
    attributes: attributesChanged ? (list.attributes.length > 0 ? serializeMdcAttributes(list) : null) : input.attributes,
    yaml: yamlChanged ? stringifyYaml(doc, input.yaml) : input.yaml
  };
}

function removeAttribute(list: MdcAttributeList, index: number) {
  const [removed] = list.attributes.splice(index, 1);
  const next = list.attributes[index];
  // The first attribute's spacing hugs the `{`; the one moving into its place takes it over.
  if (index === 0 && next && removed) list.attributes[0] = { ...next, leading: removed.leading };
}

function propsByName(schema: MdcComponentSchema | undefined) {
  return new Map((schema?.props ?? []).map((prop) => [kebabCase(prop.name), prop] as const));
}

function parseAttributes(src: string | null): MdcAttributeList {
  return src === null ? { attributes: [], trailing: "" } : parseMdcAttributes(src);
}

function attributeValue(attribute: MdcAttribute, prop: MdcPropSchema | undefined, errors: string[]): unknown {
  if (attribute.kind === "bare") return true;
  const text = String(attribute.value);

  if (attribute.kind === "bound") {
    try {
      return JSON.parse(text);
    } catch {
      const doc = parseDocument(text);
      if (doc.errors.length === 0) return doc.toJS();
      errors.push(`Attribute :${attribute.key} is not valid JSON or YAML: ${doc.errors[0]!.message}`);
      return undefined;
    }
  }

  // String attributes are always text in MDC; coerce only where the schema expects otherwise.
  const numeric = /^-?\d+(\.\d+)?$/.test(text);
  if (prop?.type === "number" && numeric) return Number(text);
  if (prop?.type === "boolean" && (text === "true" || text === "false")) return text === "true";
  if (prop && numeric && selectOptions(prop)?.includes(Number(text))) return Number(text);
  return text;
}

function isInlineValue(value: unknown) {
  if (typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  return typeof value === "string" && value.length <= MDC_INLINE_STRING_MAX && !/[\n\r\t]/.test(value);
}

function inlineAttribute(previous: MdcAttribute, value: unknown): MdcAttribute {
  const base = { key: previous.key, leading: previous.leading };
  if (value === true) return { ...base, kind: "bare", quote: "", value: true };
  if (typeof value === "string") {
    return { ...base, kind: "string", quote: previous.kind === "string" ? previous.quote : '"', value };
  }
  const text = String(value);
  if (typeof value === "number" && previous.kind === "string" && previous.quote === "") {
    return { ...base, kind: "string", quote: "", value: text };
  }
  return { ...base, kind: "bound", quote: previous.kind === "bound" && previous.quote ? previous.quote : '"', value: text };
}

interface LoadedYaml {
  doc: Document | null;
  error?: string;
}

function loadYaml(text: string | null): LoadedYaml {
  if (text === null || text.trim() === "") return { doc: null };
  const doc = parseDocument(dedent(text).body);
  if (doc.errors.length > 0) return { doc: null, error: `Invalid YAML: ${doc.errors[0]!.message}` };
  if (doc.contents !== null && !isMap(doc.contents)) {
    return { doc: null, error: "Invalid YAML: the props block must be a mapping of prop names to values." };
  }
  return { doc };
}

function stringifyYaml(doc: Document, original: string | null): string | null {
  if (!isMap(doc.contents) || doc.contents.items.length === 0) return null;

  const { indent } = original === null ? { indent: "" } : dedent(original);
  let body = doc.toString(yamlOutput);
  // The trailing newline follows the original; a new block has none.
  if (original === null || !original.endsWith("\n")) body = body.replace(/\n$/, "");

  return indent ? body.replace(/^(?=.)/gm, indent) : body;
}

/** Strips the common indentation of a YAML block nested inside another component. */
function dedent(text: string) {
  const lines = text.split("\n").filter((line) => line.trim() !== "");
  const indent = lines.reduce<string | null>((common, line) => {
    const own = /^ */.exec(line)![0];
    return common === null || own.length < common.length ? own : common;
  }, null) ?? "";
  if (!indent) return { body: text, indent };
  return { body: text.split("\n").map((line) => (line.startsWith(indent) ? line.slice(indent.length) : line.trimStart())).join("\n"), indent };
}

function findYamlKey(doc: Document, name: string): string | undefined {
  if (!isMap(doc.contents)) return undefined;
  for (const pair of doc.contents.items) {
    const key = isScalar(pair.key) ? pair.key.value : pair.key;
    if (typeof key === "string" && kebabCase(key) === name) return key;
  }
  return undefined;
}

function normalizePath(key: MdcPropKey): MdcPropPath {
  const path: MdcPropPath = typeof key === "string" ? parsePathString(key) : [...key];
  if (typeof path[0] === "string") path[0] = kebabCase(path[0]);
  return path;
}

function parsePathString(key: string): MdcPropPath {
  if (!/[.[]/.test(key)) return [key];
  const path: MdcPropPath = [];
  for (const match of key.matchAll(/([^.[\]]+)|\[(\d+)\]/g)) {
    path.push(match[2] !== undefined ? Number(match[2]) : match[1]!);
  }
  return path;
}

function setAtPath(target: unknown, path: MdcPropPath, value: unknown): unknown {
  const [head, ...rest] = path;
  const container = (target ?? (typeof head === "number" ? [] : {})) as Record<string | number, unknown>;
  if (rest.length === 0) {
    if (value === undefined) {
      if (Array.isArray(container) && typeof head === "number") container.splice(head, 1);
      else delete container[head!];
    } else {
      container[head!] = value;
    }
    return container;
  }
  container[head!] = setAtPath(container[head!], rest, value);
  return container;
}

/** True when every collection above the target exists, so a nested edit can stay local. */
function canEditInPlace(doc: Document, path: MdcPropPath) {
  for (let depth = 1; depth < path.length; depth += 1) {
    const node = doc.getIn(path.slice(0, depth), true);
    const next = path[depth];
    if (typeof next === "number" ? !isSeq(node) || next > node.items.length : !isMap(node)) return false;
  }
  return true;
}

/**
 * Sets `value` at `path`, recursing into existing collections so unchanged entries keep their
 * nodes (and so their formatting and comments). Undefined deletes.
 */
function applyValue(doc: Document, path: MdcPropPath, value: unknown) {
  if (value === undefined) {
    doc.deleteIn(path);
    return;
  }

  const node = doc.getIn(path, true);

  if (Array.isArray(value) && isSeq(node)) {
    value.forEach((item, index) => applyValue(doc, [...path, index], item));
    while (node.items.length > value.length) node.items.pop();
    return;
  }

  if (isPlainObject(value) && isMap(node)) {
    for (const pair of [...node.items]) {
      const key = isScalar(pair.key) ? pair.key.value : pair.key;
      if (!(String(key) in value)) node.delete(pair.key);
    }
    for (const [key, item] of Object.entries(value)) applyValue(doc, [...path, key], item);
    return;
  }

  if (isScalar(node) && deepEqual(node.value, value)) return;

  if (typeof value === "string" && /[\t\u0000-\u0008\u000b-\u001f]/.test(value)) {
    // Double quotes escape tabs and control characters, so none land in the source raw.
    const scalar = new Scalar(value);
    scalar.type = Scalar.QUOTE_DOUBLE;
    doc.setIn(path, scalar);
    return;
  }

  // Scalars are passed raw so an existing scalar node keeps its quoting style, unless the old
  // value could not be written plain: then the quotes were forced (an empty string, or a
  // trailing space mid-typing), not chosen, and the new value is styled afresh.
  const keepStyle = isScalar(node) && !(typeof node.value === "string" && needsQuotes(node.value));
  doc.setIn(path, keepStyle ? value : doc.createNode(value));
}

function needsQuotes(value: string) {
  return /^["']/.test(new Document(value).toString(yamlOutput));
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((item, index) => deepEqual(item, b[index]));
  if (isPlainObject(a) && isPlainObject(b)) {
    const keys = Object.keys(a);
    return keys.length === Object.keys(b).length && keys.every((key) => key in b && deepEqual(a[key], b[key]));
  }
  return false;
}

function findLastIndex<T>(items: T[], predicate: (item: T) => boolean) {
  for (let index = items.length - 1; index >= 0; index -= 1) if (predicate(items[index]!)) return index;
  return -1;
}

/** A prop value as YAML text, for a free-form field. */
export function formatMdcValue(value: unknown): string {
  if (value === undefined) return "";
  return new Document(value).toString(yamlOutput).replace(/\n$/, "");
}

/** Parses a free-form field's text (YAML, which includes JSON). Empty text is undefined. */
export function parseMdcValue(text: string): { value: unknown; error?: undefined } | { error: string; value?: undefined } {
  if (text.trim() === "") return { value: undefined };
  const doc = parseDocument(text);
  if (doc.errors.length > 0) return { error: doc.errors[0]!.message };
  return { value: doc.toJS() };
}
