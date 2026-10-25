import { fieldKind, formatMdcValue, parseMdcValue, readMdcProps, schemaDefault, selectOptions, writeMdcProp, type MdcPropSchema } from "./form-model.ts";
import { schemas } from "./form-model.fixtures.ts";

const prop = (component: string, name: string): MdcPropSchema => {
  const found = schemas[component]!.props.find((candidate) => candidate.name === name);
  if (!found) throw new Error(`${component}.${name} missing from fixture`);
  return found;
};

// Real blocks from the CaterCow marketing-content repo, trimmed.
const faqsYaml = `title: "Baby Shower Catering FAQ"
structured-data: true
items:
  - question: 'What food is best for a baby shower?'
    answer: 'The best baby shower catering foods are light, brunch-friendly, and easy to eat while standing or socializing.'
  - question: 'How much does baby shower catering cost?'
    answer: 'Baby shower catering usually costs between $12 and $25 per person for drop-off or buffet-style service.'
`;

const pricingYaml = `items:
  - name: Party Platter Triple Dipper
    price: 7.25
    upcharge: 0
    description: Choose any three appetizers on one platter. Served with dipping sauces. Serves up to 8. Platter $58.
    tag: Most Popular
  - name: Party Platter Boneless Wings (32 ct)
    price: 6.00
    upcharge: 0
    description: 32 boneless wings, up to 2 flavors. Served with ranch or blue cheese. Serves 6–8. Platter $42. (48 ct $56.)
    tag: Best Value
`;

const comparisonYaml = `columns:
  - Chili's
  - Applebee's
  - TGI Fridays
rows:
  - category: Price Per Person
    values:
      - $5–$12
      - $13–$20
      - $10–$15
  # Formats differ the most.
  - category: Best Format
    values:
      - Party Platters via regular menu
      - Dedicated catering trays
      - Family Meals & Platters via regular menu
`;

const ihopYaml = `items:
  - name: Pancake Breakfast Platter
    description: "20 Original Buttermilk Pancakes, small tray of scrambled eggs.\\nFeeds 6–10.\\n$135 Total"
    price: 135
`;

const faqs = schemas["faqs"];
const pricing = schemas["pricing-grid"];
const comparison = schemas["comparison-table"];

describe("fieldKind", () => {
  it.each([
    ["callout", "title", "text"],
    ["card", "number", "boolean"],
    ["grid", "columns", "select"],
    ["feature-grid", "columns", "select"],
    ["bullet-list", "variant", "select"],
    ["bullet-list", "items", "string-list"],
    ["comparison-table", "columns", "string-list"],
    ["faqs", "items", "object-list"],
    ["pricing-grid", "items", "object-list"]
  ])("%s.%s is %s", (component, name, kind) => {
    expect(fieldKind(prop(component, name))).toBe(kind);
  });

  it("uses item field types inside object lists", () => {
    const fields = prop("pricing-grid", "items").items!;
    const kinds = Object.fromEntries(fields.map((field) => [field.name, fieldKind(field)]));
    expect(kinds).toEqual({
      count: "text",
      description: "textarea",
      name: "text",
      "per-person": "boolean",
      price: "number",
      secondary_description: "textarea",
      tag: "text",
      upcharge: "number"
    });
    expect(fieldKind(prop("faqs", "items").items!.find((field) => field.name === "answer")!)).toBe("textarea");
  });

  it("uses a textarea when the description asks for prose", () => {
    expect(fieldKind({ description: "One short paragraph under the title.", name: "intro", type: "string" })).toBe("textarea");
  });

  it("falls back to json for shapes it cannot edit as a form", () => {
    expect(fieldKind({ name: "rows", type: "GalleryImage[][]" })).toBe("json");
    expect(fieldKind({ name: "items", type: "Faq[]" })).toBe("json");
    expect(fieldKind({ name: "data", type: "Record<string, number>" })).toBe("json");
  });
});

describe("selectOptions and schemaDefault", () => {
  it("reads options from the schema or a literal union", () => {
    expect(selectOptions(prop("grid", "columns"))).toEqual(["2", "1", "3", "4"]);
    expect(selectOptions(prop("feature-grid", "columns"))).toEqual([2, 3, 4]);
    expect(selectOptions({ name: "variant", type: `"primary" | 'plain'` })).toEqual(["primary", "plain"]);
    expect(selectOptions({ name: "x", type: "string | number" })).toBeNull();
  });

  it("parses default literals", () => {
    expect(schemaDefault(prop("faqs", "title"))).toBe("Frequently Asked Questions");
    expect(schemaDefault(prop("faqs", "items"))).toEqual([]);
    expect(schemaDefault(prop("card", "number"))).toBe(false);
    expect(schemaDefault(prop("feature-grid", "columns"))).toBe(3);
    expect(schemaDefault(prop("cta-banner", "to"))).toBeUndefined();
    expect(schemaDefault({ default: "'single'", name: "x", type: "string" })).toBe("single");
    expect(schemaDefault({ default: "someFunction()", name: "x", type: "string" })).toBeUndefined();
  });
});

