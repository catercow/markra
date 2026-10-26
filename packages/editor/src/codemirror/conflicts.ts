import { EditorSelection, Facet, StateField, type EditorState, type Text } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  WidgetType,
  showPanel,
  type DecorationSet,
  type Panel,
  type ViewUpdate,
} from "@codemirror/view";
import { conflictMarkerKind } from "./conflict-syntax.ts";
import { defineMarkraPlugin } from "./plugin.ts";

export interface ConflictLabels {
  yours: string;
  theirs: string;
  keepMine: string;
  useTheirs: string;
  keepBoth: string;
  /** `{count}` is filled in. */
  count: string;
  countOne: string;
  next: string;
}

export const defaultConflictLabels: ConflictLabels = {
  count: "{count} conflicts to resolve",
  countOne: "1 conflict to resolve",
  keepBoth: "Keep both",
  keepMine: "Keep mine",
  next: "Next conflict",
  theirs: "Their version",
  useTheirs: "Use theirs",
  yours: "Your version",
};

export interface ConflictRange {
  readonly from: number;
  readonly to: number;
}

/** One `<<<<<<<` … `>>>>>>>` block. Marker ranges cover the marker line's text. */
export interface ConflictBlock {
  readonly from: number;
  readonly to: number;
  readonly start: ConflictRange;
  readonly base: ConflictRange | null;
  readonly separator: ConflictRange;
  readonly end: ConflictRange;
  /** Your lines, from the start of the first to the start of the next marker. */
  readonly mine: ConflictRange;
  readonly theirs: ConflictRange;
}

export type ConflictChoice = "mine" | "theirs" | "both";

export function readConflictBlocks(doc: Text): ConflictBlock[] {
  const blocks: ConflictBlock[] = [];
  let start: ConflictRange | null = null;
  let base: ConflictRange | null = null;
  let separator: ConflictRange | null = null;
  for (let number = 1; number <= doc.lines; number += 1) {
    const line = doc.line(number);
    const kind = conflictMarkerKind(line.text);
    if (!kind) continue;
    const range = { from: line.from, to: line.to };
    if (kind === "start") {
      start = range;
      base = null;
      separator = null;
    } else if (!start) {
      continue;
    } else if (kind === "base" && !base && !separator) {
      base = range;
    } else if (kind === "separator" && !separator) {
      separator = range;
    } else if (kind === "end" && separator) {
      blocks.push({
        base,
        end: range,
        from: start.from,
        mine: { from: start.to + 1, to: (base ?? separator).from },
        separator,
        start,
        theirs: { from: separator.to + 1, to: range.from },
        to: range.to,
      });
      start = null;
    }
  }
  return blocks;
}

/** The change that replaces a whole block (its markers included) with the chosen lines. */
export function conflictResolution(doc: Text, block: ConflictBlock, choice: ConflictChoice) {
  const mine = doc.sliceString(block.mine.from, block.mine.to);
  const theirs = doc.sliceString(block.theirs.from, block.theirs.to);
  const insert = choice === "mine" ? mine : choice === "theirs" ? theirs : mine + theirs;
  const to = block.to < doc.length ? block.to + 1 : block.to;
  return { from: block.from, insert, to };
}

const conflictLabelsFacet = Facet.define<ConflictLabels, ConflictLabels>({
  combine: (values) => values[0] ?? defaultConflictLabels,
});

interface ConflictState {
  blocks: readonly ConflictBlock[];
  decorations: DecorationSet;
}

/** Resolves the conflict containing `pos` (a position in its first marker line or inside it). */
export function resolveConflictAt(view: EditorView, pos: number, choice: ConflictChoice) {
  if (view.state.readOnly) return false;
  const block = view.state.field(conflictField, false)?.blocks.find((candidate) => candidate.from <= pos && pos <= candidate.to)
    ?? readConflictBlocks(view.state.doc).find((candidate) => candidate.from <= pos && pos <= candidate.to);
  if (!block) return false;
  const change = conflictResolution(view.state.doc, block, choice);
  view.dispatch({
    changes: change,
    selection: EditorSelection.cursor(change.from),
    scrollIntoView: true,
    userEvent: "input.resolve-conflict",
  });
  view.focus();
  return true;
}

/** Moves the cursor to the next conflict after it, wrapping round. */
export function selectNextConflict(view: EditorView) {
  const blocks = view.state.field(conflictField, false)?.blocks ?? [];
  if (blocks.length === 0) return false;
  const head = view.state.selection.main.head;
  const next = blocks.find((block) => block.from > head) ?? blocks[0];
  view.dispatch({
    effects: EditorView.scrollIntoView(next.from, { y: "center" }),
    selection: EditorSelection.cursor(next.mine.from),
  });
  view.focus();
  return true;
}

