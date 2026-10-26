import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import { EditorSelection, EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { GFM } from "@lezer/markdown";
import {
  groupMdcComponents,
  mdcColonShift,
  mdcComponentSnippet,
  type MdcCatalogComponent,
} from "../mdc/components.ts";
import { readMdcBlocksFromTree, type MdcBlock } from "./mdc-blocks.ts";
import { mdcPropsEditorField, openMdcPropsEditor } from "./mdc-edit.ts";
import { markraMdc } from "./mdc-syntax.ts";
import { defineMarkraPlugin } from "./plugin.ts";

const snippetLanguage = markdown({ extensions: [GFM, markraMdc] });

function parseBlocks(state: EditorState, upto = state.doc.length): MdcBlock[] {
  const tree = ensureSyntaxTree(state, upto, 1000) ?? syntaxTree(state);
  return readMdcBlocksFromTree(tree, state.doc);
}

/** Adds colons to every opener and closer of `snippet` so none matches `enclosingColons`. */
export function nestMdcSnippet(snippet: string, enclosingColons: readonly number[]) {
  if (enclosingColons.length === 0) return snippet;
  const state = EditorState.create({ doc: snippet, extensions: snippetLanguage });
  const blocks = parseBlocks(state);
  const shift = mdcColonShift(blocks.map((block) => block.colons), enclosingColons);
  if (shift === 0) return snippet;

  const marks = blocks.flatMap((block) => [block.open.from, ...(block.close ? [block.close.from] : [])]);
  const colons = ":".repeat(shift);
  return state.update({ changes: marks.map((from) => ({ from, insert: colons })) }).state.doc.toString();
}

export interface MdcInsertion {
  from: number;
  to: number;
  insert: string;
  /** Offset in the new document where the inserted snippet starts. */
  snippetFrom: number;
}

function blank(text: string) {
  return text.trim() === "";
}

/**
 * Where a component goes for a caret on an opener or closer: before the opener's line, or
 * after the closer's line, never inside the syntax itself.
 */
function insertionPoint(state: EditorState, blocks: readonly MdcBlock[], pos: number) {
  for (const block of blocks) {
    const openFrom = state.doc.lineAt(block.open.from).from;
    if (pos >= openFrom && pos <= block.open.to) return openFrom;
    if (block.close) {
      const closeLine = state.doc.lineAt(block.close.from);
      if (pos >= closeLine.from && pos <= closeLine.to) return closeLine.to;
    }
  }
  return pos;
}

/**
 * The edit that puts `snippet` in its own block at the main selection: it replaces the
 * selection, splits the paragraph around it, keeps a blank line on both sides, takes the line's
 * indentation, and adds colons when the caret is inside other components.
 */
export function mdcComponentInsertion(state: EditorState, snippet: string): MdcInsertion {
  const blocks = parseBlocks(state);
  const selection = state.selection.main;
  const point = selection.empty ? insertionPoint(state, blocks, selection.head) : null;
  const from = point ?? selection.from;
  const to = point ?? selection.to;

  const enclosing = blocks.filter((block) => block.open.to < from && (!block.close || from <= block.close.from));
  const nested = nestMdcSnippet(snippet, enclosing.map((block) => block.colons));

  const { doc } = state;
  const startLine = doc.lineAt(from);
  const endLine = doc.lineAt(to);
  // Right after the enclosing opener or right before its closer, no blank line is needed.
  const separatedBefore = (number: number) =>
    number < 1 || blank(doc.line(number).text) || enclosing.some((block) => doc.lineAt(block.open.to).number === number);
  const separatedAfter = (number: number) =>
    number > doc.lines ||
    blank(doc.line(number).text) ||
    enclosing.some((block) => block.close && doc.lineAt(block.close.from).number === number);
  const before = state.sliceDoc(startLine.from, from);
  const after = state.sliceDoc(to, endLine.to);
  const indent = /^[ \t]*/u.exec(startLine.text)![0];
  const body = nested
    .split("\n")
    .map((line) => (line.trim() ? indent + line : line))
    .join("\n");

  let replaceFrom: number;
  let prefix: string;
  if (blank(before)) {
    replaceFrom = startLine.from;
    prefix = separatedBefore(startLine.number - 1) ? "" : "\n";
  } else {
    replaceFrom = startLine.from + before.trimEnd().length;
    prefix = "\n\n";
  }

  let replaceTo: number;
  let suffix: string;
  if (blank(after)) {
    replaceTo = endLine.to;
    suffix = separatedAfter(endLine.number + 1) ? "" : "\n";
  } else {
    replaceTo = endLine.to - after.trimStart().length;
    suffix = `\n\n${indent}`;
  }

  return {
    from: replaceFrom,
    insert: `${prefix}${body}${suffix}`,
    snippetFrom: replaceFrom + prefix.length,
    to: replaceTo,
  };
}

/** The caret after insertion: the end of the first body line, else just after the block. */
function caretFor(state: EditorState, block: MdcBlock) {
  const { doc } = state;
  const end = block.close?.from ?? block.open.to;
  const firstBody = doc.lineAt(block.open.to).number + 1;
  const lastBody = block.close ? doc.lineAt(block.close.from).number - 1 : firstBody - 1;
  let emptyLine: number | null = null;
  for (let number = firstBody; number <= lastBody; number++) {
    const line = doc.line(number);
    if (line.from >= end) break;
    if (!blank(line.text)) return line.to;
    emptyLine ??= line.to;
  }
  return emptyLine ?? block.close?.to ?? block.open.to;
}

/**
 * Inserts a component's snippet at the selection, puts the caret in its body and opens its
 * props form.
 */
export function insertMdcComponent(view: EditorView, component: MdcCatalogComponent) {
  if (view.state.facet(EditorState.readOnly)) return false;
  const insertion = mdcComponentInsertion(view.state, mdcComponentSnippet(component));

  view.dispatch({
    changes: { from: insertion.from, insert: insertion.insert, to: insertion.to },
    scrollIntoView: true,
    userEvent: "input.mdc-insert",
  });

  const snippetEnd = insertion.snippetFrom + insertion.insert.length;
  const block = parseBlocks(view.state, snippetEnd).find((candidate) => candidate.open.from >= insertion.snippetFrom);
  if (!block) return true;
  view.dispatch({
    effects: openMdcPropsEditor.of(block.open.from),
    selection: EditorSelection.cursor(caretFor(view.state, block)),
  });
  return true;
}

export interface MdcComponentsPluginOptions {
  components: readonly MdcCatalogComponent[];
  /** Heading of the slash menu section that lists the components. */
  sectionLabel?: string;
}

export function mdcComponentCommandId(name: string) {
  return `mdc.insert.${name}`;
}

/** One slash-menu command per schema component, in a section grouped by `group`. */
export function mdcComponentsPlugin({ components, sectionLabel = "Components" }: MdcComponentsPluginOptions) {
  const groups = groupMdcComponents(components);
  const isEditable = (view: EditorView) => !view.state.facet(EditorState.readOnly);

  return defineMarkraPlugin({
    id: "markra.mdc-components",
    extension: mdcPropsEditorField,
    commands: groups.flatMap(({ components: members }) =>
      members.map((component) => ({
        description: component.purpose ?? undefined,
        id: mdcComponentCommandId(component.name),
        isEnabled: isEditable,
        label: component.name,
        run: (view: EditorView) => insertMdcComponent(view, component),
      }))),
    ui: groups.flatMap(({ components: members, group }, index) =>
      members.map((component) => ({
        command: mdcComponentCommandId(component.name),
        group,
        icon: "puzzle",
        keywords: [component.name, component.component ?? ""].filter(Boolean),
        // Groups stay together in schema order; the menu sorts by label within one order.
        order: 10_000 + index,
        placement: "slash-menu" as const,
        section: sectionLabel,
      }))),
  });
}
