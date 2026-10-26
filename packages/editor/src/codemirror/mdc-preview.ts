import { syntaxTree } from "@codemirror/language";
import { forEachDiagnostic, setDiagnosticsEffect } from "@codemirror/lint";
import {
  StateField,
  type EditorState,
  type Range,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  WidgetType,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { mdcBlocksForState, type MdcBlock } from "./mdc-blocks.ts";
import { mdcPropsEditorField, openMdcPropsEditor } from "./mdc-edit.ts";
import { defineMarkraPlugin } from "./plugin.ts";
import { cursorInsideRange, selectionChangeAffectsReveal } from "./policy.ts";

export interface MdcPreviewComponentInfo {
  readonly name: string;
  readonly group?: string | null;
  readonly purpose?: string | null;
}

export interface MdcPreviewPluginOptions {
  enabled?: boolean;
  /** Component descriptions keyed by kebab-case name, read on each rebuild. */
  components?: () => readonly MdcPreviewComponentInfo[] | null | undefined;
  /** Called after a chip's Edit button opens the block's props form. */
  onOpenComponent?: (view: EditorView, block: MdcBlock) => unknown;
}

type LineRole = "open" | "props" | "body" | "close";

interface LineMark {
  readonly from: number;
  readonly role: LineRole;
  readonly depth: number;
  readonly first: boolean;
  readonly last: boolean;
}

const MAX_DEPTH = 4;


function blockEnd(block: MdcBlock) {
  return block.close?.to ?? block.open.to;
}

/**
 * One role per line, decided from the document alone: the innermost block
 * that owns the line wins, so nested cards draw inside their parents.
 */
export function readMdcLineMarks(state: EditorState): LineMark[] {
  const marks = new Map<number, LineMark>();
  const { doc } = state;

  for (const block of mdcBlocksForState(state)) {
    const depth = Math.min(block.depth, MAX_DEPTH);
    const end = blockEnd(block);
    const firstLine = doc.lineAt(block.open.from).number;
    const lastLine = doc.lineAt(end).number;
    const propsFirst = block.props ? doc.lineAt(block.props.from).number : -1;
    const propsLast = block.props ? doc.lineAt(block.props.to).number : -1;
    const closeLine = block.close ? doc.lineAt(block.close.from).number : -1;

    for (let number = firstLine; number <= lastLine; number++) {
      const line = doc.line(number);
      const role: LineRole =
        number === firstLine
          ? "open"
          : number >= propsFirst && number <= propsLast
            ? "props"
            : number === closeLine
              ? "close"
              : "body";
      // Blocks arrive outermost first, so inner blocks overwrite.
      marks.set(line.from, {
        depth,
        first: number === firstLine,
        from: line.from,
        last: number === lastLine,
        role,
      });
    }
  }

  return [...marks.values()].sort((a, b) => a.from - b.from);
}

// A line-level YAML highlighter. Props blocks are small and flat enough that
// keys, scalars, comments and list dashes cover what a reader needs to see.
const YAML_LINE =
  /^(\s*)(- )?(?:((?:"[^"]*"|'[^']*'|[^\s#:'"][^:#]*?)\s*):(?=\s|$))?(\s*)(.*)$/u;

export function yamlLineTokens(text: string) {
  const tokens: { from: number; to: number; kind: string }[] = [];
  const match = YAML_LINE.exec(text);
  if (!match) return tokens;
  let pos = match[1]!.length;
  if (match[2]) {
    tokens.push({ from: pos, to: pos + 1, kind: "dash" });
    pos += 2;
  }
  if (match[3] !== undefined) {
    tokens.push({ from: pos, to: pos + match[3].length, kind: "key" });
    pos = text.indexOf(":", pos + match[3].length) + 1;
  }
  pos += match[4]!.length;
  const rest = match[5]!;
  if (!rest) return tokens;
  if (rest.startsWith("#")) {
    tokens.push({ from: pos, to: text.length, kind: "comment" });
    return tokens;
  }
  const comment = /\s#/u.exec(rest);
  const value = (comment ? rest.slice(0, comment.index) : rest).trimEnd();
  const kind = /^["']/u.test(value)
    ? "string"
    : /^(true|false|null|~|-?\d[\d_.]*(e[+-]?\d+)?)$/iu.test(value)
      ? "literal"
      : /^[|>][-+]?\d*$/u.test(value) || /^[[{]/u.test(value)
        ? "punctuation"
        : "string";
  if (value) tokens.push({ from: pos, to: pos + value.length, kind });
  if (comment) tokens.push({ from: pos + comment.index + 1, to: text.length, kind: "comment" });
  return tokens;
}

function buildLineDecorations(state: EditorState) {
  const ranges: Range<Decoration>[] = [];
  for (const mark of readMdcLineMarks(state)) {
    const classes = [
      "cm-markra-mdc",
      `markra-mdc-${mark.role}`,
      `markra-mdc-depth-${mark.depth}`,
      mark.first ? "markra-mdc-first" : "",
      mark.last ? "markra-mdc-last" : "",
    ].filter(Boolean).join(" ");
    ranges.push(Decoration.line({ class: classes }).range(mark.from));

    if (mark.role !== "props") continue;
    const line = state.doc.lineAt(mark.from);
    if (line.text.trim() === "---") continue;
    for (const token of yamlLineTokens(line.text)) {
      ranges.push(
        Decoration.mark({ class: `markra-mdc-yaml-${token.kind}` }).range(
          line.from + token.from,
          line.from + token.to,
        ),
      );
    }
  }
  return Decoration.set(ranges, true);
}

class MdcSpacerWidget extends WidgetType {
  constructor(readonly edge: "before" | "after") {
    super();
  }

  eq(other: MdcSpacerWidget) {
    return other.edge === this.edge;
  }

  toDOM(view: EditorView) {
    const spacer = view.dom.ownerDocument.createElement("div");
    spacer.className = `markra-mdc-spacer markra-mdc-spacer-${this.edge}`;
    spacer.setAttribute("aria-hidden", "true");
    return spacer;
  }
}

function buildSpacingDecorations(state: EditorState) {
  const ranges: Range<Decoration>[] = [];
  for (const block of mdcBlocksForState(state)) {
    if (block.depth > 0) continue;
    // Block widgets, not .cm-line margins, so CodeMirror's height map stays
    // in step with what is drawn.
    ranges.push(
      Decoration.widget({ block: true, side: -1, widget: new MdcSpacerWidget("before") })
        .range(state.doc.lineAt(block.open.from).from),
      Decoration.widget({ block: true, side: 1, widget: new MdcSpacerWidget("after") })
        .range(state.doc.lineAt(blockEnd(block)).to),
    );
  }
  return Decoration.set(ranges, true);
}

const mdcStructureField = StateField.define<{ lines: DecorationSet; spacing: DecorationSet }>({
  create: (state) => ({
    lines: buildLineDecorations(state),
    spacing: buildSpacingDecorations(state),
  }),
  update(value, transaction) {
    // The tree can finish parsing without a doc change, so compare trees too.
    if (!transaction.docChanged && syntaxTree(transaction.startState) === syntaxTree(transaction.state)) {
      return value;
    }
    return {
      lines: buildLineDecorations(transaction.state),
      spacing: buildSpacingDecorations(transaction.state),
    };
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value) => value.lines),
    EditorView.decorations.from(field, (value) => value.spacing),
  ],
});

function humanName(name: string) {
  const words = name.replace(/([a-z])([A-Z])/gu, "$1 $2").split(/[-_\s]+/u).filter(Boolean);
  const text = words.join(" ").toLowerCase();
  return text.charAt(0).toUpperCase() + text.slice(1);
}

class MdcChipWidget extends WidgetType {
  constructor(
    readonly block: MdcBlock,
    readonly label: string,
    readonly attributes: string,
    readonly purpose: string,
    readonly known: boolean,
    readonly onOpen: MdcPreviewPluginOptions["onOpenComponent"],
    /** Lint messages for the opener line, which this chip hides. */
    readonly problems: string,
  ) {
    super();
  }

  eq(other: MdcChipWidget) {
    return (
      other.block.open.from === this.block.open.from &&
      other.label === this.label &&
      other.attributes === this.attributes &&
      other.purpose === this.purpose &&
      other.known === this.known &&
      other.problems === this.problems
    );
  }

  ignoreEvent(event: Event) {
    return event.type !== "mousedown";
  }

  toDOM(view: EditorView) {
    const document = view.dom.ownerDocument;
    const chip = document.createElement("span");
    chip.className = `markra-mdc-chip${this.known ? "" : " markra-mdc-chip-unknown"}${
      this.problems ? " markra-mdc-chip-problem" : ""
    }`;
    chip.title = this.problems || (this.known ? this.purpose : `Unknown component "${this.block.name}"`);
    chip.dataset.mdcName = this.block.name;

    const label = document.createElement("span");
    label.className = "markra-mdc-chip-label";
    label.textContent = this.label;
    chip.append(label);

    if (this.attributes) {
      const attrs = document.createElement("span");
      attrs.className = "markra-mdc-chip-attributes";
      attrs.textContent = this.attributes;
      chip.append(attrs);
    }

    {
      const edit = document.createElement("button");
      edit.type = "button";
      edit.className = "markra-mdc-chip-edit";
      edit.textContent = "Edit";
      edit.disabled = view.state.readOnly;
      edit.addEventListener("mousedown", (event) => {
        event.preventDefault();
        event.stopPropagation();
      });
      edit.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        view.dispatch({ effects: openMdcPropsEditor.of(this.block.open.from) });
        this.onOpen?.(view, this.block);
      });
      chip.append(edit);
    }
    return chip;
  }
}

class MdcCloseWidget extends WidgetType {
  constructor(readonly label: string) {
    super();
  }

  eq(other: MdcCloseWidget) {
    return other.label === this.label;
  }

  toDOM(view: EditorView) {
    const end = view.dom.ownerDocument.createElement("span");
    end.className = "markra-mdc-close-label";
    end.textContent = `End ${this.label.toLowerCase()}`;
    return end;
  }
}

function openerProblems(state: EditorState) {
  const problems = new Map<number, string[]>();
  forEachDiagnostic(state, (diagnostic, from) => {
    const line = state.doc.lineAt(from);
    problems.set(line.from, [...(problems.get(line.from) ?? []), diagnostic.message]);
  });
  return problems;
}

function buildRevealDecorations(view: EditorView, options: MdcPreviewPluginOptions) {
  const ranges: Range<Decoration>[] = [];
  const problems = openerProblems(view.state);
  const components = new Map(
    (options.components?.() ?? []).map((component) => [component.name, component]),
  );

  for (const block of mdcBlocksForState(view.state)) {
    const { from, to } = block.openLine;
    if (blockEnd(block) < view.viewport.from) continue;
    if (from > view.viewport.to) break;

    const info = components.get(block.name);
    const label = humanName(block.name);
    if (!cursorInsideRange(view, from, to)) {
      const attributes = block.attributes
        ? view.state.sliceDoc(block.attributes.from + 1, block.attributes.to - 1).trim()
        : "";
      ranges.push(
        Decoration.replace({
          widget: new MdcChipWidget(
            block,
            label,
            attributes,
            info?.purpose ?? "",
            components.size === 0 || Boolean(info),
            options.onOpenComponent,
            (problems.get(view.state.doc.lineAt(from).from) ?? []).join("\n"),
          ),
        }).range(from, to),
      );
    }
    if (block.close && !cursorInsideRange(view, block.close.from, block.close.to)) {
      ranges.push(
        Decoration.replace({ widget: new MdcCloseWidget(label) }).range(
          block.close.from,
          block.close.to,
        ),
      );
    }
  }
  return Decoration.set(ranges, true);
}

const depthColors = ["#4f7cac", "#299764", "#8250df", "#9a6700", "#cf222e"];

const mdcTheme = EditorView.baseTheme({
  ".cm-line.cm-markra-mdc": {
    background: "color-mix(in srgb, var(--mdc-color) 5%, transparent)",
    borderLeft: "2px solid color-mix(in srgb, var(--mdc-color) 45%, transparent)",
    paddingLeft: "calc(0.75em + var(--mdc-indent, 0em))",
  },
  ...Object.fromEntries(
    depthColors.map((color, depth) => [
      `.cm-line.markra-mdc-depth-${depth}`,
      { "--mdc-color": color, "--mdc-indent": `${depth * 0.75}em` },
    ]),
  ),
  // Markra's block spacing is drawn by spacer widgets between lines; paint
  // the ones inside a card so it reads as one box. Colour only, no geometry.
  ...Object.fromEntries(
    depthColors.map((color, depth) => [
      [
        `.cm-line.markra-mdc-depth-${depth}:not(.markra-mdc-last) + .cm-markra-paragraph-spacer`,
        `.cm-line.markra-mdc-depth-${depth}:not(.markra-mdc-last) + .cm-markra-heading-spacer`,
      ].join(", "),
      {
        background: `color-mix(in srgb, ${color} 5%, transparent)`,
        borderLeft: `2px solid color-mix(in srgb, ${color} 45%, transparent)`,
      },
    ]),
  ),
  ".cm-line.markra-mdc-props": {
    fontFamily: "var(--markra-code-font, ui-monospace, SFMono-Regular, Menlo, monospace)",
    fontSize: "0.88em",
  },
  ".markra-mdc-yaml-key": { color: "var(--markra-mdc-yaml-key, #0550ae)" },
  ".markra-mdc-yaml-string": { color: "var(--markra-mdc-yaml-string, #0a3069)" },
  ".markra-mdc-yaml-literal": { color: "var(--markra-mdc-yaml-literal, #8250df)" },
  ".markra-mdc-yaml-comment": { color: "var(--markra-mdc-yaml-comment, #6e7781)", fontStyle: "italic" },
  ".markra-mdc-yaml-dash, .markra-mdc-yaml-punctuation": { color: "var(--markra-mdc-yaml-punctuation, #6e7781)" },
  "&dark .markra-mdc-yaml-key": { color: "var(--markra-mdc-yaml-key, #79c0ff)" },
  "&dark .markra-mdc-yaml-string": { color: "var(--markra-mdc-yaml-string, #a5d6ff)" },
  "&dark .markra-mdc-yaml-literal": { color: "var(--markra-mdc-yaml-literal, #d2a8ff)" },
  "&dark .markra-mdc-yaml-comment, &dark .markra-mdc-yaml-dash, &dark .markra-mdc-yaml-punctuation": {
    color: "var(--markra-mdc-yaml-comment, #8b949e)",
  },
  ".markra-mdc-chip": {
    alignItems: "baseline",
    color: "var(--mdc-color)",
    display: "inline-flex",
    flexWrap: "wrap",
    gap: "0.5em",
  },
  ".markra-mdc-chip-label": { fontWeight: "650" },
  ".markra-mdc-chip-unknown .markra-mdc-chip-label, .markra-mdc-chip-problem .markra-mdc-chip-label": {
    textDecoration: "underline wavy #cf222e",
  },
  ".markra-mdc-chip-attributes": {
    color: "var(--markra-muted-text, #6e7781)",
    fontFamily: "var(--markra-code-font, ui-monospace, SFMono-Regular, Menlo, monospace)",
    fontSize: "0.85em",
  },
  ".markra-mdc-chip-edit": {
    background: "transparent",
    border: "1px solid color-mix(in srgb, var(--mdc-color) 35%, transparent)",
    borderRadius: "0.35em",
    color: "inherit",
    cursor: "pointer",
    font: "inherit",
    fontSize: "0.8em",
    padding: "0 0.5em",
  },
  ".markra-mdc-close-label": {
    color: "color-mix(in srgb, var(--mdc-color) 60%, transparent)",
    fontSize: "0.8em",
  },
  ".markra-mdc-spacer": { height: "0.75em" },
});

export function mdcPreviewPlugin(options: MdcPreviewPluginOptions = {}) {
  const enabled = options.enabled ?? true;
  return defineMarkraPlugin({
    id: "markra.mdc-preview",
    extension: enabled
      ? [
          mdcStructureField,
          mdcPropsEditorField,
          ViewPlugin.fromClass(
            class {
              decorations: DecorationSet;

              constructor(view: EditorView) {
                this.decorations = buildRevealDecorations(view, options);
              }

              update(update: ViewUpdate) {
                if (
                  update.docChanged ||
                  syntaxTree(update.startState) !== syntaxTree(update.state) ||
                  selectionChangeAffectsReveal(update) ||
                  update.focusChanged ||
                  update.viewportChanged ||
                  update.transactions.some((transaction) =>
                    transaction.effects.some((effect) => effect.is(setDiagnosticsEffect)))
                ) {
                  this.decorations = buildRevealDecorations(update.view, options);
                }
              }
            },
            { decorations: (plugin) => plugin.decorations },
          ),
          mdcTheme,
        ]
      : [],
  });
}
