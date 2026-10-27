import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import { catalog, component } from "../mdc/components.fixtures.ts";
import {
  groupMdcComponents,
  MDC_GALLERY_SKELETON,
  mdcColonShift,
  mdcComponentSnippet,
  mdcTableSkeleton,
  type MdcCatalogComponent,
} from "../mdc/components.ts";
import {
  blocksPlugin,
  getMarkraSlashMenuState,
  liveMarkdown,
  openMarkraSlashMenu,
  runMarkraSlashMenuAction,
} from "./index.ts";
import { mdcBlocksForState } from "./mdc-blocks.ts";
import { openMdcBlock } from "./mdc-edit.ts";
import {
  insertMdcComponent,
  mdcComponentCommandId,
  mdcComponentInsertion,
  mdcComponentsPlugin,
  nestMdcSnippet,
} from "./mdc-insert.ts";
import { lintMdcDocument } from "./mdc-lint.ts";
import { mdcPreviewPlugin } from "./mdc-preview.ts";
import "./dom.test-support.ts";

const views: EditorView[] = [];

function createView(doc: string, cursor = doc.length, components = catalog) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc,
      extensions: [
        liveMarkdown({
          mdc: true,
          plugins: [blocksPlugin(), mdcPreviewPlugin(), mdcComponentsPlugin({ components })],
          slashMenu: true,
        }),
      ],
      selection: EditorSelection.cursor(cursor),
    }),
  });
  views.push(view);
  return view;
}

function pick(view: EditorView, name: string) {
  return runMarkraSlashMenuAction(view, mdcComponentCommandId(name));
}

function caretLine(view: EditorView) {
  return view.state.doc.lineAt(view.state.selection.main.head).text;
}

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.replaceChildren();
});

describe("groupMdcComponents", () => {
  it("keeps the schema's group order and sorts each group by name", () => {
    expect(groupMdcComponents(catalog).map(({ components, group }) => [group, components.map((c) => c.name)])).toEqual([
      ["Calls to action", ["address-cta", "cta-banner"]],
      ["Text and lists", ["bullet-list", "callout", "faqs", "quote", "steps"]],
      ["Layout", ["card", "grid", "hero", "instagram-post", "page-section"]],
      ["Calculators", ["chipotle-cost-calculator"]],
      ["Pricing and numbers", ["pricing-grid", "promo-codes"]],
    ]);
  });

  it("puts components without a group last", () => {
    const groups = groupMdcComponents([{ name: "z", props: [] }, component("hero")]);
    expect(groups.map((group) => group.group)).toEqual(["Layout", ""]);
  });
});

describe("components in the slash menu", () => {
  it("lists them after the built-in blocks, in a section grouped by group", () => {
    const menu = getMarkraSlashMenuState(createView("/"));
    const sectioned = menu.actions.filter((action) => action.section);

    expect(menu.actions[0]!.section).toBeUndefined();
    expect(menu.actions.slice(menu.actions.length - sectioned.length).every((action) => action.section === "Components")).toBe(true);
    expect([...new Set(sectioned.map((action) => action.group))]).toEqual([
      "Calls to action",
      "Text and lists",
      "Layout",
      "Calculators",
      "Pricing and numbers",
    ]);
    const quote = sectioned.find((action) => action.label === "quote");
    expect(quote?.description).toBe("Pull quote or testimonial with the speaker's name, title and company.");
  });

  it("leads with components when the menu is opened from a button", () => {
    const view = createView("Intro\n\n");
    openMarkraSlashMenu(view);
    const menu = getMarkraSlashMenuState(view);

    expect(menu.actions[0]!.label).toBe("address-cta");
    expect(menu.actions.at(-1)!.section).toBeUndefined();
  });

  it("searches by name, component id and purpose", () => {
    const labels = (query: string) =>
      getMarkraSlashMenuState(createView(`/${query}`)).actions.filter((action) => action.section).map((action) => action.label);

    expect(labels("promo")).toEqual(["promo-codes"]);
    expect(labels("PromoCodes")).toEqual(["promo-codes"]);
    expect(labels("testimonial")).toEqual(["quote"]);
    expect(labels("calculator")).toEqual(["chipotle-cost-calculator"]);
    expect(labels("components")).toHaveLength(catalog.length);
  });
});