describe("readMdcProps", () => {
  it("merges attributes, YAML and defaults with their sources", () => {
    const read = readMdcProps({ attributes: 'title="Common questions"', schema: faqs, yaml: "structured-data: true\n" });
    expect(read.values).toEqual({ items: [], "structured-data": true, title: "Common questions" });
    expect(read.source).toEqual({ items: "default", "structured-data": "yaml", title: "attribute" });
    expect(read.unknown).toEqual([]);
    expect(read.errors).toEqual([]);
  });

  it("reads bare, unquoted and bound attributes", () => {
    expect(readMdcProps({ attributes: 'title="Tip" number', schema: schemas["card"], yaml: null }).values).toEqual({
      number: true,
      title: "Tip"
    });
    expect(readMdcProps({ attributes: "columns=2", schema: schemas["grid"], yaml: null }).values).toEqual({ columns: "2" });
    expect(readMdcProps({ attributes: "columns=4", schema: schemas["feature-grid"], yaml: null }).values.columns).toBe(4);
    expect(
      readMdcProps({ attributes: `:items='["One", "Two"]' variant=warning`, schema: schemas["bullet-list"], yaml: null }).values
    ).toEqual({ items: ["One", "Two"], variant: "warning" });
    expect(readMdcProps({ attributes: ":items='[a, b]'", schema: schemas["bullet-list"], yaml: null }).values.items).toEqual(["a", "b"]);
  });

  it("normalizes camelCase keys to kebab-case", () => {
    const read = readMdcProps({ attributes: "structuredData", schema: faqs, yaml: "itemsCount: 2\n" });
    expect(read.values["structured-data"]).toBe(true);
    expect(read.unknown).toEqual(["items-count"]);
  });

  it("leaves #id and .class out of the props", () => {
    const read = readMdcProps({ attributes: '#top .wide title="Tip"', schema: schemas["callout"], yaml: null });
    expect(read.values).toEqual({ title: "Tip" });
    expect(read.unknown).toEqual([]);
  });

  it("lets YAML win over an attribute of the same name", () => {
    const read = readMdcProps({ attributes: 'title="Inline"', schema: faqs, yaml: "title: Block\n" });
    expect(read.values.title).toBe("Block");
    expect(read.source.title).toBe("yaml");
  });

  it("lists every prop as unknown without a schema", () => {
    expect(readMdcProps({ attributes: "a=1", yaml: "b: 2\n" }).unknown).toEqual(["a", "b"]);
  });

  it("reads real blocks", () => {
    const read = readMdcProps({ attributes: null, schema: pricing, yaml: pricingYaml });
    expect((read.values.items as Array<{ price: number }>).map((item) => item.price)).toEqual([7.25, 6]);
    expect(readMdcProps({ attributes: null, schema: comparison, yaml: comparisonYaml }).values.columns).toEqual([
      "Chili's",
      "Applebee's",
      "TGI Fridays"
    ]);
  });

  it("reports invalid YAML and bound attributes without throwing", () => {
    const read = readMdcProps({ attributes: ":items='[unclosed'", schema: faqs, yaml: "title: [oops\n" });
    expect(read.errors).toHaveLength(2);
    expect(read.errors[0]).toMatch(/:items/);
    expect(read.errors[1]).toMatch(/^Invalid YAML/);
    expect(read.values.title).toBe("Frequently Asked Questions");
    expect(readMdcProps({ attributes: null, yaml: "- a\n- b\n" }).errors[0]).toMatch(/mapping/);
  });
});

