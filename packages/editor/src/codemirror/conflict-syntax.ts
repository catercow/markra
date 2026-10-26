import type { BlockContext, Line, MarkdownConfig } from "@lezer/markdown";

// Git conflict markers, as written by CowMark's merge and by git itself:
//   <<<<<<< yours
//   ...                  your lines
//   ||||||| base         (optional, git's diff3 style)
//   =======
//   ...                  their lines
//   >>>>>>> theirs
// Each marker line is its own leaf block so the lines around it keep their
// own Markdown parsing: without this, `=======` underlines the last line of
// yours as a heading and `>>>>>>>` opens a blockquote that swallows the next
// paragraph.

export type ConflictMarkerKind = "start" | "base" | "separator" | "end";

const LT = 60;
const PIPE = 124;
const EQ = 61;
const GT = 62;

/** The marker a line holds (trailing `\r` ignored), or null. */
export function conflictMarkerKind(text: string): ConflictMarkerKind | null {
  const first = text.charCodeAt(0);
  if (first !== LT && first !== PIPE && first !== EQ && first !== GT) return null;
  const line = text.endsWith("\r") ? text.slice(0, -1) : text;
  if (/^<{7}(?:[ \t].*)?$/u.test(line)) return "start";
  if (/^\|{7}(?:[ \t].*)?$/u.test(line)) return "base";
  if (/^={7}[ \t]*$/u.test(line)) return "separator";
  if (/^>{7}(?:[ \t].*)?$/u.test(line)) return "end";
  return null;
}

const lookBehind = 64 * 1024;

/**
 * A lone `=======` is a valid heading underline, so it is a separator only
 * after an unclosed `<<<<<<<`. Lezer keeps the document input on the block
 * context without typing it; without it, separators stay ordinary text.
 */
function insideConflict(cx: BlockContext) {
  const input = (cx as unknown as { input?: { read(from: number, to: number): string } }).input;
  if (!input) return false;
  const before = input.read(Math.max(0, cx.lineStart - lookBehind), cx.lineStart);
  const lines = before.split("\n");
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const kind = conflictMarkerKind(lines[index]);
    if (kind === "start" || kind === "base") return true;
    if (kind) return false;
  }
  return false;
}

function claims(cx: BlockContext, line: Line) {
  if (line.pos !== 0) return false;
  const kind = conflictMarkerKind(line.text);
  return kind === "separator" ? insideConflict(cx) : kind !== null;
}

export const markraConflictSyntax: MarkdownConfig = {
  defineNodes: [{ name: "ConflictMarker", block: true }],
  parseBlock: [
    {
      name: "ConflictMarker",
      before: "Blockquote",
      parse(cx, line) {
        if (!claims(cx, line)) return false;
        cx.addElement(cx.elt("ConflictMarker", cx.lineStart, cx.lineStart + line.text.length));
        cx.nextLine();
        return true;
      },
      endLeaf: (cx, line) => claims(cx, line),
    },
  ],
};
