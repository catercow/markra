// Reads and writes top-level keys of YAML front matter. A write replaces only the text of the key
// it changes; every other byte (other keys, their quoting, comments, blank lines) stays as it was.

import { Document, isMap, isScalar, parseDocument, Scalar, type Node, type Pair, type ToStringOptions } from "yaml";

export interface FrontmatterRead {
  values: Record<string, unknown>;
  /** Top-level keys in source order. */
  keys: string[];
  error?: string;
}

export interface FrontmatterWrite {
  text: string;
  /** Set when the write was refused; `text` is then the input unchanged. */
  error?: string;
}

const yamlOutput: ToStringOptions = {
  doubleQuotedAsJSON: true,
  lineWidth: 0,
  // Front matter in the content repository quotes with single quotes.
  singleQuote: true
};

// Nuxt Content parses front matter with YAML 1.1 rules, where an unquoted ISO date becomes a Date.
const timestampLike = /^\d{4}-\d{2}-\d{2}(?:[Tt ]|$)/u;

function load(text: string) {
  const doc = parseDocument(text);
  if (doc.errors.length > 0) return { error: `Invalid YAML: ${doc.errors[0]!.message}` } as const;
  if (doc.contents !== null && (!isMap(doc.contents) || doc.contents.flow)) {
    return { error: "The front matter must be a list of `key: value` lines." } as const;
  }
  return { doc } as const;
}

function pairsOf(doc: Document) {
  return isMap(doc.contents) ? (doc.contents.items as Pair<Node, Node | null>[]) : [];
}

function keyName(pair: Pair<Node, Node | null>) {
  return isScalar(pair.key) ? String(pair.key.value) : String(pair.key);
}

export function readFrontmatterFields(text: string): FrontmatterRead {
  const loaded = load(text);
  if ("error" in loaded) return { error: loaded.error, keys: [], values: {} };
  const values = (loaded.doc.toJS() ?? {}) as Record<string, unknown>;
  return { keys: pairsOf(loaded.doc).map(keyName), values };
}

function lineStart(text: string, position: number) {
  return text.lastIndexOf("\n", position - 1) + 1;
}

function lineEnd(text: string, position: number) {
  const end = text.indexOf("\n", position);
  return end === -1 ? text.length : end + 1;
}

function isScalarValue(value: unknown) {
  return value === null || ["string", "number", "boolean"].includes(typeof value);
}

/** `key: value` as YAML, without a trailing newline. A replaced scalar keeps its quoting style. */
function renderPair(key: string, value: unknown, previous: Node | null | undefined) {
  const doc = new Document({});
  let node: unknown = doc.createNode(value);
  if (isScalarValue(value)) {
    const scalar = new Scalar(value);
    if (isScalar(previous) && typeof previous.value === typeof value && previous.type) {
      scalar.type = previous.type;
    } else if (typeof value === "string" && timestampLike.test(value)) {
      scalar.type = Scalar.QUOTE_SINGLE;
    }
    node = scalar;
  }
  doc.set(key, node);
  return doc.toString(yamlOutput).replace(/\n$/u, "");
}

/** Where a new key goes: before the first later key in alphabetical order, keeping a leading `title` first. */
function insertionPoint(text: string, pairs: Pair<Node, Node | null>[], key: string) {
  for (const [index, pair] of pairs.entries()) {
    const name = keyName(pair);
    if (index === 0 && name === "title") continue;
    if (name.localeCompare(key) > 0 && pair.key.range) return lineStart(text, pair.key.range[0]);
  }
  return text.length;
}

/** Sets (or, with `undefined`, removes) one top-level key. */
export function writeFrontmatterField(text: string, key: string, value: unknown): FrontmatterWrite {
  const loaded = load(text);
  if ("error" in loaded) return { error: loaded.error, text };
  const pairs = pairsOf(loaded.doc);
  const pair = pairs.find((candidate) => keyName(candidate) === key);

  if (!pair) {
    if (value === undefined) return { text };
    const at = insertionPoint(text, pairs, key);
    const before = text.slice(0, at);
    const separator = before === "" || before.endsWith("\n") ? "" : "\n";
    const after = text.slice(at);
    return { text: `${before}${separator}${renderPair(key, value, null)}\n${after}` };
  }

  const start = pair.key.range![0];
  let end = (pair.value?.range ?? pair.key.range!)[1];
  // Block collections end after their last newline; the line break belongs to the text around them.
  while (end > start && text[end - 1] === "\n") end -= 1;

  if (value === undefined) {
    const from = lineStart(text, start);
    return { text: text.slice(0, from) + text.slice(lineEnd(text, end)) };
  }

  const current = text.slice(start, end);
  const keyText = current.slice(0, pair.key.range![1] - start);
  // Keep the key exactly as written (quoted keys included), replace the rest.
  const rendered = renderPair(key, value, pair.value);
  const renderedKey = renderPair(key, null, null).replace(/:.*$/su, "");
  const next = keyText + rendered.slice(renderedKey.length);
  return { text: text.slice(0, start) + next + text.slice(end) };
}

/** A value as YAML text, for a free-form field. */
export function formatFrontmatterValue(value: unknown): string {
  if (value === undefined) return "";
  return new Document(value).toString(yamlOutput).replace(/\n$/u, "");
}
