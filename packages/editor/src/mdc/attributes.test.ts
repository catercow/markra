import { parseMdcAttributes, renderMdcAttribute, serializeMdcAttributes } from "./attributes.ts";

const realOpeners = [
  'title="CaterCow Pro Tip"',
  'variant="warning"',
  "columns=2",
  'to="/cuisines/american-catering" cta="Browse American & BBQ Catering"',
  'to="https://www.catercow.com/dishes" cta="Browse engagement party catering →"',
  'name="Quang Nguyen" title="Owner" company="Umami Tsunami Taqueria"',
  `title='"Make it Deluxe" Wins for Small Groups' number`,
  'title="Use the 2-Hour Lead Time as Your Edge" number'
];

describe("parseMdcAttributes", () => {
  it.each(realOpeners)("round-trips %s byte-for-byte", (src) => {
    expect(serializeMdcAttributes(parseMdcAttributes(src))).toBe(src);
  });

  it("round-trips odd spacing, escapes and every attribute kind", () => {
    const src = '  #hero  .wide .dark key="a \\"b\\" c"\tcompact :items=\'["x", "y"]\' size=lg   ';
    expect(serializeMdcAttributes(parseMdcAttributes(src))).toBe(src);
  });

  it("parses kinds, keys and values in order", () => {
    const { attributes } = parseMdcAttributes(
      `#intro .wide title="Common questions" compact columns=2 note='it\\'s: fine' :items='["a","b"]' :count="3"`
    );

    expect(attributes.map(({ key, kind, quote, value }) => ({ key, kind, quote, value }))).toEqual([
      { key: "id", kind: "id", quote: "", value: "intro" },
      { key: "class", kind: "class", quote: "", value: "wide" },
      { key: "title", kind: "string", quote: '"', value: "Common questions" },
      { key: "compact", kind: "bare", quote: "", value: true },
      { key: "columns", kind: "string", quote: "", value: "2" },
      { key: "note", kind: "string", quote: "'", value: "it's: fine" },
      { key: "items", kind: "bound", quote: "'", value: '["a","b"]' },
      { key: "count", kind: "bound", quote: '"', value: "3" }
    ]);
  });

  it("records source ranges", () => {
    const src = 'a="x y" b';
    const { attributes } = parseMdcAttributes(src);
    expect(attributes.map(({ from, to }) => src.slice(from, to))).toEqual(['a="x y"', "b"]);
  });

  it("keeps JSON escapes inside bound values for the JSON parser", () => {
    const { attributes } = parseMdcAttributes(':items="[\\"a\\\\nb\\"]"');
    expect(JSON.parse(String(attributes[0]!.value))).toEqual(["a\\nb"]);
  });

  it("treats an unterminated quote as running to the end", () => {
    const src = 'title="open ended';
    const list = parseMdcAttributes(src);
    expect(list.attributes[0]!.value).toBe("open ended");
    expect(serializeMdcAttributes(list)).toBe(src);
  });

  it("handles empty input", () => {
    expect(parseMdcAttributes("")).toEqual({ attributes: [], trailing: "" });
  });
});

describe("renderMdcAttribute", () => {
  const base = { key: "title", leading: "" };

  it("keeps the preferred quote, switches quotes before escaping", () => {
    expect(renderMdcAttribute({ ...base, kind: "string", quote: '"', value: "plain" })).toBe('title="plain"');
    expect(renderMdcAttribute({ ...base, kind: "string", quote: '"', value: 'say "hi"' })).toBe(`title='say "hi"'`);
    expect(renderMdcAttribute({ ...base, kind: "string", quote: '"', value: `it's "x"` })).toBe('title="it\'s \\"x\\""');
  });

  it("keeps unquoted values unquoted only while safe", () => {
    expect(renderMdcAttribute({ ...base, kind: "string", quote: "", value: "3" })).toBe("title=3");
    expect(renderMdcAttribute({ ...base, kind: "string", quote: "", value: "two words" })).toBe('title="two words"');
  });

  it("renders bare, bound, id and class attributes", () => {
    expect(renderMdcAttribute({ ...base, kind: "bare", quote: "", value: true })).toBe("title");
    expect(renderMdcAttribute({ ...base, kind: "bound", quote: "", value: "false" })).toBe(":title='false'");
    expect(renderMdcAttribute({ key: "id", leading: "", kind: "id", quote: "", value: "top" })).toBe("#top");
    expect(renderMdcAttribute({ key: "class", leading: "", kind: "class", quote: "", value: "wide" })).toBe(".wide");
  });

  it("round-trips rendered values through the parser", () => {
    for (const value of ['a "b"', "c'd", `e'f"g`, "back\\slash", "colon: yes"]) {
      const text = renderMdcAttribute({ ...base, kind: "string", quote: '"', value });
      expect(parseMdcAttributes(text).attributes[0]!.value).toBe(value);
    }
  });

  it("serializes edited entries with a separating space", () => {
    const list = parseMdcAttributes('a="1"');
    list.attributes.push({ key: "b", kind: "bare", leading: "", quote: "", value: true });
    expect(serializeMdcAttributes(list)).toBe('a="1" b');
  });
});
