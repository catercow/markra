import type { BlockContext, Element, InlineContext, Line, MarkdownConfig } from "@lezer/markdown";
import { tags } from "@lezer/highlight";

// MDC (Nuxt Content / comark) syntax:
//   ::name{attrs}        block opener, more colons on outer blocks
//   ---                  optional YAML props, only directly after the opener
//   ...                  Markdown content, parsed normally
//   #slot{attrs}         named slot inside a block
//   ::                   closer with the opener's colon count
//   :name[label]{attrs}  inline component; {attrs} also follows spans,
//                        emphasis, code and links
// MDC lines may be indented any amount (nested components often are).
// Openers and closers are separate leaf nodes so the content between them
// keeps Markdown's own parsing and incremental reuse; `readMdcBlocks` pairs
// them.

const COLON = 58;
const HASH = 35;
const BRACE_OPEN = 123;
const BRACKET_OPEN = 91;

const NAME_START = /[A-Za-z]/u;
const NAME_CHAR = /[\w-]/u;

/** Index just past a balanced `{...}` starting at `start`, or -1. */
export function scanMdcAttributes(text: string, start: number): number {
  if (text.charCodeAt(start) !== BRACE_OPEN) return -1;
  let quote = 0;
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    const ch = text.charCodeAt(i);
    if (quote) {
      if (ch === 92) i++;
      else if (ch === quote) quote = 0;
    } else if (ch === 34 || ch === 39) {
      quote = ch;
    } else if (ch === BRACE_OPEN) {
      depth++;
    } else if (ch === 125) {
      depth--;
      if (depth === 0) return i + 1;
    } else if (ch === 10) {
      return -1;
    }
  }
  return -1;
}

function scanName(text: string, start: number) {
  if (!NAME_START.test(text.charAt(start))) return -1;
  let i = start + 1;
  while (i < text.length && NAME_CHAR.test(text.charAt(i))) i++;
  return i;
}

function scanBracket(text: string, start: number) {
  if (text.charCodeAt(start) !== BRACKET_OPEN) return -1;
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    const ch = text.charCodeAt(i);
    if (ch === 92) i++;
    else if (ch === BRACKET_OPEN) depth++;
    else if (ch === 93 && --depth === 0) return i + 1;
  }
  return -1;
}

function isBlank(text: string, from: number) {
  return /^\s*$/u.test(text.slice(from));
}

function colonRun(text: string, start: number) {
  let i = start;
  while (text.charCodeAt(i) === COLON) i++;
  return i - start;
}

interface OpenerMatch {
  colons: number;
  nameEnd: number;
  attrsFrom: number;
  attrsEnd: number;
}

function matchOpener(line: Line): OpenerMatch | null {
  if (line.next !== COLON) return null;
  const colons = colonRun(line.text, line.pos);
  if (colons < 2) return null;
  const nameEnd = scanName(line.text, line.pos + colons);
  if (nameEnd < 0) return null;
  let end = nameEnd;
  const labelEnd = scanBracket(line.text, end);
  if (labelEnd > 0) end = labelEnd;
  const attrsFrom = end;
  const attrsEnd = scanMdcAttributes(line.text, end);
  if (attrsEnd > 0) end = attrsEnd;
  if (!isBlank(line.text, end)) return null;
  return { colons, nameEnd, attrsFrom, attrsEnd: attrsEnd > 0 ? attrsEnd : -1 };
}

function isCloser(line: Line) {
  if (line.next !== COLON) return false;
  const colons = colonRun(line.text, line.pos);
  return colons >= 2 && isBlank(line.text, line.pos + colons);
}

function matchSlot(line: Line) {
  if (line.next !== HASH) return -1;
  const nameEnd = scanName(line.text, line.pos + 1);
  if (nameEnd < 0) return -1;
  const attrsEnd = scanMdcAttributes(line.text, nameEnd);
  const end = attrsEnd > 0 ? attrsEnd : nameEnd;
  return isBlank(line.text, end) ? end : -1;
}

function isPropsFence(text: string) {
  return text.trim() === "---";
}

// Lezer keeps container depth off its public types; FencedCode uses the same
// check to stop at the end of a list item or blockquote.
function stillInside(cx: BlockContext, line: Line) {
  const depth = (line as unknown as { depth?: number }).depth;
  const stack = (cx as unknown as { stack?: unknown[] }).stack;
  return depth === undefined || stack === undefined || depth >= stack.length;
}

