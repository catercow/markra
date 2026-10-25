import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState, Text } from "@codemirror/state";
import { GFM, parser as baseParser } from "@lezer/markdown";
import { describe, expect, it } from "vitest";
import { markraSetextHeading } from "./setext-heading.ts";
import { readMdcBlocksFromTree } from "./mdc-blocks.ts";
import { comarkFixtures } from "./mdc-syntax.fixtures.ts";
import { markraMdc } from "./mdc-syntax.ts";

const parser = baseParser.configure([GFM, markraSetextHeading, markraMdc]);

function tree(doc: string) {
  return parser.parse(doc);
}

function nodes(doc: string, name: string) {
  const found: { from: number; to: number; text: string }[] = [];
  tree(doc).iterate({
    enter(ref) {
      if (ref.name === name) found.push({ from: ref.from, to: ref.to, text: doc.slice(ref.from, ref.to) });
    },
  });
  return found;
}

function blocks(doc: string) {
  return readMdcBlocksFromTree(tree(doc), Text.of(doc.split("\n")));
}

describe("MDC block syntax", () => {
  it("parses an opener with a YAML props block instead of a setext heading", () => {
    const doc = "::facts-grid\n---\nitems:\n  - label: Serves\n    value: \"10\"\n---\n::\n";
    expect(tree(doc).toString()).toBe(
      "Document(MdcBlockOpen(MdcBlockMark,MdcName,MdcProps(MdcPropsMark,MdcPropsContent,MdcPropsMark)),MdcBlockClose(MdcBlockMark))",
    );
    expect(nodes(doc, "MdcPropsContent")[0]?.text).toBe("items:\n  - label: Serves\n    value: \"10\"");
  });

  it("parses inline attributes, including braces and quotes inside values", () => {
    const doc = '::card{title="A {curly} \\"quoted\\" title" number}\nBody\n::\n';
    expect(nodes(doc, "MdcAttributes").map((node) => node.text)).toEqual([
      '{title="A {curly} \\"quoted\\" title" number}',
    ]);
    expect(nodes(doc, "Paragraph").map((node) => node.text)).toEqual(["Body"]);
  });

  it("keeps a thematic break that does not directly follow the opener", () => {
    const doc = "::card\nAbove\n\n---\n\nBelow\n::\n";
    expect(tree(doc).toString()).toBe(
      "Document(MdcBlockOpen(MdcBlockMark,MdcName),Paragraph,HorizontalRule,Paragraph,MdcBlockClose(MdcBlockMark))",
    );
  });

  it("lets an opener or closer interrupt a paragraph", () => {
    const doc = "Intro text\n::callout\nInside\n::\nAfter\n";
    expect(tree(doc).toString()).toBe(
      "Document(Paragraph,MdcBlockOpen(MdcBlockMark,MdcName),Paragraph,MdcBlockClose(MdcBlockMark),Paragraph)",
    );
  });

  it("leaves components inside fenced code alone", () => {
    const doc = "```md\n::card\n---\na: 1\n---\n::\n```\n";
    expect(nodes(doc, "MdcBlockOpen")).toEqual([]);
    expect(nodes(doc, "FencedCode")).toHaveLength(1);
  });

  it("ends an unclosed props block at the component's closer", () => {
    const doc = "::faqs\n---\nitems: []\n::\n";
    const [block] = blocks(doc);
    expect(block?.propsClosed).toBe(false);
    expect(block?.close).not.toBeNull();
    expect(nodes(doc, "MdcPropsContent")[0]?.text).toBe("items: []");
  });

  it("parses named slots with attributes", () => {
    const doc = '::comp\n#header{unwrap="p"}\nTitle\n#default\nBody\n::\n';
    expect(nodes(doc, "MdcSlot").map((node) => node.text)).toEqual(['#header{unwrap="p"}', "#default"]);
    expect(nodes(doc, "ATXHeading1")).toEqual([]);
  });

  it("does not claim prose that only looks like MDC", () => {
    for (const doc of ["Note: this is fine\n", "Visit https://catercow.com today\n", "#hashtag text\n", ":: not a name\n", "::card trailing words\n"]) {
      expect(nodes(doc, "MdcBlockOpen"), doc).toEqual([]);
      expect(nodes(doc, "MdcInline"), doc).toEqual([]);
    }
  });

  it("parses inline components and attribute suffixes", () => {
    const doc = 'A :badge[New]{color="red"} and **bold**{.lead} and [span]{.hl} and `code`{lang="ts"}\n';
    expect(nodes(doc, "MdcInline").map((node) => node.text)).toEqual([':badge[New]{color="red"}']);
    expect(nodes(doc, "MdcAttributes").map((node) => node.text)).toEqual([
      '{color="red"}',
      "{.lead}",
      "{.hl}",
      '{lang="ts"}',
    ]);
  });

  it("pairs nested blocks by colon count", () => {
    const doc = ":::grid{columns=2}\n::card{title=\"One\"}\nA\n::\n\n::card{title=\"Two\"}\nB\n::\n:::\n";
    const result = blocks(doc);
    expect(result.map((block) => [block.name, block.depth, block.colons, Boolean(block.close)])).toEqual([
      ["grid", 0, 3, true],
      ["card", 1, 2, true],
      ["card", 1, 2, true],
    ]);
    expect(doc.slice(result[0]!.close!.from, result[0]!.close!.to)).toBe(":::");
  });

  it("parses components inside list items", () => {
    const doc = "- item\n  ::block\n  ---\n  a: 1\n  ---\n  ::\n";
    expect(nodes(doc, "MdcBlockOpen")).toHaveLength(1);
    expect(nodes(doc, "MdcPropsContent")[0]?.text).toBe("  a: 1");
    expect(nodes(doc, "MdcBlockClose")).toHaveLength(1);
  });
});

