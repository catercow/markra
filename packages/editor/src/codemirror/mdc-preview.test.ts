import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it, vi } from "vitest";
import { liveMarkdown, mdcPreviewPlugin } from "./index.ts";
import { yamlLineTokens } from "./mdc-preview.ts";
import type { MdcPreviewPluginOptions } from "./mdc-preview.ts";
import "./dom.test-support.ts";

const views: EditorView[] = [];

function createView(doc: string, options: MdcPreviewPluginOptions = {}, cursor = doc.length) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc,
      extensions: [liveMarkdown({ mdc: true, plugins: [mdcPreviewPlugin(options)] })],
      selection: EditorSelection.cursor(cursor),
    }),
  });
  view.focus();
  view.dispatch({ selection: view.state.selection });
  views.push(view);
  return view;
}

function lineRoles(view: EditorView) {
  return Array.from(view.dom.querySelectorAll(".cm-line")).map((line) => {
    const role = ["open", "props", "body", "close"].find((name) => line.classList.contains(`markra-mdc-${name}`));
    const depth = /markra-mdc-depth-(\d)/u.exec(line.className)?.[1];
    return role ? `${role}${depth}` : "-";
  });
}

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.replaceChildren();
});

const FAQ = 'Intro\n\n::faqs{title="Questions"}\n---\nitems:\n  - question: "How far ahead?"\n    answer: 24 hours # usually\n---\nBody text\n::\n\nAfter';

describe("mdcPreviewPlugin", () => {
  it("draws a component as a card without changing the Markdown", () => {
    const view = createView(FAQ);
    expect(lineRoles(view)).toEqual(["-", "-", "open0", "props0", "props0", "props0", "props0", "props0", "body0", "close0", "-", "-"]);
    expect(view.dom.querySelector(".markra-mdc-chip-label")?.textContent).toBe("Faqs");
    expect(view.dom.querySelector(".markra-mdc-chip-attributes")?.textContent).toBe('title="Questions"');
    expect(view.dom.querySelector(".markra-mdc-close-label")?.textContent).toBe("End faqs");
    expect(view.dom.querySelectorAll(".markra-mdc-spacer")).toHaveLength(2);
    expect(view.state.doc.toString()).toBe(FAQ);
  });

  it("highlights YAML props", () => {
    const view = createView(FAQ);
    const text = (cls: string) => Array.from(view.dom.querySelectorAll(`.markra-mdc-yaml-${cls}`)).map((node) => node.textContent);
    expect(text("key")).toEqual(["items", "question", "answer"]);
    expect(text("string")).toEqual(['"How far ahead?"', "24 hours"]);
    expect(text("comment")).toEqual(["# usually"]);
  });

  it("reveals the source of the opener and closer under the caret", () => {
    const view = createView(FAQ, {}, FAQ.indexOf("::faqs") + 3);
    expect(view.dom.querySelector(".markra-mdc-chip")).toBeNull();
    expect(view.dom.textContent).toContain('::faqs{title="Questions"}');
    expect(view.dom.querySelector(".markra-mdc-close-label")).not.toBeNull();
  });

  it("nests cards by depth", () => {
    const doc = ":::grid{columns=2}\n::card{title=\"One\"}\nA\n::\n:::\n";
    const view = createView(doc);
    expect(lineRoles(view)).toEqual(["open0", "open1", "body1", "close1", "close0", "-"]);
  });

  it("flags components missing from the schema", () => {
    const view = createView(FAQ, { components: () => [{ name: "hero", purpose: "Top banner" }] });
    expect(view.dom.querySelector(".markra-mdc-chip-unknown")).not.toBeNull();
    const known = createView(FAQ, { components: () => [{ name: "faqs", purpose: "Questions and answers" }] });
    expect(known.dom.querySelector(".markra-mdc-chip-unknown")).toBeNull();
    expect(known.dom.querySelector<HTMLElement>(".markra-mdc-chip")?.title).toBe("Questions and answers");
  });

  it("opens the component editor from the chip", () => {
    const onOpenComponent = vi.fn();
    const view = createView(FAQ, { onOpenComponent });
    view.dom.querySelector<HTMLButtonElement>(".markra-mdc-chip-edit")?.click();
    expect(onOpenComponent).toHaveBeenCalledTimes(1);
    expect(onOpenComponent.mock.calls[0]?.[1]).toMatchObject({ name: "faqs", propsClosed: true });
    expect(view.state.doc.toString()).toBe(FAQ);
  });

  it("produces the same lines after editing as after recreation", () => {
    const view = createView(FAQ);
    const at = FAQ.indexOf("Body text");
    view.dispatch({ changes: { from: at, insert: ":::grid\n::card\nX\n::\n:::\n" } });
    const recreated = createView(view.state.doc.toString());
    expect(lineRoles(view)).toEqual(lineRoles(recreated));
  });

  it("does nothing when disabled", () => {
    const view = createView(FAQ, { enabled: false });
    expect(view.dom.querySelector(".cm-markra-mdc")).toBeNull();
  });
});

describe("yamlLineTokens", () => {
  it.each([
    ["title: Hello", [["key", "title"], ["string", "Hello"]]],
    ["  - label: \"Serves\"", [["dash", "-"], ["key", "label"], ["string", "\"Serves\""]]],
    ["count: 12", [["key", "count"], ["literal", "12"]]],
    ["flag: true # on", [["key", "flag"], ["literal", "true"], ["comment", "# on"]]],
    ["body: |", [["key", "body"], ["punctuation", "|"]]],
    ["  - plain item", [["dash", "-"], ["string", "plain item"]]],
    ["url: https://catercow.com/x", [["key", "url"], ["string", "https://catercow.com/x"]]],
    ["# just a comment", [["comment", "# just a comment"]]],
  ])("%s", (line, expected) => {
    expect(yamlLineTokens(line).map((token) => [token.kind, line.slice(token.from, token.to)])).toEqual(expected);
  });
});