function parseOpener(cx: BlockContext, line: Line) {
  const match = matchOpener(line);
  if (!match) return false;

  const start = cx.lineStart;
  const from = start + line.pos;
  const children: Element[] = [
    cx.elt("MdcBlockMark", from, from + match.colons),
    cx.elt("MdcName", from + match.colons, start + match.nameEnd),
  ];
  if (match.attrsEnd > 0) {
    children.push(cx.elt("MdcAttributes", start + match.attrsFrom, start + match.attrsEnd));
  }
  let to = start + line.text.trimEnd().length;

  if (isPropsFence(cx.peekLine())) {
    cx.nextLine();
    const propsFrom = cx.lineStart + line.pos;
    const marks: Element[] = [
      cx.elt("MdcPropsMark", propsFrom, propsFrom + 3),
    ];
    let contentFrom = -1;
    let contentTo = -1;
    let closed = false;
    while (cx.nextLine() && stillInside(cx, line)) {
      if (isPropsFence(line.text.slice(line.basePos))) {
        marks.push(cx.elt("MdcPropsMark", cx.lineStart + line.pos, cx.lineStart + line.pos + 3));
        closed = true;
        break;
      }
      if (isCloser(line)) break;
      if (contentFrom < 0) contentFrom = cx.lineStart;
      contentTo = cx.lineStart + line.text.length;
    }
    if (contentFrom >= 0) {
      marks.splice(1, 0, cx.elt("MdcPropsContent", contentFrom, contentTo));
    }
    const propsTo = closed ? cx.lineStart + line.text.trimEnd().length : Math.max(contentTo, propsFrom + 3);
    children.push(cx.elt("MdcProps", propsFrom, propsTo, marks));
    to = propsTo;
    if (closed) cx.nextLine();
  } else {
    cx.nextLine();
  }

  cx.addElement(cx.elt("MdcBlockOpen", from, to, children));
  return true;
}

// With indented code removed, a fence indented 4+ columns (inside a nested
// component) opens in Lezer but its closing fence is never recognized, so
// the code would run to the end of the document. Parse those fences here.
function parseDeepFence(cx: BlockContext, line: Line) {
  if (line.indent - line.baseIndent < 4) return false;
  const fence = /^(`{3,}|~{3,})(.*)$/u.exec(line.text.slice(line.pos));
  if (!fence || (fence[1]!.startsWith("`") && fence[2]!.includes("`"))) return false;
  const open = fence[1]!;
  const from = cx.lineStart + line.pos;
  const children: Element[] = [cx.elt("CodeMark", from, from + open.length)];
  const info = fence[2]!.trim();
  if (info) {
    const infoFrom = from + open.length + fence[2]!.indexOf(info);
    children.push(cx.elt("CodeInfo", infoFrom, infoFrom + info.length));
  }
  let to = from + line.text.slice(line.pos).trimEnd().length;
  while (cx.nextLine() && stillInside(cx, line)) {
    const text = line.text.slice(line.pos);
    const close = /^(`{3,}|~{3,})\s*$/u.exec(text);
    if (close && close[1]![0] === open[0] && close[1]!.length >= open.length) {
      const markFrom = cx.lineStart + line.pos;
      children.push(cx.elt("CodeMark", markFrom, markFrom + close[1]!.length));
      to = markFrom + close[1]!.length;
      cx.nextLine();
      break;
    }
    if (line.basePos < line.text.length) {
      children.push(cx.elt("CodeText", cx.lineStart + line.basePos, cx.lineStart + line.text.length));
    }
    to = cx.lineStart + line.text.length;
  }
  cx.addElement(cx.elt("FencedCode", from, to, children));
  return true;
}

