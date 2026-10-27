import { describe, expect, it } from "vitest";

import { parseMdcSchema } from "./mdc-schema.ts";

describe("parseMdcSchema", () => {
  it("parses components, props, nested items and examples", () => {
    const schema = parseMdcSchema(JSON.stringify({
      generatedBy: "agent-files.mjs",
      components: [
        {
          name: "address-cta",
          component: "AddressCta",
          group: "Calls to action",
          purpose: "Address box.",
          content: true,
          props: [
            {
              name: "background",
              type: "\"none\" | \"warm\"",
              required: false,
              description: "Box color.",
              default: "\"none\"",
              options: ["none", "warm"]
            },
            {
              name: "rows",
              type: "PriceRow[]",
              required: true,
              items: [{ name: "category", type: "string", required: true }]
            }
          ],
          example: { from: "blog/a.md", markdown: "::address-cta\n::" }
        }
      ]
    }));

    expect(schema).toEqual({
      generatedBy: "agent-files.mjs",
      components: [
        {
          name: "address-cta",
          component: "AddressCta",
          group: "Calls to action",
          purpose: "Address box.",
          content: true,
          props: [
            {
              name: "background",
              type: "\"none\" | \"warm\"",
              required: false,
              description: "Box color.",
              default: "\"none\"",
              options: ["none", "warm"]
            },
            {
              name: "rows",
              type: "PriceRow[]",
              required: true,
              items: [{ name: "category", type: "string", required: true }]
            }
          ],
          example: { from: "blog/a.md", markdown: "::address-cta\n::" }
        }
      ]
    });
  });

  it("returns null for non-JSON or a missing components array", () => {
    expect(parseMdcSchema("not json")).toBeNull();
    expect(parseMdcSchema("[]")).toBeNull();
    expect(parseMdcSchema("{\"components\": {}}")).toBeNull();
    expect(parseMdcSchema("null")).toBeNull();
  });

  it("drops malformed components and props and tolerates missing fields", () => {
    const schema = parseMdcSchema(JSON.stringify({
      components: [
        null,
        "callout",
        { name: "" },
        { component: "NoName" },
        {
          name: "callout",
          props: [{ type: "string" }, 7, { name: "tone", options: ["a", 2], default: 3, required: "yes" }],
          example: { markdown: 5 }
        }
      ]
    }));

    expect(schema).toEqual({
      components: [
        {
          name: "callout",
          props: [{ name: "tone", options: ["a"], default: "3" }]
        }
      ]
    });
  });
});

describe("parseMdcSchema component input", () => {
  it("reads table and image inputs, link formats and search filters, and ignores what it doesn't know", () => {
    const schema = parseMdcSchema(JSON.stringify({
      components: [
        {
          name: "pricing-grid",
          props: [],
          input: {
            kind: "table",
            columns: [
              { name: "Item", aliases: ["Name", 3], required: true, format: "text", markdown: true },
              { name: "" },
              { name: "Price", format: "price" }
            ]
          }
        },
        { name: "gallery", props: [], input: { kind: "images" } },
        {
          name: "packages",
          props: [{
            name: "link",
            type: "string",
            format: "search-link",
            searchParams: { package_type: { values: ["breakfast", 2] }, package_category: { values: null }, x: 5 }
          }]
        },
        { name: "chart", props: [], input: { kind: "chart" } },
        { name: "empty-table", props: [], input: { kind: "table", columns: [] } }
      ]
    }));

    expect(schema?.components[0]?.input).toEqual({
      columns: [
        { aliases: ["Name"], format: "text", markdown: true, name: "Item", required: true },
        { format: "price", name: "Price" }
      ],
      kind: "table"
    });
    expect(schema?.components[1]?.input).toEqual({ kind: "images" });
    expect(schema?.components[2]?.props[0]).toMatchObject({
      format: "search-link",
      searchParams: { package_category: { values: null }, package_type: { values: ["breakfast"] }, x: { values: null } }
    });
    expect(schema?.components[3]?.input).toBeUndefined();
    expect(schema?.components[4]?.input).toBeUndefined();
  });
});
