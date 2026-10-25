import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { GFM } from "@lezer/markdown";
import { describe, expect, it } from "vitest";
import { writeMdcProp } from "../mdc/index.ts";
import { mdcBlocksForState } from "./mdc-blocks.ts";
import {
  mdcPropsChanges,
  mdcPropsEditorField,
  openMdcBlock,
  openMdcPropsEditor,
  readMdcPropsText,
  type MdcPropsText,
} from "./mdc-edit.ts";
import { markraMdc } from "./mdc-syntax.ts";

const extensions = [markdown({ extensions: [GFM, markraMdc] }), mdcPropsEditorField];

function state(doc: string) {
  const created = EditorState.create({ doc, extensions });
  ensureSyntaxTree(created, created.doc.length, 5000);
  return created;
}

function apply(doc: string, next: MdcPropsText, index = 0) {
  const current = state(doc);
  const block = mdcBlocksForState(current)[index]!;
  return current.update({ changes: mdcPropsChanges(current, block, next) }).state.doc.toString();
}

describe("readMdcPropsText", () => {
  it("reads the attribute and YAML text", () => {
    const current = state('::hero{image="/a.jpg"}\n---\ntitle: Hi\n---\n::\n');
    expect(readMdcPropsText(current, mdcBlocksForState(current)[0]!)).toEqual({
      attributes: 'image="/a.jpg"',
      yaml: "title: Hi",
    });
  });

  it("distinguishes an empty props block from none", () => {
    const empty = state("::hero\n---\n---\n::\n");
    expect(readMdcPropsText(empty, mdcBlocksForState(empty)[0]!).yaml).toBe("");
    const none = state("::hero\n::\n");
    expect(readMdcPropsText(none, mdcBlocksForState(none)[0]!)).toEqual({ attributes: null, yaml: null });
  });
});

describe("mdcPropsChanges", () => {
  it("edits only the YAML between the fences", () => {
    expect(apply("::hero\n---\ntitle: Hi\n---\nBody\n::\n", { attributes: null, yaml: "title: Hello" }))
      .toBe("::hero\n---\ntitle: Hello\n---\nBody\n::\n");
  });

  it("adds a props block after the opener, keeping its indentation", () => {
    expect(apply("- item\n  ::hero\n  ::\n", { attributes: null, yaml: "  title: Hi" }))
      .toBe("- item\n  ::hero\n  ---\n  title: Hi\n  ---\n  ::\n");
  });

  it("fills an empty props block", () => {
    expect(apply("::hero\n---\n---\n::\n", { attributes: null, yaml: "title: Hi" }))
      .toBe("::hero\n---\ntitle: Hi\n---\n::\n");
  });

  it("removes the props block", () => {
    expect(apply("::hero\n---\ntitle: Hi\n---\nBody\n::\n", { attributes: null, yaml: null }))
      .toBe("::hero\nBody\n::\n");
  });

  it("adds, edits and removes attributes", () => {
    expect(apply("::card\n::\n", { attributes: 'title="A"', yaml: null })).toBe('::card{title="A"}\n::\n');
    expect(apply("::card[Label]\n::\n", { attributes: "number", yaml: null })).toBe("::card[Label]{number}\n::\n");
    expect(apply('::card{title="A"}\n::\n', { attributes: 'title="B"', yaml: null })).toBe('::card{title="B"}\n::\n');
    expect(apply('::card{title="A"}\n::\n', { attributes: null, yaml: null })).toBe("::card\n::\n");
  });

  it("round-trips a form edit from the props model", () => {
    const doc = '::faqs\n---\nitems:\n  - question: "How far ahead?"\n    answer: 24 hours\n---\n::\n';
    const current = state(doc);
    const block = mdcBlocksForState(current)[0]!;
    const write = writeMdcProp(readMdcPropsText(current, block), ["items", 0, "answer"], "Two days: plan ahead");
    const next = current.update({ changes: mdcPropsChanges(current, block, write) }).state.doc.toString();
    expect(next).toBe('::faqs\n---\nitems:\n  - question: "How far ahead?"\n    answer: "Two days: plan ahead"\n---\n::\n');
  });
});

describe("mdcPropsEditorField", () => {
  it("follows its block through edits and closes when the block goes away", () => {
    let current = state("Intro\n\n::hero\n::\n");
    const from = mdcBlocksForState(current)[0]!.open.from;
    current = current.update({ effects: openMdcPropsEditor.of(from) }).state;
    expect(openMdcBlock(current)?.name).toBe("hero");

    current = current.update({ changes: { from: 0, insert: "More " } }).state;
    ensureSyntaxTree(current, current.doc.length, 5000);
    expect(openMdcBlock(current)?.name).toBe("hero");

    const opener = mdcBlocksForState(current)[0]!.openLine;
    current = current.update({ changes: { from: opener.from, to: opener.to, insert: "plain" } }).state;
    expect(current.field(mdcPropsEditorField)).toBeNull();
  });
});
