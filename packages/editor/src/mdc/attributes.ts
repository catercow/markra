// Inline attributes of an MDC component opener: the text between `{` and `}` in
// `::name{title="x" compact :items='["a"]' #id .class}`.

export type MdcAttributeKind = "string" | "bare" | "bound" | "id" | "class";
export type MdcAttributeQuote = '"' | "'" | "";

export interface MdcAttribute {
  /** Attribute name without its `:`, `#` or `.` sigil; `#x` is key `id`, `.x` is key `class`. */
  key: string;
  /**
   * `string`: the unescaped value. `bare`: `true`. `bound`: the value's source text, still
   * to be parsed as JSON/YAML. `id` / `class`: the name after the sigil.
   */
  value: string | true;
  kind: MdcAttributeKind;
  /** Quote around the value in source; `""` when unquoted or valueless. */
  quote: MdcAttributeQuote;
  /** Whitespace between the previous attribute (or the `{`) and this one. */
  leading: string;
  /**
   * Original source text of this attribute. While present, serialization emits it verbatim,
   * so drop it (or build a new entry without it) when changing key, value, kind or quote.
   */
  raw?: string;
  /** Offsets of `raw` in the parsed source; absent on entries that were not parsed. */
  from?: number;
  to?: number;
}

export interface MdcAttributeList {
  attributes: MdcAttribute[];
  /** Whitespace after the last attribute. */
  trailing: string;
}

const whitespace = /\s/;
const unquotedValue = /^[^\s"'`=<>{}]+$/;

export function parseMdcAttributes(src: string): MdcAttributeList {
  const attributes: MdcAttribute[] = [];
  let pos = 0;

  while (pos < src.length) {
    const wsStart = pos;
    while (pos < src.length && whitespace.test(src[pos]!)) pos += 1;
    if (pos >= src.length) {
      return { attributes, trailing: src.slice(wsStart) };
    }

    const leading = src.slice(wsStart, pos);
    const from = pos;
    const sigil = src[pos];

    if (sigil === "#" || sigil === ".") {
      pos += 1;
      const nameStart = pos;
      while (pos < src.length && !whitespace.test(src[pos]!)) pos += 1;
      attributes.push({
        from,
        key: sigil === "#" ? "id" : "class",
        kind: sigil === "#" ? "id" : "class",
        leading,
        quote: "",
        raw: src.slice(from, pos),
        to: pos,
        value: src.slice(nameStart, pos)
      });
      continue;
    }

    const bound = sigil === ":";
    if (bound) pos += 1;
    const keyStart = pos;
    while (pos < src.length && !whitespace.test(src[pos]!) && src[pos] !== "=") pos += 1;
    const key = src.slice(keyStart, pos);

    if (src[pos] !== "=") {
      attributes.push({ from, key, kind: "bare", leading, quote: "", raw: src.slice(from, pos), to: pos, value: true });
      continue;
    }

    pos += 1;
    const open = src[pos];
    let quote: MdcAttributeQuote = "";
    let value = "";

    if (open === '"' || open === "'") {
      quote = open;
      pos += 1;
      while (pos < src.length && src[pos] !== quote) {
        if (src[pos] === "\\" && pos + 1 < src.length) {
          // Bound values are JSON/YAML source: only an escaped quote belongs to the attribute,
          // every other escape is left for that parser.
          if (bound && src[pos + 1] !== quote) value += src[pos];
          pos += 1;
        }
        value += src[pos];
        pos += 1;
      }
      // An unterminated quote runs to the end of the source.
      if (pos < src.length) pos += 1;
    } else {
      const valueStart = pos;
      while (pos < src.length && !whitespace.test(src[pos]!)) pos += 1;
      value = src.slice(valueStart, pos);
    }

    attributes.push({
      from,
      key,
      kind: bound ? "bound" : "string",
      leading,
      quote,
      raw: src.slice(from, pos),
      to: pos,
      value
    });
  }

  return { attributes, trailing: "" };
}

export function serializeMdcAttributes(list: MdcAttributeList): string {
  return (
    list.attributes
      .map((attribute, index) => (index === 0 ? attribute.leading : attribute.leading || " ") + renderMdcAttribute(attribute))
      .join("") + list.trailing
  );
}

/** Source text for one attribute: `raw` when present, otherwise rendered from its fields. */
export function renderMdcAttribute(attribute: MdcAttribute): string {
  if (attribute.raw !== undefined) return attribute.raw;

  switch (attribute.kind) {
    case "id":
      return `#${String(attribute.value)}`;
    case "class":
      return `.${String(attribute.value)}`;
    case "bare":
      return attribute.key;
    case "bound":
      return `:${attribute.key}=${quoteAttributeValue(String(attribute.value), attribute.quote || "'", false)}`;
    case "string":
      return `${attribute.key}=${quoteAttributeValue(String(attribute.value), attribute.quote, true)}`;
  }
}

/**
 * Quotes a value, keeping the preferred quote when the value allows it, switching to the other
 * quote to avoid escapes, and escaping only as a last resort. `""` keeps a value unquoted when
 * it has no characters that would end it.
 */
function quoteAttributeValue(value: string, preferred: MdcAttributeQuote, escapeBackslashes: boolean) {
  if (preferred === "" && unquotedValue.test(value)) return value;

  const first: '"' | "'" = preferred === "'" ? "'" : '"';
  const second: '"' | "'" = first === '"' ? "'" : '"';
  const hasBackslash = escapeBackslashes && value.includes("\\");

  if (!hasBackslash) {
    if (!value.includes(first)) return `${first}${value}${first}`;
    if (!value.includes(second)) return `${second}${value}${second}`;
  }

  const escaped = (escapeBackslashes ? value.replace(/\\/g, "\\\\") : value).split(first).join(`\\${first}`);
  return `${first}${escaped}${first}`;
}
