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
