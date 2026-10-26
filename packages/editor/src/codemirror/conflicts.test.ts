import { history, undo } from "@codemirror/commands";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import { conflictMarkerKind } from "./conflict-syntax.ts";
import {
  conflictMarkersPlugin,
  countConflictBlocks,
  readConflictBlocks,
  selectNextConflict,
} from "./conflicts.ts";
import { liveMarkdown } from "./index.ts";
import "./dom.test-support.ts";

const views: EditorView[] = [];

const conflicted = [
  "# Prices",
  "",
  "<<<<<<< yours",
  "Lunch is $12.",
  "=======",
  "Lunch is $15.",
  ">>>>>>> theirs",
  "",
  "Dinner is $20.",
  "",
].join("\n");

function createView(doc: string, options: { readOnly?: boolean } = {}) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc,
      extensions: [
        liveMarkdown({
          conflicts: true,
          plugins: [conflictMarkersPlugin({ labels: { keepMine: "Mine please" } })],
        }),
        history(),
        EditorState.readOnly.of(options.readOnly ?? false),
      ],
      selection: EditorSelection.cursor(0),
    }),
  });
  views.push(view);
  return view;
}

function click(view: EditorView, choice: string, index = 0) {
  const buttons = view.dom.querySelectorAll<HTMLButtonElement>(`.markra-conflict-button[data-choice="${choice}"]`);
  buttons[index].dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0 }));
}

function nodeNames(doc: string) {
  const state = EditorState.create({ doc, extensions: [liveMarkdown({ conflicts: true })] });
  const names: string[] = [];
  ensureSyntaxTree(state, state.doc.length, 5000)?.iterate({
    enter(node) {
      names.push(node.name);
    },
  });
  return names;
}

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.replaceChildren();
});

describe("conflict markers", () => {
  it("recognises marker lines, with or without labels", () => {
    expect(conflictMarkerKind("<<<<<<< yours")).toBe("start");
    expect(conflictMarkerKind("<<<<<<< HEAD\r")).toBe("start");
    expect(conflictMarkerKind("||||||| base")).toBe("base");
    expect(conflictMarkerKind("=======")).toBe("separator");
    expect(conflictMarkerKind(">>>>>>>")).toBe("end");
    expect(conflictMarkerKind("<<<<<<<< eight")).toBeNull();
    expect(conflictMarkerKind("> quote")).toBeNull();
  });

  it("reads each block's sides", () => {
    const view = createView(conflicted);
    const [block] = readConflictBlocks(view.state.doc);
    expect(view.state.sliceDoc(block.mine.from, block.mine.to)).toBe("Lunch is $12.\n");
    expect(view.state.sliceDoc(block.theirs.from, block.theirs.to)).toBe("Lunch is $15.\n");
    expect(countConflictBlocks(conflicted)).toBe(1);
    expect(countConflictBlocks("Title\n=======\n")).toBe(0);
  });

  it("keeps marker lines out of headings and quotes, but leaves real heading underlines alone", () => {
    const names = nodeNames(conflicted);
    expect(names.filter((name) => name === "ConflictMarker")).toHaveLength(3);
    expect(names).not.toContain("SetextHeading1");
    expect(names).not.toContain("Blockquote");
    expect(nodeNames("Title\n=======\n\ntext\n")).toContain("SetextHeading1");
  });

  it("shows a bar with the three choices and a count above the document", () => {
    const view = createView(conflicted);
    const bar = view.dom.querySelector(".markra-conflict-bar");
    expect(bar?.textContent).toContain("Your version");
    expect([...view.dom.querySelectorAll(".markra-conflict-button[data-choice]")].map((button) => button.textContent)).toEqual([
      "Mine please",
      "Use theirs",
      "Keep both",
    ]);
    expect(view.dom.querySelector(".markra-conflict-divider")?.textContent).toBe("Their version");
    expect(view.dom.querySelectorAll(".cm-line.markra-conflict-mine")).toHaveLength(1);
    expect(view.dom.querySelectorAll(".cm-line.markra-conflict-theirs")).toHaveLength(1);
    expect(view.dom.querySelector(".markra-conflict-count")?.textContent).toBe("1 conflict to resolve");
  });

  it.each([
    ["mine", "Lunch is $12.\n"],
    ["theirs", "Lunch is $15.\n"],
    ["both", "Lunch is $12.\nLunch is $15.\n"],
  ])("resolving with %s rewrites only that block, as one undoable change", (choice, lines) => {
    const view = createView(conflicted);
    click(view, choice);
    expect(view.state.doc.toString()).toBe(`# Prices\n\n${lines}\nDinner is $20.\n`);
    expect(view.dom.querySelector(".markra-conflict-bar")).toBeNull();
    expect(view.dom.querySelector(".markra-conflict-panel")).toBeNull();
    undo(view);
    expect(view.state.doc.toString()).toBe(conflicted);
  });

  it("counts the conflicts left and resolves the one whose bar was clicked", () => {
    const doc = `${conflicted}<<<<<<< yours\nA\n=======\nB\n>>>>>>> theirs\n`;
    const view = createView(doc);
    expect(view.dom.querySelector(".markra-conflict-count")?.textContent).toBe("2 conflicts to resolve");
    click(view, "theirs", 1);
    expect(view.state.doc.toString()).toBe(`${conflicted}B\n`);
    expect(view.dom.querySelector(".markra-conflict-count")?.textContent).toBe("1 conflict to resolve");
  });

  it("jumps to the next conflict", () => {
    const doc = `${conflicted}<<<<<<< yours\nA\n=======\nB\n>>>>>>> theirs\n`;
    const view = createView(doc);
    const blocks = readConflictBlocks(view.state.doc);
    selectNextConflict(view);
    expect(view.state.selection.main.head).toBe(blocks[0].mine.from);
    selectNextConflict(view);
    expect(view.state.selection.main.head).toBe(blocks[1].mine.from);
    selectNextConflict(view);
    expect(view.state.selection.main.head).toBe(blocks[0].mine.from);
  });

  it("ignores a block at the end of the file without a final newline", () => {
    const view = createView("a\n<<<<<<< yours\nx\n=======\ny\n>>>>>>> theirs");
    click(view, "mine");
    expect(view.state.doc.toString()).toBe("a\nx\n");
  });

  it("changes nothing in a read-only editor", () => {
    const view = createView(conflicted, { readOnly: true });
    click(view, "mine");
    expect(view.state.doc.toString()).toBe(conflicted);
  });
});
