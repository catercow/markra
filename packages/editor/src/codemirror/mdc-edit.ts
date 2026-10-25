import {
  StateEffect,
  StateField,
  type ChangeSpec,
  type EditorState,
} from "@codemirror/state";
import { mdcBlockAt, mdcBlocksForState, type MdcBlock } from "./mdc-blocks.ts";

/** The text a props form reads: inside the braces, and between the fences. */
export interface MdcPropsText {
  attributes: string | null;
  yaml: string | null;
}

export function readMdcPropsText(state: EditorState, block: MdcBlock): MdcPropsText {
  return {
    attributes: block.attributes
      ? state.sliceDoc(block.attributes.from + 1, block.attributes.to - 1)
      : null,
    yaml: block.propsContent
      ? state.sliceDoc(block.propsContent.from, block.propsContent.to)
      : block.props
        ? ""
        : null,
  };
}

function indentOf(state: EditorState, pos: number) {
  const line = state.doc.lineAt(pos);
  return /^[ \t]*/u.exec(line.text)![0];
}

/**
 * The smallest edits that turn a block's current props text into `next`.
 * `null` removes the braces or the whole `---` block.
 */
export function mdcPropsChanges(
  state: EditorState,
  block: MdcBlock,
  next: MdcPropsText,
): ChangeSpec[] {
  const changes: ChangeSpec[] = [];
  const current = readMdcPropsText(state, block);

  if (next.attributes !== current.attributes) {
    if (block.attributes) {
      changes.push(
        next.attributes === null
          ? { from: block.attributes.from, to: block.attributes.to }
          : { from: block.attributes.from + 1, to: block.attributes.to - 1, insert: next.attributes },
      );
    } else if (next.attributes !== null) {
      // After any [label]: the attributes come last on the opener line.
      const lineEnd = state.sliceDoc(block.nameRange.to, block.openLine.to).trimEnd().length;
      changes.push({ from: block.nameRange.to + lineEnd, insert: `{${next.attributes}}` });
    }
  }

  if (next.yaml !== current.yaml) {
    const indent = indentOf(state, block.open.from);
    if (next.yaml === null) {
      if (block.props) changes.push({ from: block.openLine.to, to: block.props.to });
    } else if (block.propsContent) {
      changes.push({ from: block.propsContent.from, to: block.propsContent.to, insert: next.yaml });
    } else if (block.props) {
      // An empty `---\n---` block: put the YAML after the first fence line.
      const fenceEnd = state.doc.lineAt(block.props.from).to;
      changes.push({ from: fenceEnd, insert: `\n${next.yaml}` });
    } else {
      changes.push({ from: block.openLine.to, insert: `\n${indent}---\n${next.yaml}\n${indent}---` });
    }
  }

  return changes;
}

export const openMdcPropsEditor = StateEffect.define<number | null>();

/**
 * Which block's props form is open, as the position of its opener. Mapped
 * through edits so typing elsewhere keeps the same block selected.
 */
export const mdcPropsEditorField = StateField.define<number | null>({
  create: () => null,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(openMdcPropsEditor)) value = effect.value;
    }
    if (value === null || !transaction.docChanged) return value;
    const mapped = transaction.changes.mapPos(value, 1);
    return mdcBlocksForState(transaction.state).some((block) => block.open.from === mapped)
      ? mapped
      : null;
  },
});

/** The block whose props form is open, read fresh from the current tree. */
export function openMdcBlock(state: EditorState): MdcBlock | null {
  const from = state.field(mdcPropsEditorField, false);
  if (from === null || from === undefined) return null;
  return mdcBlocksForState(state).find((block) => block.open.from === from) ?? null;
}

/** The innermost block around the main selection, if any. */
export function mdcBlockAtSelection(state: EditorState): MdcBlock | null {
  return mdcBlockAt(mdcBlocksForState(state), state.selection.main.head);
}
