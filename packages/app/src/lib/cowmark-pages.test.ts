import { describe, expect, it } from "vitest";
import {
  collectFieldValues,
  collectionForDocument,
  datePart,
  localDate,
  newPageText,
  pageFieldCharLimit,
  pageFieldKind,
  parseCollectionsSchema,
  slugify,
  withDatePart,
} from "./cowmark-pages";

const schema = JSON.stringify({
  collections: [
    {
      name: "blog",
      folder: "content/blog",
      url: "/blog/<file name without .md>",
      fields: [
        { name: "title", type: "string", required: true },
        { name: "description", type: "string", required: false, default: "" },
        { name: "feature_image", type: "string", required: false },
        { name: "published_at", type: "string", required: true },
        { name: "featured", type: "boolean", required: false, default: false },
        { name: "primary_tag", type: "{ slug: string, name: string }", required: false },
        { name: "tags", type: "list of { slug: string, name: string }", required: false, default: [] },
        { name: "category", type: "string", required: false },
        { name: "updated_on", type: "date", required: false },
        { name: "schema_org", type: "list of object", required: false, default: [] },
      ],
    },
    { name: "cuisines", folder: "content/cuisines", fields: [{ name: "title", type: "string", required: true }] },
  ],
});

describe("collections", () => {
  const collections = parseCollectionsSchema(schema);

  it("matches a document to its collection by folder", () => {
    expect(collectionForDocument(collections, "/repo", "/repo/content/blog/a.md")?.name).toBe("blog");
    expect(collectionForDocument(collections, "/repo/", "/repo/content/cuisines/x/y.md")?.name).toBe("cuisines");
    expect(collectionForDocument(collections, "C:\\Repo", "c:\\repo\\content\\blog\\a.md")?.name).toBe("blog");
    expect(collectionForDocument(collections, "/repo", "/repo/content/help/a.md")).toBeNull();
    expect(collectionForDocument(collections, "/repo", "/repo/content/blog/a/photo.png")).toBeNull();
    expect(collectionForDocument(collections, "/repo", "/elsewhere/content/blog/a.md")).toBeNull();
  });

  it("ignores a malformed schema", () => {
    expect(parseCollectionsSchema("{")).toEqual([]);
    expect(parseCollectionsSchema(null)).toEqual([]);
  });

  it("picks a control for each field", () => {
    const kinds = Object.fromEntries(collections[0]!.fields.map((field) => [field.name, pageFieldKind(field)]));
    expect(kinds).toEqual({
      category: "text",
      description: "textarea",
      feature_image: "image",
      featured: "boolean",
      primary_tag: "yaml",
      published_at: "date",
      schema_org: "yaml",
      tags: "yaml",
      title: "text",
      updated_on: "date",
    });
    expect(pageFieldCharLimit({ name: "meta_title", required: false, type: "string" })).toBe(60);
    expect(pageFieldCharLimit({ name: "description", required: false, type: "string" })).toBe(160);
    expect(pageFieldCharLimit({ name: "excerpt", required: false, type: "string" })).toBeNull();
  });
});

describe("dates", () => {
  it("writes plain dates and keeps an older timestamp's time", () => {
    expect(localDate(new Date(2026, 9, 6, 23, 5))).toBe("2026-10-06");
    expect(datePart("2026-10-06T12:06:00.000+00:00")).toBe("2026-10-06");
    expect(datePart("2026-10-06")).toBe("2026-10-06");
    expect(datePart(undefined)).toBe("");
    expect(withDatePart("2026-10-06", "2026-11-01")).toBe("2026-11-01");
    expect(withDatePart(undefined, "2026-11-01")).toBe("2026-11-01");
    expect(withDatePart("2026-10-06T12:06:00.000+00:00", "2026-11-01")).toBe("2026-11-01T12:06:00.000+00:00");
  });
});

describe("new pages", () => {
  it("slugifies titles", () => {
    expect(slugify("Papa John's Catering: Get a $100 Gift Card!")).toBe("papa-johns-catering-get-a-100-gift-card");
    expect(slugify("  Café  Déjà vu ")).toBe("cafe-deja-vu");
  });

  it("fills the required fields", () => {
    const text = newPageText(parseCollectionsSchema(schema)[0]!, {
      now: new Date(2026, 9, 26, 14, 5),
      title: "Lunch: a guide",
    });
    expect(text).toBe("---\ntitle: 'Lunch: a guide'\npublished_at: '2026-10-26'\n---\n\n");
  });
});

describe("collectFieldValues", () => {
  it("offers values that repeat across files, most used first", () => {
    const files = [
      "---\ntitle: A\ncategory: tips\nlayout: wide\n---\nBody",
      "---\ntitle: B\ncategory: news\n---\n",
      "---\ntitle: C\ncategory: tips\n---\n",
      "no front matter",
    ];
    expect(collectFieldValues(files)).toEqual({ category: ["tips", "news"] });
  });
});