describe("inserting a component", () => {
  it("inserts the schema example as its own block and opens its props form", () => {
    const view = createView("Intro\n\n/call");
    expect(pick(view, "callout")).toBe(true);

    expect(view.state.doc.toString()).toBe(`Intro\n\n${component("callout").example!.markdown}`);
    expect(openMdcBlock(view.state)?.name).toBe("callout");
    expect(caretLine(view)).toMatch(/^CaterCow delivers Italian catering/u);
  });

  it("places the caret after a block that has no body", () => {
    const view = createView("/");
    pick(view, "chipotle-cost-calculator");

    expect(view.state.doc.toString()).toBe("::chipotle-cost-calculator\n::");
    expect(view.state.selection.main.head).toBe(view.state.doc.length);
    expect(openMdcBlock(view.state)?.name).toBe("chipotle-cost-calculator");
  });

  it("builds a bare block when the schema has no example", () => {
    expect(mdcComponentSnippet(component("steps"))).toBe("::steps\n\n::");
    const view = createView("Intro\n\n/");
    pick(view, "steps");
    expect(view.state.doc.toString()).toBe("Intro\n\n::steps\n\n::");
    expect(view.state.doc.lineAt(view.state.selection.main.head).number).toBe(4);
  });

  const pricingGrid: MdcCatalogComponent = {
    example: { markdown: "::pricing-grid\n| Item | Price |\n| - | - |\n| Tacos | $7 |\n::" },
    input: {
      columns: [
        { name: "Item", required: true },
        { format: "price", name: "Price", required: true },
        { name: "Description" },
      ],
      kind: "table",
    },
    name: "pricing-grid",
    props: [],
  };

  it("gives a table component an empty table to fill in, caret in its first cell", () => {
    expect(mdcComponentSnippet(pricingGrid)).toBe(
      "::pricing-grid\n| Item | Price | Description |\n| --- | --- | --- |\n|  |  |  |\n::",
    );
    const view = createView("/", 1, [pricingGrid]);
    pick(view, "pricing-grid");
    const head = view.state.selection.main.head;
    expect(view.state.doc.lineAt(head).number).toBe(4);
    expect(head - view.state.doc.lineAt(head).from).toBe(2);
  });

  it("gives a gallery placeholder images", () => {
    expect(mdcComponentSnippet({ input: { kind: "images" }, name: "photo-grid", props: [] })).toBe(
      `::photo-grid\n${MDC_GALLERY_SKELETON}\n::`,
    );
    expect(mdcTableSkeleton([{ name: "A" }])).toBe("| A |\n| --- |\n|  |");
  });

  it("splits a paragraph when chosen on one of its lines", () => {
    const view = createView("line one\n/\nline three", 10);
    pick(view, "card");

    expect(view.state.doc.toString()).toBe(
      `line one\n\n${component("card").example!.markdown}\n\nline three`,
    );
    expect(mdcBlocksForState(view.state).map((block) => block.name)).toEqual(["card"]);
  });

  it("splits a line at the caret, keeping the text on both sides", () => {
    const state = EditorState.create({ doc: "Hello world", selection: EditorSelection.cursor(6) });
    const insertion = mdcComponentInsertion(state, "::hero\n::");
    const next = state.update({ changes: insertion }).state.doc.toString();

    expect(next).toBe("Hello\n\n::hero\n::\n\nworld");
  });

  it("replaces a selection", () => {
    const state = EditorState.create({ doc: "Keep this. Drop this. Keep that.", selection: EditorSelection.range(11, 21) });
    const next = state.update({ changes: mdcComponentInsertion(state, "::hero\n::") }).state.doc.toString();

    expect(next).toBe("Keep this.\n\n::hero\n::\n\nKeep that.");
  });

  it("keeps a card's colons inside a grid that already uses more", () => {
    const doc = ":::grid{columns=2}\n/\n:::";
    const view = createView(doc, doc.indexOf("/") + 1);
    pick(view, "card");

    const blocks = mdcBlocksForState(view.state);
    expect(view.state.doc.toString()).toMatch(/^:::grid\{columns=2\}\n::card\{title="Use the 2-Hour Lead Time/u);
    expect(blocks.map((block) => [block.name, block.depth, Boolean(block.close)])).toEqual([
      ["grid", 0, true],
      ["card", 1, true],
    ]);
  });

  it("adds a colon so a component nests inside one with the same count", () => {
    const doc = "::page-section\nLead\n\n/\n::";
    const view = createView(doc, doc.indexOf("/") + 1);
    pick(view, "card");

    const text = view.state.doc.toString();
    expect(text).toMatch(/\n:::card\{title=/u);
    expect(text).toMatch(/\n:::\n::$/u);
    expect(mdcBlocksForState(view.state).map((block) => [block.name, block.depth])).toEqual([
      ["page-section", 0],
      ["card", 1],
    ]);
  });

  it("shifts every level of a nested example", () => {
    const grid = component("grid").example!.markdown;
    expect(nestMdcSnippet(grid, [2])).toBe(grid.replace(/^:::/gmu, "::::").replace(/^::(?!:)/gmu, ":::"));
    expect(nestMdcSnippet(grid, [4])).toBe(grid);
    expect(mdcColonShift([3, 2], [2, 3])).toBe(2);
  });

  it("indents a nested component like the line it replaces", () => {
    const doc = "::page-section\n  /\n::";
    const view = createView(doc, doc.indexOf("/") + 1);
    pick(view, "chipotle-cost-calculator");

    expect(view.state.doc.toString()).toBe("::page-section\n  :::chipotle-cost-calculator\n  :::\n::");
  });

  it("goes before a component when the caret is on its opener", () => {
    const view = createView("Intro\n::hero\nBody\n::", 9);
    insertMdcComponent(view, component("chipotle-cost-calculator"));

    expect(view.state.doc.toString()).toBe("Intro\n\n::chipotle-cost-calculator\n::\n\n::hero\nBody\n::");
  });

  it("refuses in a read-only editor", () => {
    const view = new EditorView({
      parent: document.body,
      state: EditorState.create({ doc: "Intro", extensions: [EditorState.readOnly.of(true)] }),
    });
    views.push(view);
    expect(insertMdcComponent(view, component("hero"))).toBe(false);
    expect(view.state.doc.toString()).toBe("Intro");
  });

  it("inserts every schema component as a block the linter accepts", () => {
    for (const entry of catalog) {
      const view = createView("Intro\n\n/");
      pick(view, entry.name);
      const blocks = mdcBlocksForState(view.state);

      expect(blocks[0]?.name, entry.name).toBe(entry.name);
      expect(lintMdcDocument(view.state, catalog), entry.name).toEqual([]);
    }
  });
});