describe("MDC syntax against the comark spec inputs", () => {
  const OPENER = /^(:{2,})[A-Za-z][\w-]*(\[[^\]]*\])?(\{.*\})?\s*$/u;
  const CLOSER = /^:{2,}\s*$/u;

  // An independent line scan: which lines must be openers, closers or props.
  function expectedLines(doc: string) {
    const lines = doc.split("\n");
    const openers: number[] = [];
    const closers: number[] = [];
    let fence: string | null = null;
    for (let i = 0; i < lines.length; i++) {
      const text = lines[i]!.replace(/^(\s*[-*+] )?\s*/u, "");
      const fenceMatch = /^(`{3,}|~{3,})/u.exec(text);
      if (fence) {
        if (fenceMatch && fenceMatch[1]!.startsWith(fence) && text.trim() === fenceMatch[1]) fence = null;
        continue;
      }
      if (fenceMatch) {
        fence = fenceMatch[1]!;
        continue;
      }
      if (OPENER.test(text)) {
        openers.push(i);
        if (lines[i + 1]?.trim() === "---") {
          let j = i + 2;
          while (j < lines.length && lines[j]!.trim() !== "---" && !CLOSER.test(lines[j]!.trim())) j++;
          i = lines[j]?.trim() === "---" ? j : j - 1;
        }
      } else if (CLOSER.test(text)) {
        closers.push(i);
      }
    }
    return { openers, closers };
  }

  function lineOf(doc: string, pos: number) {
    return doc.slice(0, pos).split("\n").length - 1;
  }

  for (const [name, doc] of Object.entries(comarkFixtures)) {
    it(name, () => {
      const expected = expectedLines(doc);
      expect(nodes(doc, "MdcBlockOpen").map((node) => lineOf(doc, node.from))).toEqual(expected.openers);
      expect(nodes(doc, "MdcBlockClose").map((node) => lineOf(doc, node.from))).toEqual(expected.closers);
      // Frontmatter is handled outside this parser.
      if (!doc.startsWith("---")) expect(nodes(doc, "SetextHeading2")).toEqual([]);
      if (expected.openers.length === expected.closers.length) {
        expect(blocks(doc).every((block) => block.close !== null)).toBe(true);
      }
    });
  }
});

describe("MDC syntax in an editor state", () => {
  const language = markdown({ extensions: [GFM, markraSetextHeading, markraMdc] });

  function treeString(state: EditorState) {
    return ensureSyntaxTree(state, state.doc.length, 5000)!.toString();
  }

  it("produces the same tree after incremental edits as after recreation", () => {
    let state = EditorState.create({ doc: "Intro\n\n::card\nBody\n::\n", extensions: language });
    treeString(state);
    const insertAt = "Intro\n\n::card".length;
    state = state.update({ changes: { from: insertAt, insert: "\n---\ntitle: Hi\n---" } }).state;
    const incremental = treeString(state);
    const fresh = treeString(EditorState.create({ doc: state.doc.toString(), extensions: language }));
    expect(incremental).toBe(fresh);
    expect(incremental).toContain("MdcProps(MdcPropsMark,MdcPropsContent,MdcPropsMark)");

    state = state.update({ changes: { from: insertAt, to: insertAt + 4 } }).state;
    expect(treeString(state)).toBe(
      treeString(EditorState.create({ doc: state.doc.toString(), extensions: language })),
    );
  });
});