function stopEditorPointer(element: HTMLElement) {
  element.addEventListener("mousedown", (event) => {
    event.preventDefault();
    event.stopPropagation();
  });
}

function sameLabels(a: ConflictLabels, b: ConflictLabels) {
  return (Object.keys(a) as Array<keyof ConflictLabels>).every((key) => a[key] === b[key]);
}

class ConflictBarWidget extends WidgetType {
  constructor(readonly labels: ConflictLabels) {
    super();
  }

  eq(other: ConflictBarWidget) {
    return sameLabels(this.labels, other.labels);
  }

  ignoreEvent() {
    return true;
  }

  toDOM(view: EditorView) {
    const document = view.dom.ownerDocument;
    const bar = document.createElement("div");
    bar.className = "markra-conflict-bar";
    bar.setAttribute("role", "group");
    bar.setAttribute("aria-label", this.labels.yours);
    const label = document.createElement("span");
    label.className = "markra-conflict-label";
    label.textContent = this.labels.yours;
    bar.append(label);

    const choices: Array<[ConflictChoice, string]> = [
      ["mine", this.labels.keepMine],
      ["theirs", this.labels.useTheirs],
      ["both", this.labels.keepBoth],
    ];
    for (const [choice, text] of choices) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "markra-conflict-button";
      button.dataset.choice = choice;
      button.textContent = text;
      stopEditorPointer(button);
      button.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        resolveConflictAt(view, view.posAtDOM(bar), choice);
      });
      bar.append(button);
    }
    return bar;
  }
}

class ConflictDividerWidget extends WidgetType {
  constructor(readonly label: string) {
    super();
  }

  eq(other: ConflictDividerWidget) {
    return this.label === other.label;
  }

  toDOM(view: EditorView) {
    const divider = view.dom.ownerDocument.createElement("div");
    divider.className = "markra-conflict-divider";
    divider.textContent = this.label;
    return divider;
  }
}

class ConflictEndWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM(view: EditorView) {
    const end = view.dom.ownerDocument.createElement("div");
    end.className = "markra-conflict-end";
    return end;
  }
}

const mineLine = Decoration.line({ class: "markra-conflict-mine" });
const theirsLine = Decoration.line({ class: "markra-conflict-theirs" });
const baseLine = Decoration.line({ class: "markra-conflict-base" });

function lineDecorations(doc: Text, range: ConflictRange, decoration: Decoration) {
  if (range.to <= range.from) return [];
  const first = doc.lineAt(range.from).number;
  const last = doc.lineAt(range.to - 1).number;
  const result = [];
  for (let number = first; number <= last; number += 1) {
    result.push(decoration.range(doc.line(number).from));
  }
  return result;
}

function buildConflictState(state: EditorState): ConflictState {
  const blocks = readConflictBlocks(state.doc);
  if (blocks.length === 0) return { blocks, decorations: Decoration.none };
  const labels = state.facet(conflictLabelsFacet);
  const ranges = blocks.flatMap((block) => [
    Decoration.replace({ block: true, widget: new ConflictBarWidget(labels) }).range(block.start.from, block.start.to),
    ...lineDecorations(state.doc, block.mine, mineLine),
    ...(block.base
      ? [
          Decoration.replace({ block: true, widget: new ConflictDividerWidget("") }).range(block.base.from, block.base.to),
          ...lineDecorations(state.doc, { from: block.base.to + 1, to: block.separator.from }, baseLine),
        ]
      : []),
    Decoration.replace({ block: true, widget: new ConflictDividerWidget(labels.theirs) }).range(
      block.separator.from,
      block.separator.to,
    ),
    ...lineDecorations(state.doc, block.theirs, theirsLine),
    Decoration.replace({ block: true, widget: new ConflictEndWidget() }).range(block.end.from, block.end.to),
  ]);
  return { blocks, decorations: Decoration.set(ranges, true) };
}

const conflictField = StateField.define<ConflictState>({
  create: buildConflictState,
  update(value, transaction) {
    const labelsChanged = !sameLabels(
      transaction.startState.facet(conflictLabelsFacet),
      transaction.state.facet(conflictLabelsFacet),
    );
    if (!transaction.docChanged && !labelsChanged) return value;
    return buildConflictState(transaction.state);
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value) => value.decorations),
    showPanel.from(field, (value) => (value.blocks.length > 0 ? conflictPanel : null)),
  ],
});

function countText(labels: ConflictLabels, count: number) {
  return count === 1 ? labels.countOne : labels.count.replace("{count}", String(count));
}

