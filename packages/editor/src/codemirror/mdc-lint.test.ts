import { markdown } from "@codemirror/lang-markdown";
import { diagnosticCount, forceLinting, setDiagnostics } from "@codemirror/lint";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { GFM } from "@lezer/markdown";
import { afterEach, describe, expect, it } from "vitest";
import { catalog, component } from "../mdc/components.fixtures.ts";
import { liveMarkdown } from "./index.ts";
import { lintMdcDocument, mdcLintPlugin } from "./mdc-lint.ts";
import { mdcPreviewPlugin } from "./mdc-preview.ts";
import { markraMdc } from "./mdc-syntax.ts";
import "./dom.test-support.ts";

function lint(doc: string, components = catalog) {
  const state = EditorState.create({ doc, extensions: markdown({ extensions: [GFM, markraMdc] }) });
  return lintMdcDocument(state, components).map((diagnostic) => ({
    message: diagnostic.message,
    severity: diagnostic.severity,
    text: doc.slice(diagnostic.from, diagnostic.to),
  }));
}

function example(name: string) {
  return component(name).example!.markdown;
}

const views: EditorView[] = [];

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.replaceChildren();
});

describe("lintMdcDocument", () => {
  it("accepts every schema example", () => {
    for (const entry of catalog.filter((candidate) => candidate.example)) {
      expect(lint(entry.example!.markdown), entry.name).toEqual([]);
    }
  });

  it("flags a component the schema doesn't have", () => {
    expect(lint("::promo-code\n::")).toEqual([
      {
        message: "There is no “promo-code” component. Check the spelling, or pick one from the Components list.",
        severity: "error",
        text: "promo-code",
      },
    ]);
  });

  it("flags a missing required prop on the component's name", () => {
    const doc = example("cta-banner").replace('to="https://www.catercow.com/brands/9109-chickfila" ', "");
    expect(lint(doc)).toEqual([{ message: "This component needs a `to`.", severity: "error", text: "cta-banner" }]);
  });

  it("flags a required prop left empty", () => {
    const doc = example("instagram-post").replace("url: https://www.instagram.com/p/DMd8x-TRioJ/", "url:");
    expect(lint(doc)).toEqual([{ message: "This component needs a `url`.", severity: "error", text: "url:" }]);
  });

  it("flags each list item missing a required field", () => {
    const doc = example("promo-codes").replace("    code: 100OFFWAWADC\n", "");
    expect(lint(doc)).toEqual([
      {
        message: "This component needs a `code` for each item.",
        severity: "error",
        text: "label: Washington, DC Area",
      },
    ]);
  });

  it("flags props the component doesn't use, in the braces and in the YAML", () => {
    expect(lint('::callout{titel="Need every meal labeled?"}\nBody\n::')).toEqual([
      {
        message: "This component doesn't use `titel`, so the site ignores it.",
        severity: "warning",
        text: 'titel="Need every meal labeled?"',
      },
    ]);
    const doc = example("address-cta").replace("cta: Search", "button: Search");
    expect(lint(doc)).toEqual([
      { message: "This component doesn't use `button`, so the site ignores it.", severity: "warning", text: "button: Search" },
    ]);
  });

  it("flags item fields the component doesn't use", () => {
    const doc = example("promo-codes").replace("    code: 100OFFWAWANYC\n", "    code: 100OFFWAWANYC\n    expires: soon\n");
    expect(lint(doc)).toEqual([
      { message: "Items here don't use `expires`, so the site ignores it.", severity: "warning", text: "expires: soon" },
    ]);
  });

  it("flags values that don't match the prop's type", () => {
    expect(lint(example("faqs").replace("structured-data: true", 'structured-data: "yes"'))).toEqual([
      { message: "`structured-data` should be true or false.", severity: "warning", text: 'structured-data: "yes"' },
    ]);
    expect(lint(example("bullet-list").replace('variant="warning"', 'variant="danger"'))).toEqual([
      { message: "`variant` should be one of: default, warning.", severity: "warning", text: 'variant="danger"' },
    ]);
    expect(lint(example("pricing-grid").replace("price: 11.50", 'price: "11.50"'))).toEqual([
      { message: "`price` should be a number.", severity: "warning", text: 'price: "11.50"' },
    ]);
    expect(lint(example("promo-codes").replace("code: 100OFFWAWADC", "code: 100"))).toEqual([
      {
        message: "`code` should be text. If it is a number or true/false, put quotes around it.",
        severity: "warning",
        text: "code: 100",
      },
    ]);
    expect(lint("::bullet-list\n---\nitems: Just one line\n---\n::")).toEqual([
      { message: "`items` should be a list.", severity: "warning", text: "items: Just one line" },
    ]);
    expect(lint("::faqs\n---\nitems:\n  - How far ahead?\n---\n::")).toEqual([
      {
        message: "Each item in `items` needs its own fields, like `question: …`.",
        severity: "warning",
        text: "How far ahead?",
      },
    ]);
  });

  it("reads attribute values the way the site does", () => {
    expect(lint(example("grid"))).toEqual([]);
    expect(lint('::card{title="Tip" number}\nBody\n::')).toEqual([]);
    expect(lint('::card{title="Tip" number="yes"}\nBody\n::')).toEqual([
      { message: "`number` should be true or false.", severity: "warning", text: 'number="yes"' },
    ]);
  });

  it("flags unclosed components and props blocks", () => {
    expect(lint(example("callout").replace(/\n::$/u, ""))).toEqual([
      {
        message: "This component isn't closed. Add a line with just `::` where it ends.",
        severity: "error",
        text: "callout",
      },
    ]);
    expect(lint(example("grid").replace(/\n:::$/u, ""))[0]).toMatchObject({
      message: "This component isn't closed. Add a line with just `:::` where it ends.",
      text: "grid",
    });
    expect(lint("::address-cta\n---\nheadline: Hi\n::")).toEqual([
      {
        message: "The props block isn't closed. Add a line with just `---` after the last prop.",
        severity: "error",
        text: "---",
      },
    ]);
  });

  it("points at YAML that can't be read", () => {
    expect(lint("::address-cta\n---\nheadline: Hi\n  cta: Search\n---\n::")).toEqual([
      {
        message: "The props block can't be read. Check the indentation and quotes around here.",
        severity: "error",
        text: "headline: Hi",
      },
    ]);
  });

  it("checks nested and indented components", () => {
    const doc = "::page-section\n  :::card{title=\"A\" colour=\"red\"}\n  Body\n  :::\n::";
    expect(lint(doc)).toEqual([
      { message: "This component doesn't use `colour`, so the site ignores it.", severity: "warning", text: 'colour="red"' },
    ]);
    const yaml = "::page-section\n  :::promo-codes\n  ---\n  items:\n    - label: A\n  ---\n  :::\n::";
    expect(lint(yaml)).toEqual([
      { message: "This component needs a `code` for each item.", severity: "error", text: "label: A" },
    ]);
  });

  it("checks only the structure without a schema", () => {
    expect(lint("::anything{foo=1}\n::\n\n::other\n", [])).toEqual([
      {
        message: "This component isn't closed. Add a line with just `::` where it ends.",
        severity: "error",
        text: "other",
      },
    ]);
  });

  it("uses the host's messages", () => {
    const state = EditorState.create({ doc: "::nope\n::", extensions: markdown({ extensions: [GFM, markraMdc] }) });
    expect(lintMdcDocument(state, catalog, { unknownComponent: "Kein {name}" })[0]!.message).toBe("Kein nope");
  });
});

