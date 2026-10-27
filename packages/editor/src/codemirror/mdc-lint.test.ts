import { markdown } from "@codemirror/lang-markdown";
import { diagnosticCount, forceLinting, forEachDiagnostic, setDiagnostics, type Diagnostic } from "@codemirror/lint";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { GFM } from "@lezer/markdown";
import { afterEach, describe, expect, it } from "vitest";
import { catalog, component } from "../mdc/components.fixtures.ts";
import { liveMarkdown } from "./index.ts";
import {
  externalLintDiagnostics,
  lintMdcDocument,
  lintMdcView,
  mdcLintPlugin,
  type ExternalLintDiagnostic,
} from "./mdc-lint.ts";
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

describe("external lint", () => {
  function createView(doc: string, plugins = [mdcPreviewPlugin()]) {
    const parent = document.createElement("div");
    document.body.append(parent);
    const view = new EditorView({
      parent,
      state: EditorState.create({ doc, extensions: [liveMarkdown({ mdc: true, plugins })] }),
    });
    views.push(view);
    return view;
  }

  function found(fromUtf16: number, toUtf16: number, rest: Partial<ExternalLintDiagnostic> = {}): ExternalLintDiagnostic {
    return { fromUtf16, message: "Problem with `x`.", rule: "missing-image", severity: "warning", toUtf16, ...rest };
  }

  function deferred() {
    let resolve!: (value: ExternalLintDiagnostic[]) => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<ExternalLintDiagnostic[]>((done, fail) => {
      resolve = done;
      reject = fail;
    });
    return { promise, reject, resolve };
  }

  it("maps UTF-16 offsets and keeps severity, message and rule", () => {
    const doc = "😀 ![](gone.jpg)\n";
    const state = EditorState.create({ doc });
    const [diagnostic] = externalLintDiagnostics(state.doc, [found(3, 16, { rule: "missing-image", severity: "error" })]);

    expect(doc.slice(diagnostic!.from, diagnostic!.to)).toBe("![](gone.jpg)");
    expect(diagnostic).toMatchObject({ message: "Problem with `x`.", severity: "error", source: "missing-image" });
  });

  it("offers the fix as a quick-fix action that is one change", () => {
    const view = createView("Price: 7.5 today");
    const [diagnostic] = externalLintDiagnostics(
      view.state.doc,
      [found(7, 10, { fix: { fromUtf16: 7, insert: "$7.50", toUtf16: 10 } })],
      "Repair",
    );
    expect(diagnostic!.actions?.map((action) => action.name)).toEqual(["Repair"]);
    diagnostic!.actions![0]!.apply(view, diagnostic!.from, diagnostic!.to);
    expect(view.state.doc.toString()).toBe("Price: $7.50 today");
  });

  it("follows edits before the diagnostic and skips a fix whose text changed", () => {
    const view = createView("Price: 7.5");
    const [diagnostic] = externalLintDiagnostics(view.state.doc, [
      found(7, 10, { fix: { fromUtf16: 7, insert: "$7.50", toUtf16: 10 } }),
    ]);
    view.dispatch({ changes: { from: 0, insert: "New " } });
    diagnostic!.actions![0]!.apply(view, diagnostic!.from + 4, diagnostic!.to + 4);
    expect(view.state.doc.toString()).toBe("New Price: $7.50");
    diagnostic!.actions![0]!.apply(view, 11, 13);
    expect(view.state.doc.toString()).toBe("New Price: $7.50");
  });

  it("has no action without a usable fix", () => {
    const state = EditorState.create({ doc: "abc" });
    const [plain, outside] = externalLintDiagnostics(state.doc, [
      found(0, 1),
      found(0, 1, { fix: { fromUtf16: 0, insert: "x", toUtf16: 9 } }),
    ]);
    expect(plain!.actions).toBeUndefined();
    expect(outside!.actions).toBeUndefined();
  });

  it("clamps offsets to the document", () => {
    const state = EditorState.create({ doc: "short" });
    const ranges = externalLintDiagnostics(state.doc, [found(3, 99), found(40, 50), found(-2, 1), found(4, 2)])
      .map(({ from, to }) => [from, to]);
    expect(ranges).toEqual([[3, 5], [5, 5], [0, 1], [4, 4]]);
  });

  it("shows text between backticks as code", () => {
    const view = createView("text");
    const [diagnostic] = externalLintDiagnostics(view.state.doc, [found(0, 1)]);
    const element = diagnostic!.renderMessage!(view) as HTMLElement;
    expect(element.querySelector("code")?.textContent).toBe("x");
  });

  it("uses the external result when it applies", async () => {
    const view = createView("::promo-code\n::\n");
    const texts: string[] = [];
    const diagnostics = await lintMdcView(view, {
      components: () => catalog,
      external: (text) => {
        texts.push(text);
        return Promise.resolve([found(2, 12, { rule: "unknown-component", severity: "error" })]);
      },
    });
    expect(texts).toEqual(["::promo-code\n::\n"]);
    expect(diagnostics.map((diagnostic) => diagnostic.source)).toEqual(["unknown-component"]);
  });

  it("falls back to the built-in lint when the external one doesn't apply or fails", async () => {
    const view = createView("::promo-code\n::\n");
    const builtIn = await lintMdcView(view, { components: () => catalog, external: () => null });
    expect(builtIn.map((diagnostic) => diagnostic.source)).toEqual(["MDC"]);

    const failed = await lintMdcView(view, {
      components: () => catalog,
      external: () => Promise.reject(new Error("offline")),
    });
    expect(failed.map((diagnostic) => diagnostic.source)).toEqual(["MDC"]);
  });

  it("drops results for text that changed while waiting", async () => {
    const view = createView("Intro\n");
    const pending = deferred();
    const result = lintMdcView(view, { external: () => pending.promise });
    view.dispatch({ changes: { from: 0, insert: "New " } });
    pending.resolve([found(0, 5)]);
    expect(await result).toEqual([]);
  });

  it("keeps the shown diagnostics when a stale result arrives", async () => {
    const calls: Array<ReturnType<typeof deferred>> = [];
    const view = createView("Intro\n", [
      mdcLintPlugin({
        external: () => {
          const call = deferred();
          calls.push(call);
          return call.promise;
        },
      }),
    ]);
    const shown = () => {
      const all: Diagnostic[] = [];
      forEachDiagnostic(view.state, (diagnostic) => all.push(diagnostic));
      return all.map((diagnostic) => [diagnostic.from, diagnostic.to, diagnostic.source]);
    };

    forceLinting(view);
    await new Promise((resolve) => setTimeout(resolve, 0));
    calls[0]!.resolve([found(0, 5, { rule: "image-alt" })]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(shown()).toEqual([[0, 5, "image-alt"]]);

    forceLinting(view);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toHaveLength(1);

    view.dispatch({ changes: { from: 0, insert: "New " } });
    forceLinting(view);
    await new Promise((resolve) => setTimeout(resolve, 0));
    view.dispatch({ changes: { from: 0, insert: "Really " } });
    calls[1]!.resolve([found(1, 2, { rule: "broken-link" })]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(shown().map(([, , source]) => source)).toEqual(["image-alt"]);
  });
});
