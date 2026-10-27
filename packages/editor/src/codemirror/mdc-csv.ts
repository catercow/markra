// Pasting CSV (or tab-separated text copied from a spreadsheet) as a Markdown table. The host
// converts the text (CowMark: the shared Rust code in `cowmark-core`), which also gives the
// table a table component's columns and canonical values when the caret is in that component.

import { EditorSelection, EditorState, Prec, type Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { MdcCatalogComponent } from "../mdc/components.ts";
import { mdcBlocksForState, type MdcBlock } from "./mdc-blocks.ts";
import { mdcComponentInsertion } from "./mdc-insert.ts";
import { defineMarkraPlugin } from "./plugin.ts";

export interface MdcCsvInput {
  text: string;
  /** The clipboard said `text/csv`, or the text came from a `.csv` file. */
  csvType: boolean;
  /** The table component whose slot the table goes in; the host formats it for that component. */
  component: string | null;
}

/** The table as Markdown, or null when the text isn't a table. */
export type MdcCsvConvert = (input: MdcCsvInput) => Promise<string | null>;

export interface MdcCsvPluginOptions {
  convert: MdcCsvConvert;
  /** The workspace's components, read on each paste. */
  components: () => readonly MdcCatalogComponent[] | null | undefined;
}

/**
 * A cheap first look before asking the host: two or more lines, all tab-separated, or all with
 * a comma (or anything at all from a `text/csv` clipboard).
 */
export function mightBeCsv(text: string, csvType: boolean) {
  const lines = text.split(/\r?\n/u).filter((line) => line.trim() !== "");
  if (lines.length === 0) return false;
  if (csvType) return true;
  if (lines.length < 2) return false;
  return lines.every((line) => line.includes("\t")) || lines.every((line) => line.includes(","));
}

function tableComponents(components: readonly MdcCatalogComponent[] | null | undefined) {
  return new Set((components ?? []).filter((component) => component.input?.kind === "table").map((component) => component.name));
}

/** The innermost table component whose slot (between its opener or props and its closer) holds `pos`. */
export function mdcTableSlotAt(state: EditorState, pos: number, components: readonly MdcCatalogComponent[] | null | undefined) {
  const tables = tableComponents(components);
  let found: MdcBlock | null = null;
  for (const block of mdcBlocksForState(state)) {
    if (!block.close || !tables.has(block.name)) continue;
    if (pos > block.open.to && pos <= block.close.from && (!found || block.depth > found.depth)) found = block;
  }
  return found;
}

/** The slot's table lines, when every body row is empty (a fresh skeleton): replaced by a paste. */
function emptySlotTable(doc: Text, block: MdcBlock) {
  const first = doc.lineAt(block.open.to).number + 1;
  const last = doc.lineAt(block.close!.from).number - 1;
  let from: number | null = null;
  let to = 0;
  const rows: string[] = [];
  for (let number = first; number <= last; number++) {
    const line = doc.line(number);
    const text = line.text.trim();
    if (text === "") {
      if (from !== null) break;
      continue;
    }
    if (!text.includes("|")) return null;
    from ??= line.from + (line.text.length - line.text.trimStart().length);
    to = line.to;
    rows.push(text);
  }
  if (from === null) return null;
  const cells = (row: string) => row.replace(/^\|/u, "").replace(/\|$/u, "").split("|").map((cell) => cell.trim());
  const body = rows.slice(1).filter((row) => !cells(row).every((cell) => /^:?-+:?$/u.test(cell)));
  return body.every((row) => cells(row).every((cell) => cell === "")) ? { from, to } : null;
}

/**
 * Converts `text` and inserts the table at `from..to`: in place of an empty table skeleton when
 * that's where it goes, else as its own block. Resolves false (and changes nothing) when the
 * host says the text isn't a table, or the document is read-only.
 */
export async function insertMdcCsv(
  view: EditorView,
  { components, convert, csvType, from, text, to }: { text: string; csvType: boolean; from: number; to: number } & MdcCsvPluginOptions,
) {
  if (view.state.facet(EditorState.readOnly)) return false;
  const startDoc = view.state.doc;
  const slot = mdcTableSlotAt(view.state, from, components());
  let table: string | null;
  try {
    table = await convert({ component: slot?.name ?? null, csvType, text });
  } catch {
    table = null;
  }
  if (table === null || view.state.facet(EditorState.readOnly)) return false;

  // Edits made while converting move the target.
  const changes = view.state.doc === startDoc ? null : view.state.changes;
  const mappedFrom = changes ? view.state.selection.main.from : from;
  const mappedTo = changes ? view.state.selection.main.to : to;
  const state = view.state;
  const currentSlot = mdcTableSlotAt(state, mappedFrom, components());
  const skeleton = currentSlot ? emptySlotTable(state.doc, currentSlot) : null;
  if (skeleton) {
    const indent = /^[ \t]*/u.exec(state.doc.lineAt(skeleton.from).text)![0];
    const insert = table.split("\n").join(`\n${indent}`);
    view.dispatch({
      changes: { from: skeleton.from, insert, to: skeleton.to },
      scrollIntoView: true,
      selection: EditorSelection.cursor(skeleton.from + insert.length),
      userEvent: "input.paste",
    });
    return true;
  }
  const insertion = mdcComponentInsertion(state.update({ selection: EditorSelection.range(mappedFrom, mappedTo) }).state, table);
  view.dispatch({
    changes: { from: insertion.from, insert: insertion.insert, to: insertion.to },
    scrollIntoView: true,
    selection: EditorSelection.cursor(insertion.from + insertion.insert.trimEnd().length),
    userEvent: "input.paste",
  });
  return true;
}

/**
 * Turns pasted CSV or spreadsheet cells into a Markdown table. Other pastes are left to the
 * editor's usual handling; text that looked like CSV but isn't is pasted as plain text.
 */
export function mdcCsvPastePlugin(options: MdcCsvPluginOptions) {
  return defineMarkraPlugin({
    id: "markra.mdc-csv-paste",
    extension: Prec.high(
      EditorView.domEventHandlers({
        paste(event, view) {
          const data = event.clipboardData;
          if (!data || view.state.facet(EditorState.readOnly)) return false;
          // The clipboard can't be read once this handler returns.
          const csv = data.getData("text/csv");
          const plain = data.getData("text/plain");
          const csvType = csv !== "";
          const text = csvType ? csv : plain;
          if (!text || !mightBeCsv(text, csvType)) return false;

          event.preventDefault();
          const { from, to } = view.state.selection.main;
          void insertMdcCsv(view, { ...options, csvType, from, text, to }).then((inserted) => {
            if (inserted || view.state.facet(EditorState.readOnly)) return;
            view.dispatch(view.state.replaceSelection(plain || text), {
              scrollIntoView: true,
              userEvent: "input.paste",
            });
          });
          return true;
        },
      }),
    ),
  });
}
