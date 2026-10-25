import { undo } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import { openMdcPropsEditor } from "@markra/editor/codemirror";
import { parseMdcSchema } from "@markra/shared";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CodeMirrorPaperSurface } from "./CodeMirrorPaperSurface";

// Components from the CaterCow marketing-content schema/components.json.
const schema = parseMdcSchema(JSON.stringify({
  components: [
    {
      name: "faqs",
      group: "Text and lists",
      purpose: "Frequently asked questions with plain-text answers.",
      props: [
        {
          name: "items",
          type: "Faq[]",
          description: "Questions and answers, in display order.",
          default: "[]",
          items: [
            { name: "question", type: "string", required: true, description: "The question, as the reader would ask it." },
            { name: "answer", type: "string", required: true, description: "Plain-text answer." },
          ],
        },
        { name: "title", type: "string", description: "Heading above the questions.", default: "\"Frequently Asked Questions\"" },
        { name: "structured-data", type: "boolean", description: "Also tell search engines these are FAQs.", default: "false" },
      ],
    },
    {
      name: "hero",
      group: "Layout",
      purpose: "Large image banner with text at the top of a page.",
      props: [{ name: "image", type: "string", description: "Background image.", default: "\"\"" }],
    },
    {
      name: "pricing-grid",
      group: "Pricing and numbers",
      purpose: "Grid of menu items with counts, prices, per-person prices and descriptions.",
      props: [
        {
          name: "items",
          type: "PricedItem[]",
          description: "One card per item.",
          default: "[]",
          items: [
            { name: "name", type: "string", required: true, description: "Item or package name." },
            { name: "count", type: "string" },
            { name: "price", type: "number", description: "Price in dollars, e.g. 12.5." },
            { name: "per-person", type: "boolean" },
          ],
        },
      ],
    },
    {
      name: "bullet-list",
      group: "Text and lists",
      purpose: "Bulleted list from a list of strings.",
      props: [
        { name: "items", type: "string[]", description: "One line per bullet.", default: "[]" },
        { name: "variant", type: "\"default\" | \"warning\"", default: "\"default\"", options: ["default", "warning"] },
      ],
    },
    {
      name: "gallery",
      group: "Layout",
      purpose: "Rows of images shown side by side.",
      props: [{ name: "rows", type: "GalleryImage[][]", default: "[]" }],
    },
  ],
}));

const faqsDoc = [
  "Intro",
  "",
  "::faqs",
  "---",
  "title: Catering FAQ",
  "items:",
  "  - question: How far ahead should I order?",
  "    answer: 24 hours.",
  "  - question: Do you deliver?",
  "    answer: Yes.",
  "---",
  "::",
  "",
].join("\n");