export const markraMdc: MarkdownConfig = {
  // Nested components are indented freely and MDC renderers dedent them, so
  // a 4-space line inside a component is content, not code. Fenced code
  // still works.
  remove: ["IndentedCode"],
  defineNodes: [
    { name: "MdcBlockOpen", block: true },
    { name: "MdcBlockClose", block: true },
    { name: "MdcSlot", block: true },
    { name: "MdcProps" },
    { name: "MdcPropsContent" },
    { name: "MdcPropsMark", style: tags.processingInstruction },
    { name: "MdcBlockMark", style: tags.processingInstruction },
    { name: "MdcSlotMark", style: tags.processingInstruction },
    { name: "MdcInlineMark", style: tags.processingInstruction },
    { name: "MdcName", style: tags.tagName },
    { name: "MdcLabel" },
    { name: "MdcAttributes", style: tags.attributeValue },
    { name: "MdcInline" },
  ],
  parseBlock: [
    { name: "MdcDeepFence", before: "FencedCode", parse: parseDeepFence },
    {
      name: "MdcBlockOpen",
      before: "FencedCode",
      parse: parseOpener,
      endLeaf: (_cx, line) => matchOpener(line) !== null || isCloser(line),
      // Lezer skips endLeaf for lines indented 4+ columns, but nested MDC
      // lines are often indented that far. End the paragraph here instead;
      // the unconsumed line is then parsed as the next block.
      leaf: () => ({
        nextLine(cx, line, leaf) {
          if (line.indent < line.baseIndent + 4) return false;
          if (matchOpener(line) === null && !isCloser(line)) return false;
          cx.addLeafElement(
            leaf,
            cx.elt(
              "Paragraph",
              leaf.start,
              leaf.start + leaf.content.length,
              cx.parser.parseInline(leaf.content, leaf.start),
            ),
          );
          return true;
        },
        finish: () => false,
      }),
    },
    {
      name: "MdcBlockClose",
      before: "FencedCode",
      parse(cx, line) {
        if (!isCloser(line)) return false;
        const from = cx.lineStart + line.pos;
        const colons = colonRun(line.text, line.pos);
        cx.addElement(
          cx.elt("MdcBlockClose", from, from + colons, [
            cx.elt("MdcBlockMark", from, from + colons),
          ]),
        );
        cx.nextLine();
        return true;
      },
    },
    {
      name: "MdcSlot",
      before: "ATXHeading",
      parse(cx, line) {
        const end = matchSlot(line);
        if (end < 0) return false;
        const start = cx.lineStart;
        const from = start + line.pos;
        const nameEnd = scanName(line.text, line.pos + 1);
        const children = [
          cx.elt("MdcSlotMark", from, from + 1),
          cx.elt("MdcName", from + 1, start + nameEnd),
        ];
        if (end > nameEnd) children.push(cx.elt("MdcAttributes", start + nameEnd, start + end));
        cx.addElement(cx.elt("MdcSlot", from, start + end, children));
        cx.nextLine();
        return true;
      },
      endLeaf: (_cx, line) => matchSlot(line) >= 0,
    },
  ],
  parseInline: [
    {
      name: "MdcInline",
      before: "Emphasis",
      parse(cx, next, pos) {
        if (next !== COLON) return -1;
        const prev = cx.char(pos - 1);
        // `a:b` and `::` are not components; neither are URLs like `https:x`.
        if (prev === COLON || (prev >= 0 && /[\w/]/u.test(String.fromCharCode(prev)))) return -1;
        const text = cx.slice(pos, cx.end);
        const nameEnd = scanName(text, 1);
        if (nameEnd < 0) return -1;
        let end = nameEnd;
        const children = [
          cx.elt("MdcInlineMark", pos, pos + 1),
          cx.elt("MdcName", pos + 1, pos + nameEnd),
        ];
        const labelEnd = scanBracket(text, end);
        if (labelEnd > 0) {
          children.push(cx.elt("MdcLabel", pos + end, pos + labelEnd));
          end = labelEnd;
        }
        const attrsEnd = scanMdcAttributes(text, end);
        if (attrsEnd > 0) {
          children.push(cx.elt("MdcAttributes", pos + end, pos + attrsEnd));
          end = attrsEnd;
        }
        // A bare `:word` is too common in prose ("Note:", times, emoji
        // shortcodes) to claim without a label or attributes.
        if (end === nameEnd) return -1;
        return cx.addElement(cx.elt("MdcInline", pos, pos + end, children));
      },
    },
    {
      name: "MdcAttributes",
      before: "Emphasis",
      parse(cx: InlineContext, next, pos) {
        if (next !== BRACE_OPEN) return -1;
        const prev = String.fromCharCode(cx.char(pos - 1));
        if (!"]`*_~)".includes(prev) || pos === cx.offset) return -1;
        const end = scanMdcAttributes(cx.slice(pos, cx.end), 0);
        if (end < 0) return -1;
        return cx.addElement(cx.elt("MdcAttributes", pos, pos + end));
      },
    },
  ],
};