function conflictPanel(view: EditorView): Panel {
  const document = view.dom.ownerDocument;
  const dom = document.createElement("div");
  dom.className = "markra-conflict-panel";
  dom.setAttribute("role", "status");
  const text = document.createElement("span");
  text.className = "markra-conflict-count";
  const next = document.createElement("button");
  next.type = "button";
  next.className = "markra-conflict-button";
  stopEditorPointer(next);
  next.addEventListener("click", () => selectNextConflict(view));
  dom.append(text, next);

  const render = (state: EditorState) => {
    const labels = state.facet(conflictLabelsFacet);
    text.textContent = countText(labels, state.field(conflictField).blocks.length);
    next.textContent = labels.next;
  };
  render(view.state);
  return {
    dom,
    top: true,
    update: (update: ViewUpdate) => render(update.state),
  };
}

/** Number of complete conflict blocks in `text`. */
export function countConflictBlocks(text: string) {
  let count = 0;
  let state: "outside" | "mine" | "theirs" = "outside";
  for (const line of text.split("\n")) {
    const kind = conflictMarkerKind(line);
    if (kind === "start") state = "mine";
    else if (kind === "separator" && state === "mine") state = "theirs";
    else if (kind === "end" && state === "theirs") {
      count += 1;
      state = "outside";
    }
  }
  return count;
}

const mineColor = "var(--markra-conflict-mine, #2f81f7)";
const theirsColor = "var(--markra-conflict-theirs, #bf8700)";

const conflictTheme = EditorView.baseTheme({
  ".cm-line.markra-conflict-mine": {
    background: `color-mix(in srgb, ${mineColor} 9%, transparent)`,
    borderLeft: `3px solid color-mix(in srgb, ${mineColor} 60%, transparent)`,
  },
  ".cm-line.markra-conflict-theirs": {
    background: `color-mix(in srgb, ${theirsColor} 10%, transparent)`,
    borderLeft: `3px solid color-mix(in srgb, ${theirsColor} 65%, transparent)`,
  },
  ".cm-line.markra-conflict-base": {
    opacity: "0.6",
    textDecoration: "line-through",
  },
  ".markra-conflict-bar, .markra-conflict-divider, .markra-conflict-panel": {
    fontFamily: "var(--font-ui, system-ui, sans-serif)",
    fontSize: "0.8em",
  },
  ".markra-conflict-bar": {
    alignItems: "center",
    background: `color-mix(in srgb, ${mineColor} 14%, transparent)`,
    borderLeft: `3px solid color-mix(in srgb, ${mineColor} 60%, transparent)`,
    borderRadius: "0.4em 0.4em 0 0",
    display: "flex",
    flexWrap: "wrap",
    gap: "0.4em",
    marginTop: "0.5em",
    padding: "0.3em 0.6em",
  },
  ".markra-conflict-label": {
    color: mineColor,
    fontWeight: "650",
    marginRight: "auto",
  },
  ".markra-conflict-divider": {
    background: `color-mix(in srgb, ${theirsColor} 16%, transparent)`,
    borderLeft: `3px solid color-mix(in srgb, ${theirsColor} 65%, transparent)`,
    color: theirsColor,
    fontWeight: "650",
    minHeight: "0.4em",
    padding: "0.2em 0.6em",
  },
  ".markra-conflict-end": {
    borderBottom: `2px solid color-mix(in srgb, ${theirsColor} 45%, transparent)`,
    borderRadius: "0 0 0.4em 0.4em",
    marginBottom: "0.5em",
  },
  ".markra-conflict-button": {
    background: "var(--bg-primary, transparent)",
    border: "1px solid var(--border-default, color-mix(in srgb, currentColor 25%, transparent))",
    borderRadius: "0.35em",
    color: "var(--text-primary, inherit)",
    cursor: "pointer",
    font: "inherit",
    padding: "0.1em 0.6em",
  },
  ".markra-conflict-button:hover": {
    borderColor: "var(--accent, currentColor)",
  },
  ".markra-conflict-panel": {
    alignItems: "center",
    background: `color-mix(in srgb, ${theirsColor} 12%, var(--bg-primary, transparent))`,
    borderBottom: "1px solid var(--border-default, transparent)",
    display: "flex",
    gap: "0.75em",
    justifyContent: "space-between",
    padding: "0.35em 0.75em",
  },
  ".markra-conflict-count": { fontWeight: "600" },
});

export interface ConflictMarkersPluginOptions {
  enabled?: boolean;
  labels?: Partial<ConflictLabels>;
}

/**
 * Shows each `<<<<<<<` … `>>>>>>>` block as your lines and their lines, with
 * Keep mine / Use theirs / Keep both above it, and a count of the conflicts
 * left above the document. The text keeps its markers until one is chosen.
 */
export function conflictMarkersPlugin(options: ConflictMarkersPluginOptions = {}) {
  const labels = { ...defaultConflictLabels, ...options.labels };
  return defineMarkraPlugin({
    id: "markra.conflicts",
    extension: options.enabled === false ? [] : [conflictLabelsFacet.of(labels), conflictField, conflictTheme],
  });
}