function renderSurface(doc: string) {
  let view: EditorView | null = null;
  render(
    <CodeMirrorPaperSurface
      extendedSyntax={{ githubAlerts: true, highlight: true, mdc: true }}
      initialContent={doc}
      mdcSchema={schema}
      onEditorReady={(next) => {
        view = next;
      }}
      onMarkdownChange={() => {}}
    />,
  );
  const editor = view! as EditorView;
  return {
    doc: () => editor.state.doc.toString(),
    open(name: string) {
      const from = editor.state.doc.toString().indexOf(`::${name}`);
      act(() => {
        editor.dispatch({ effects: openMdcPropsEditor.of(from) });
      });
      return screen.getByRole("complementary", { name: "Component props" });
    },
    view: editor,
  };
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("MdcPropsPanel", () => {
  it("opens from a card's Edit button with the component's schema fields", () => {
    const { view } = renderSurface(faqsDoc);
    expect(screen.queryByRole("complementary", { name: "Component props" })).toBeNull();

    const edit = view.dom.querySelector<HTMLButtonElement>('[data-mdc-name="faqs"] .markra-mdc-chip-edit');
    act(() => edit!.click());
    const panel = screen.getByRole("complementary", { name: "Component props" });

    expect(within(panel).getByRole("heading", { name: "Faqs" })).toBeInTheDocument();
    expect(panel).toHaveTextContent("Text and lists");
    expect(panel).toHaveTextContent("Frequently asked questions with plain-text answers.");
    expect([...panel.querySelectorAll("[data-mdc-prop]")].map((field) => field.getAttribute("data-mdc-prop")))
      .toEqual(["items", "title", "structured-data"]);
    expect(within(panel).getByLabelText("Title")).toHaveValue("Catering FAQ");
    expect(within(panel).getByLabelText("Title")).toHaveAttribute("placeholder", "Frequently Asked Questions");
    expect(within(panel).getByRole("switch", { name: "Structured data" })).toHaveAttribute("aria-checked", "false");
    expect(within(panel).getByRole("button", { name: "How far ahead should I order?" })).toHaveAttribute("aria-expanded", "false");
    expect(within(panel).getByRole("button", { name: "Do you deliver?" })).toBeInTheDocument();
    expect(panel).toHaveTextContent("Questions and answers, in display order.");
  });

  it("writes text edits into the Markdown without losing the field's focus", () => {
    const { doc, open } = renderSurface(faqsDoc);
    const panel = open("faqs");
    const title = within(panel).getByLabelText("Title");

    act(() => title.focus());
    fireEvent.change(title, { target: { value: "Ordering FAQ" } });

    expect(doc()).toBe(faqsDoc.replace("title: Catering FAQ", "title: Ordering FAQ"));
    expect(within(panel).getByLabelText("Title")).toBe(title);
    expect(document.activeElement).toBe(title);
    expect(title).toHaveValue("Ordering FAQ");

    // Typing the default keeps the text in the field while the key leaves the Markdown.
    fireEvent.change(title, { target: { value: "Frequently Asked Questions" } });
    expect(doc()).not.toContain("title:");
    expect(title).toHaveValue("Frequently Asked Questions");
  });

  it("edits, reorders and adds object-list items", () => {
    const { doc, open } = renderSurface(faqsDoc);
    const panel = open("faqs");

    fireEvent.click(within(panel).getByRole("button", { name: "Do you deliver?" }));
    fireEvent.change(within(panel).getAllByLabelText("Answer")[0]!, { target: { value: "Yes, citywide." } });
    expect(doc()).toBe(faqsDoc.replace("answer: Yes.", "answer: Yes, citywide."));

    fireEvent.click(within(panel).getByRole("button", { name: "Move up: Do you deliver?" }));
    expect(doc()).toBe([
      "Intro",
      "",
      "::faqs",
      "---",
      "title: Catering FAQ",
      "items:",
      "  - question: Do you deliver?",
      "    answer: Yes, citywide.",
      "  - question: How far ahead should I order?",
      "    answer: 24 hours.",
      "---",
      "::",
      "",
    ].join("\n"));

    fireEvent.click(within(panel).getByRole("button", { name: "Add item" }));
    expect(doc()).toContain('    answer: 24 hours.\n  - question: ""\n    answer: ""\n---');
    fireEvent.change(within(panel).getAllByLabelText("Question").at(-1)!, { target: { value: "Can I tip?" } });
    expect(doc()).toContain('  - question: Can I tip?\n    answer: ""\n---');
  });

  it("adds a priced item to an empty pricing grid and types its fields", () => {
    const { doc, open } = renderSurface("::pricing-grid\n::\n");
    const panel = open("pricing-grid");

    fireEvent.click(within(panel).getByRole("button", { name: "Add item" }));
    expect(doc()).toBe('::pricing-grid\n---\nitems:\n  - name: ""\n---\n::\n');

    fireEvent.change(within(panel).getByLabelText("Name"), { target: { value: "Taco bar" } });
    fireEvent.change(within(panel).getByLabelText("Price"), { target: { value: "12.5" } });
    fireEvent.click(within(panel).getByRole("switch", { name: "Per person" }));
    expect(doc()).toBe("::pricing-grid\n---\nitems:\n  - name: Taco bar\n    price: 12.5\n    per-person: true\n---\n::\n");
  });

  it("removes a select's key when Default is chosen, and edits string lists", () => {
    const { doc, open } = renderSurface('::bullet-list{variant="warning"}\n---\nitems:\n  - One\n---\n::\n');
    const panel = open("bullet-list");
    const variant = within(panel).getByLabelText("Variant");

    expect(variant).toHaveDisplayValue("warning");
    fireEvent.change(variant, { target: { value: "" } });
    expect(doc()).toBe("::bullet-list\n---\nitems:\n  - One\n---\n::\n");
    expect(within(panel).getByLabelText("Variant")).toHaveDisplayValue("Default (default)");

    fireEvent.click(within(panel).getByRole("button", { name: "Add item" }));
    fireEvent.change(within(panel).getByLabelText("Items 2"), { target: { value: "Two" } });
    expect(doc()).toBe("::bullet-list\n---\nitems:\n  - One\n  - Two\n---\n::\n");

    fireEvent.click(within(panel).getByRole("button", { name: "Remove item: Items 1" }));
    expect(doc()).toBe("::bullet-list\n---\nitems:\n  - Two\n---\n::\n");
  });

  it("makes each change one undo step", () => {
    const { doc, open, view } = renderSurface("::hero\n::\n");
    const panel = open("hero");

    fireEvent.change(within(panel).getByLabelText("Image"), { target: { value: "/a.jpg" } });
    expect(doc()).toBe("::hero\n---\nimage: /a.jpg\n---\n::\n");

    act(() => {
      undo(view);
    });
    expect(doc()).toBe("::hero\n::\n");
    expect(within(panel).getByLabelText("Image")).toHaveValue("");
  });

  it("shows invalid YAML and blocks editing until it is fixed in the text", () => {
    const { open } = renderSurface("::faqs\n---\ntitle: [unclosed\n---\n::\n");
    const panel = open("faqs");

    expect(within(panel).getByRole("alert")).toHaveTextContent("The props block isn't valid YAML.");
    expect(within(panel).getByLabelText("Title")).toBeDisabled();
    expect(within(panel).getByRole("switch", { name: "Structured data" })).toBeDisabled();
  });

  it("validates free-form values before writing them", () => {
    const { doc, open } = renderSurface("::gallery\n::\n");
    const panel = open("gallery");
    const rows = within(panel).getByLabelText("Rows");

    fireEvent.change(rows, { target: { value: "- - src: [" } });
    expect(within(panel).getByRole("alert")).toHaveTextContent("Not valid YAML or JSON.");
    expect(doc()).toBe("::gallery\n::\n");

    fireEvent.change(rows, { target: { value: '[[{"src": "/a.jpg"}]]' } });
    expect(within(panel).queryByRole("alert")).toBeNull();
    expect(doc()).toBe("::gallery\n---\nrows:\n  - - src: /a.jpg\n---\n::\n");
  });

  it("shows an unknown component's props read-only, and closes on Escape", () => {
    const { open, view } = renderSurface('::mystery{tone="loud"}\n::\n');
    const panel = open("mystery");

    expect(panel).toHaveTextContent("This component isn't in the workspace schema.");
    expect(panel).toHaveTextContent("Other props");
    expect(panel).toHaveTextContent("tone");
    expect(panel).toHaveTextContent("loud");
    expect(panel.querySelector("input, textarea, select")).toBeNull();

    fireEvent.keyDown(panel, { key: "Escape" });
    expect(screen.queryByRole("complementary", { name: "Component props" })).toBeNull();
    expect(document.activeElement).toBe(view.contentDOM);
  });

  it("lists props that are not in the schema under Other props", () => {
    const { open } = renderSurface("::hero\n---\nimage: /a.jpg\nlegacy: true\n---\n::\n");
    const panel = open("hero");

    expect(within(panel).getByLabelText("Image")).toHaveValue("/a.jpg");
    const other = within(panel).getByRole("heading", { name: "Other props" }).parentElement!;
    expect(other).toHaveTextContent("legacy");
    expect(other).toHaveTextContent("true");
  });
});