describe("writeMdcProp", () => {
  it("leaves untouched text byte-identical when editing one key", () => {
    const out = writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, "title", "Shower FAQ");
    expect(out.yaml).toBe(faqsYaml.replace('"Baby Shower Catering FAQ"', '"Shower FAQ"'));
    expect(out.attributes).toBeNull();
  });

  it("edits a list item field by path array or path string, keeping its quote style", () => {
    const expected = faqsYaml.replace(
      "'Baby shower catering usually costs between $12 and $25 per person for drop-off or buffet-style service.'",
      "'About $12–$25: it depends.'"
    );
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, ["items", 1, "answer"], "About $12–$25: it depends.").yaml).toBe(expected);
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, "items[1].answer", "About $12–$25: it depends.").yaml).toBe(expected);
  });

  it("keeps comments, number formatting and order in real blocks", () => {
    const pricingOut = writeMdcProp({ attributes: null, schema: pricing, yaml: pricingYaml }, ["items", 0, "tag"], "Staff Pick").yaml;
    expect(pricingOut).toBe(pricingYaml.replace("tag: Most Popular", "tag: Staff Pick"));

    const comparisonOut = writeMdcProp({ attributes: null, schema: comparison, yaml: comparisonYaml }, ["rows", 1, "category"], "Format").yaml;
    expect(comparisonOut).toBe(comparisonYaml.replace("category: Best Format", "category: Format"));

    const ihopOut = writeMdcProp({ attributes: null, schema: pricing, yaml: ihopYaml }, ["items", 0, "price"], 140).yaml;
    expect(ihopOut).toBe(ihopYaml.replace("price: 135", "price: 140"));
  });

  it("keeps unchanged list items when the whole list is written back", () => {
    const items = readMdcProps({ attributes: null, schema: faqs, yaml: faqsYaml }).values.items as Array<Record<string, string>>;
    const next = [items[0], { ...items[1], question: "What does it cost?" }];
    const out = writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, "items", next).yaml;
    expect(out).toBe(faqsYaml.replace("'How much does baby shower catering cost?'", "'What does it cost?'"));
  });

  it("appends and removes list items", () => {
    const added = writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, ["items", 2], { answer: "Yes.", question: "Can I?" }).yaml;
    expect(added).toBe(`${faqsYaml}  - answer: Yes.\n    question: Can I?\n`);

    const removed = writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, ["items", 0], undefined).yaml!;
    expect(removed).not.toContain("best for a baby shower");
    expect(removed).toContain("How much does baby shower catering cost?");
  });

  it("updates a short value where it already lives inline", () => {
    expect(writeMdcProp({ attributes: 'title="CaterCow Pro Tip"', schema: schemas["callout"], yaml: null }, "title", "Reality Check")).toEqual({
      attributes: 'title="Reality Check"',
      yaml: null
    });
    expect(
      writeMdcProp({ attributes: 'to="/a" cta="Browse"', schema: schemas["cta-banner"], yaml: null }, "cta", 'Browse "American" →').attributes
    ).toBe(`to="/a" cta='Browse "American" →'`);
    expect(writeMdcProp({ attributes: "columns=2", schema: schemas["grid"], yaml: null }, "columns", "3").attributes).toBe("columns=3");
  });

  it("writes booleans as bare attributes inline and as YAML true", () => {
    const card = schemas["card"];
    expect(writeMdcProp({ attributes: 'title="Tip" :number="false"', schema: card, yaml: null }, "number", true).attributes).toBe('title="Tip" number');
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: "title: FAQ\n" }, "structured-data", true).yaml).toBe(
      "title: FAQ\nstructured-data: true\n"
    );
  });

  it("removes a prop set to its default or to undefined", () => {
    const card = schemas["card"];
    expect(writeMdcProp({ attributes: 'title="Tip" number', schema: card, yaml: null }, "number", false).attributes).toBe('title="Tip"');
    expect(writeMdcProp({ attributes: "columns=2", schema: schemas["grid"], yaml: null }, "columns", "2").attributes).toBeNull();
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: faqsYaml }, "items", []).yaml).toBe(
      'title: "Baby Shower Catering FAQ"\nstructured-data: true\n'
    );
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: "title: FAQ\n" }, "title", undefined).yaml).toBeNull();
  });

  it("moves an inline prop into YAML when its value outgrows the opener", () => {
    const longTitle = "A".repeat(90);
    expect(writeMdcProp({ attributes: 'title="Short" compact', schema: faqs, yaml: null }, "title", longTitle)).toEqual({
      attributes: "compact",
      yaml: `title: ${longTitle}`
    });
    expect(
      writeMdcProp({ attributes: ':items=\'["a"]\'', schema: schemas["bullet-list"], yaml: null }, "items", ["a", "b"])
    ).toEqual({ attributes: null, yaml: "items:\n  - a\n  - b" });
  });

  it("writes new props into YAML, creating the block", () => {
    expect(writeMdcProp({ attributes: 'title="Tip"', schema: faqs, yaml: null }, "structured-data", true)).toEqual({
      attributes: 'title="Tip"',
      yaml: "structured-data: true"
    });
  });

  it("quotes strings YAML would misread and never writes tabs", () => {
    const write = (value: string) => writeMdcProp({ attributes: null, schema: faqs, yaml: null }, "title", value).yaml;
    expect(write("Pricing: the real story")).toBe('title: "Pricing: the real story"');
    expect(write("# not a comment")).toBe('title: "# not a comment"');
    expect(write("- not a list")).toBe('title: "- not a list"');
    expect(write("true")).toBe('title: "true"');
    expect(write("tab\there")).toBe('title: "tab\\there"');
    for (const value of ["Pricing: the real story", "# not a comment", "- not a list", "true", "tab\there", "@handle", "a #b"]) {
      const yaml = write(value)!;
      expect(yaml).not.toContain("\t");
      expect(readMdcProps({ attributes: null, schema: faqs, yaml }).values.title).toBe(value);
    }
  });

  it("re-quotes an existing plain scalar when the new value needs it", () => {
    expect(writeMdcProp({ attributes: null, schema: pricing, yaml: pricingYaml }, ["items", 0, "tag"], "New: hot").yaml).toContain(
      'tag: "New: hot"'
    );
  });

  it("updates the existing camelCase key instead of adding a kebab one", () => {
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: "structuredData: true\ntitle: FAQ\n" }, "structured-data", "x").yaml).toBe(
      "structuredData: x\ntitle: FAQ\n"
    );
  });

  it("keeps the indentation of a nested YAML block", () => {
    const nested = "  title: FAQ\n  items:\n    - question: Q\n      answer: A\n";
    expect(writeMdcProp({ attributes: null, schema: faqs, yaml: nested }, ["items", 0, "answer"], "B").yaml).toBe(nested.replace("answer: A", "answer: B"));
  });

  it("refuses to edit invalid YAML", () => {
    const input = { attributes: 'title="x"', schema: faqs, yaml: "items: [oops\n" };
    const out = writeMdcProp(input, "title", "y");
    expect(out.attributes).toBe('title="x"');
    expect(out.yaml).toBe(input.yaml);
    expect(out.error).toMatch(/^Invalid YAML/);
  });

  it("round-trips real blocks unchanged when a value is rewritten to itself", () => {
    for (const [schema, yaml] of [
      [faqs, faqsYaml],
      [pricing, pricingYaml],
      [comparison, comparisonYaml],
      [pricing, ihopYaml]
    ] as const) {
      const { values } = readMdcProps({ attributes: null, schema, yaml });
      const [first] = Object.keys(values).filter((key) => key !== "structured-data");
      expect(writeMdcProp({ attributes: null, schema, yaml }, first!, values[first!]).yaml).toBe(yaml);
    }
  });
});

