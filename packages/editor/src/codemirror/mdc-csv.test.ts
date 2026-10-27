import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import type { MdcCatalogComponent } from "../mdc/components.ts";
import { liveMarkdown } from "./index.ts";
import { insertMdcCsv, mdcCsvPastePlugin, mdcTableSlotAt, mightBeCsv, type MdcCsvInput } from "./mdc-csv.ts";
import "./dom.test-support.ts";

const components: MdcCatalogComponent[] = [
  { input: { columns: [{ name: "Item" }, { name: "Price" }], kind: "table" }, name: "pricing-grid", props: [] },
  { name: "callout", props: [] },
];

const views: EditorView[] = [];

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.replaceChildren();
});

function createView(doc: string, cursor = doc.length, extensions = [liveMarkdown({ mdc: true })]) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({ doc, extensions, selection: EditorSelection.cursor(cursor) }),
  });
  views.push(view);
  return view;
}

const table = "| Item  | Price |\n| ----- | ----- |\n| Tacos | $7.00 |";

function converter(result: string | null = table) {
  const calls: MdcCsvInput[] = [];
  const convert = (input: MdcCsvInput) => {
    calls.push(input);
    return Promise.resolve(result);
  };
  return { calls, convert };
}

describe("mightBeCsv", () => {
  it("looks for consistent separators over two or more lines", () => {
    expect(mightBeCsv("a\tb\nc\td", false)).toBe(true);
    expect(mightBeCsv("a,b\nc,d\n", false)).toBe(true);
    expect(mightBeCsv("a,b", false)).toBe(false);
    expect(mightBeCsv("a,b\nno separator", false)).toBe(false);
    expect(mightBeCsv("anything", true)).toBe(true);
    expect(mightBeCsv("  \n", true)).toBe(false);
  });
});

describe("insertMdcCsv", () => {
  it("replaces an empty skeleton table in a table component", async () => {
    const doc = "::pricing-grid\n| Item | Price |\n| ---- | ----- |\n|      |       |\n::\n";
    const view = createView(doc, doc.indexOf("|      |") + 2);
    const { calls, convert } = converter();
    const inserted = await insertMdcCsv(view, {
      components: () => components,
      convert,
      csvType: false,
      from: view.state.selection.main.from,
      text: "Item,Price\nTacos,7",
      to: view.state.selection.main.to,
    });
    expect(inserted).toBe(true);
    expect(calls).toEqual([{ component: "pricing-grid", csvType: false, text: "Item,Price\nTacos,7" }]);
    expect(view.state.doc.toString()).toBe(`::pricing-grid\n${table}\n::\n`);
  });

  it("adds the table as its own block when the component already has rows", async () => {
    const doc = "::pricing-grid\n| Item | Price |\n| --- | --- |\n| Pie | $4 |\n::\n";
    const view = createView(doc, doc.indexOf("| Pie |") + 2);
    expect(mdcTableSlotAt(view.state, view.state.selection.main.from, components)?.name).toBe("pricing-grid");
    const { convert } = converter();
    const from = view.state.doc.line(4).to;
    await insertMdcCsv(view, { components: () => components, convert, csvType: false, from, text: "a,b\nc,d", to: from });
    expect(view.state.doc.toString()).toContain("| Pie | $4 |");
    expect(view.state.doc.toString()).toContain(table);
  });

  it("passes no component outside a table component, and changes nothing when it isn't a table", async () => {
    const view = createView("::callout\nText\n::\n", 12);
    expect(mdcTableSlotAt(view.state, 12, components)).toBeNull();
    const { calls, convert } = converter(null);
    const inserted = await insertMdcCsv(view, { components: () => components, convert, csvType: true, from: 12, text: "x", to: 12 });
    expect(inserted).toBe(false);
    expect(calls[0]!.component).toBeNull();
    expect(view.state.doc.toString()).toBe("::callout\nText\n::\n");
  });

  it("leaves a read-only document alone", async () => {
    const view = createView("Hi", 2, [liveMarkdown({ mdc: true }), EditorState.readOnly.of(true)]);
    const { calls, convert } = converter();
    expect(await insertMdcCsv(view, { components: () => components, convert, csvType: true, from: 2, text: "a,b", to: 2 })).toBe(false);
    expect(calls).toEqual([]);
  });
});

describe("mdcCsvPastePlugin", () => {
  function paste(view: EditorView, data: Record<string, string>) {
    const event = new Event("paste", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: { getData: (type: string) => data[type] ?? "" } });
    view.contentDOM.dispatchEvent(event);
    return event;
  }

  it("converts spreadsheet cells and pastes other text as before", async () => {
    const { calls, convert } = converter();
    const view = createView("", 0, [liveMarkdown({ mdc: true, plugins: [mdcCsvPastePlugin({ components: () => components, convert })] })]);

    paste(view, { "text/plain": "just words" });
    expect(calls).toEqual([]);
    view.dispatch({ changes: { from: 0, insert: "", to: view.state.doc.length } });

    paste(view, { "text/plain": "Item\tPrice\nTacos\t7" });
    await Promise.resolve();
    await Promise.resolve();
    expect(calls).toEqual([{ component: null, csvType: false, text: "Item\tPrice\nTacos\t7" }]);
    expect(view.state.doc.toString()).toContain(table);
  });

  it("falls back to plain text when the host says it isn't a table", async () => {
    const { convert } = converter(null);
    const view = createView("", 0, [liveMarkdown({ mdc: true, plugins: [mdcCsvPastePlugin({ components: () => components, convert })] })]);
    paste(view, { "text/plain": "a,b\nc,d" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(view.state.doc.toString()).toBe("a,b\nc,d");
  });
});
