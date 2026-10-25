import { syntaxTree } from "@codemirror/language";
import type { EditorState, Text } from "@codemirror/state";
import type { SyntaxNode, Tree } from "@lezer/common";

export interface MdcRange {
  from: number;
  to: number;
}

export interface MdcBlock {
  name: string;
  /** Colon count of the opener; the closer must match it. */
  colons: number;
  /** Nesting depth among MDC blocks, 0 for top level. */
  depth: number;
  /** The opener node, including its YAML props block. */
  open: MdcRange;
  /** The opener's first line (`::name{...}`). */
  openLine: MdcRange;
  nameRange: MdcRange;
  attributes: MdcRange | null;
  /** The YAML props block, `---` to `---`. */
  props: MdcRange | null;
  /** The YAML between the fences; null when the block is empty. */
  propsContent: MdcRange | null;
  /** Whether the props block has its closing `---`. */
  propsClosed: boolean;
  close: MdcRange | null;
}

function range(node: SyntaxNode | null): MdcRange | null {
  return node ? { from: node.from, to: node.to } : null;
}

function colonCount(node: SyntaxNode) {
  const mark = node.getChild("MdcBlockMark");
  return mark ? mark.to - mark.from : 0;
}

/**
 * Pairs MDC openers with closers in document order. A closer ends the
 * innermost open block with the same colon count, implicitly closing any
 * blocks opened after it; unmatched closers are ignored.
 */
export function readMdcBlocksFromTree(tree: Tree, doc: Text): MdcBlock[] {
  const blocks: MdcBlock[] = [];
  const open: MdcBlock[] = [];

  tree.iterate({
    enter(ref) {
      if (ref.name === "MdcBlockOpen") {
        const node = ref.node;
        const name = node.getChild("MdcName");
        const props = node.getChild("MdcProps");
        const fences = props?.getChildren("MdcPropsMark") ?? [];
        const firstLine = doc.lineAt(node.from);
        const block: MdcBlock = {
          name: name ? doc.sliceString(name.from, name.to) : "",
          colons: colonCount(node),
          depth: open.length,
          open: { from: node.from, to: node.to },
          openLine: { from: node.from, to: Math.min(firstLine.to, node.to) },
          nameRange: range(name) ?? { from: node.from, to: node.from },
          attributes: range(node.getChild("MdcAttributes")),
          props: range(props),
          propsContent: range(props?.getChild("MdcPropsContent") ?? null),
          propsClosed: fences.length >= 2,
          close: null,
        };
        blocks.push(block);
        open.push(block);
        return false;
      }
      if (ref.name === "MdcBlockClose") {
        const colons = colonCount(ref.node);
        for (let i = open.length - 1; i >= 0; i--) {
          if (open[i]!.colons !== colons) continue;
          open[i]!.close = { from: ref.from, to: ref.to };
          open.length = i;
          break;
        }
        return false;
      }
      return undefined;
    },
  });

  return blocks;
}

export function readMdcBlocks(state: EditorState): MdcBlock[] {
  return readMdcBlocksFromTree(syntaxTree(state), state.doc);
}

const blocksCache = new WeakMap<EditorState, MdcBlock[]>();

/** `readMdcBlocks`, cached per state for the plugins that share it. */
export function mdcBlocksForState(state: EditorState): MdcBlock[] {
  let blocks = blocksCache.get(state);
  if (!blocks) {
    blocks = readMdcBlocks(state);
    blocksCache.set(state, blocks);
  }
  return blocks;
}

/** The innermost block whose opener..closer span contains `pos`. */
export function mdcBlockAt(blocks: readonly MdcBlock[], pos: number): MdcBlock | null {
  let found: MdcBlock | null = null;
  for (const block of blocks) {
    const end = block.close?.to ?? block.open.to;
    if (block.open.from <= pos && pos <= end && (!found || block.depth > found.depth)) {
      found = block;
    }
  }
  return found;
}