describe("free-form values", () => {
  it("formats values as YAML and parses YAML or JSON back", () => {
    expect(formatMdcValue([["a", "b"]])).toBe("- - a\n  - b");
    expect(formatMdcValue(undefined)).toBe("");
    expect(parseMdcValue("- - a\n  - b")).toEqual({ value: [["a", "b"]] });
    expect(parseMdcValue('[{"src": "/a.jpg"}]')).toEqual({ value: [{ src: "/a.jpg" }] });
    expect(parseMdcValue("  ")).toEqual({ value: undefined });
    expect(parseMdcValue("a: [").error).toBeTruthy();
  });
});

describe("filling empty strings", () => {
  it("drops the quotes an empty string needed once it has text", () => {
    const yaml = 'items:\n  - question: ""\n    answer: "Kept quotes"';
    expect(writeMdcProp({ attributes: null, yaml }, ["items", 0, "question"], "Can I tip?").yaml)
      .toBe('items:\n  - question: Can I tip?\n    answer: "Kept quotes"');
    expect(writeMdcProp({ attributes: null, yaml }, ["items", 0, "answer"], "New").yaml)
      .toBe('items:\n  - question: ""\n    answer: "New"');
  });

  it("does not keep quotes that a trailing space forced while typing", () => {
    let input = { attributes: null, yaml: "answer: Yes." } as { attributes: string | null; yaml: string | null };
    for (const value of ["Yes. ", "Yes. W", "Yes. We"]) input = writeMdcProp(input, "answer", value);
    expect(input.yaml).toBe("answer: Yes. We");
  });
});