describe("mdcLintPlugin", () => {
  function createView(doc: string, cursor = doc.length) {
    const parent = document.createElement("div");
    document.body.append(parent);
    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc,
        extensions: [
          liveMarkdown({
            mdc: true,
            plugins: [mdcPreviewPlugin(), mdcLintPlugin({ components: () => catalog })],
          }),
        ],
        selection: EditorSelection.cursor(cursor),
      }),
    });
    views.push(view);
    return view;
  }

  it("reports diagnostics without changing the text", async () => {
    const doc = "Intro\n\n::promo-code\n::\n";
    const view = createView(doc, 0);
    forceLinting(view);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(diagnosticCount(view.state)).toBe(1);
    expect(view.state.doc.toString()).toBe(doc);
  });

  it("marks a component's card when its hidden opener line has a problem", () => {
    const view = createView("Intro\n\n::cta-banner\nBody\n::\n", 0);
    view.dispatch(setDiagnostics(view.state, [
      { from: 10, message: "This component needs a `to`.", severity: "error", to: 20 },
    ]));

    const chip = view.dom.querySelector(".markra-mdc-chip");
    expect(chip?.classList.contains("markra-mdc-chip-problem")).toBe(true);
    expect(chip?.getAttribute("title")).toBe("This component needs a `to`.");
  });

  it("does nothing when disabled", () => {
    expect(mdcLintPlugin({ enabled: false }).extension).toEqual([]);
  });
});
