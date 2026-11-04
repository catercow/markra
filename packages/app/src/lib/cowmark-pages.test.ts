import { describe, expect, it } from "vitest";
import {
  collectFieldValues,
  collectionForDocument,
  creatableCollections,
  datePart,
  folderChain,
  localDate,
  missingRequiredFields,
  newPageFields,
  newPageInitialValues,
  newPageText,
  pageFieldCharLimit,
  pageFieldKind,
  pageUrl,
  parseCollectionsSchema,
  publicSitePath,
  slugDraft,
  slugify,
  withDatePart,
} from "./cowmark-pages";

// The shape the site's schema publishes now: labels, help text and image formats.
const siteSchema = JSON.stringify({
  collections: [
    {
      name: "competitors",
      label: "Competitor comparison",
      description: "CaterCow compared with another service.",
      creatable: true,
      slugExample: "cater2me",
      folder: "content/competitors",
      url: "/competitors/<file name without .md>",
      fields: [
        { name: "title", type: "string", required: true },
        { name: "description", type: "string", required: true, description: "Meta description." },
        { name: "competitor", type: "string", required: true },
        { name: "logo", type: "string", required: true, format: "image", description: "Logo for light backgrounds." },
        { name: "contact_cta", type: "boolean", required: false, default: false },
        { name: "rank", type: "number", required: true },
        { name: "faqs", type: "list of { question: string }", required: true },
        { name: "unknown_key", type: "string", required: false, whatever: 1 },
        { name: "photo", type: "string", required: false },
      ],
      somethingNew: { ignored: true },
    },
    {
      name: "dietaryRequirements",
      label: "Dietary requirement",
      creatable: true,
      folder: "content/dietary-requirements",
      url: "/dietary-requirements/<file name without .md>",
      fields: [
        { name: "title", type: "string", required: true },
        { name: "hero_images", type: "list of string", required: false, format: "image", default: [] },
        { name: "hero_image_alt", type: "string", required: false },
        { name: "published_on", type: "string", required: true },
      ],
    },
    { name: "minorityOwned", label: "Minority-owned", creatable: false, folder: "content/minority-owned", fields: [] },
    { name: "blog", label: "Blog post", folder: "content/blog", url: "/blog/<file name without .md>", fields: [] },
    { name: "cuisines", folder: "content/cuisines", url: "/cuisines/<file name without .md>-catering", fields: [] },
  ],
});

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
  it("lists a page type's folders outermost first, so missing ones can be created", () => {
    expect(folderChain("content/competitors/")).toEqual([
      { name: "content", parent: null },
      { name: "competitors", parent: "content" }
    ]);
  });

  it("slugifies titles", () => {
    expect(slugify("Papa John's Catering: Get a $100 Gift Card!")).toBe("papa-johns-catering-get-a-100-gift-card");
    expect(slugify("  Café  Déjà vu ")).toBe("cafe-deja-vu");
  });

  it("keeps a typed file name to the slug convention", () => {
    expect(slugDraft("Accounting Tax ")).toBe("accounting-tax-");
    expect(slugDraft("accounting--tax--catering")).toBe("accounting-tax-catering");
    expect(slugDraft("-Café_Déjà")).toBe("cafe-deja");
    expect(slugDraft("a/b")).toBe("a-b");
    expect(slugDraft("")).toBe("");
    expect(slugDraft("--")).toBe("");
  });

  it("shows the page's address from the collection's url pattern", () => {
    const [competitors, , , blog, cuisines] = parseCollectionsSchema(siteSchema);
    expect(pageUrl(competitors!, "ezcater")).toBe("/competitors/ezcater");
    expect(pageUrl(cuisines!, "thai")).toBe("/cuisines/thai-catering");
    expect(pageUrl({ ...blog!, url: undefined }, "a")).toBe("/blog/a");
    expect(pageUrl({ ...blog!, url: "/c/<file name without .md>" }, "about")).toBe("/c/about");
  });

  it("offers creatable collections, blog first", () => {
    expect(creatableCollections(parseCollectionsSchema(siteSchema)).map((collection) => collection.name))
      .toEqual(["blog", "competitors", "dietaryRequirements", "cuisines"]);
  });

  it("asks for required fields without a default and leaves images for Page details", () => {
    const competitors = parseCollectionsSchema(siteSchema)[0]!;
    const { asked, later } = newPageFields(competitors);
    expect(asked.map((field) => field.name)).toEqual(["description", "competitor", "rank"]);
    expect(later.map((field) => field.name)).toEqual(["logo", "faqs"]);
    const dietary = parseCollectionsSchema(siteSchema)[1]!;
    expect(newPageInitialValues(dietary, new Date(2026, 10, 4))).toEqual({ published_on: "2026-11-04" });
  });

  it("writes the title and the filled fields only", () => {
    const competitors = parseCollectionsSchema(siteSchema)[0]!;
    const values = { competitor: "ezCater", contact_cta: false, description: "  ", rank: 3 };
    expect(newPageText(competitors, { title: "CaterCow vs. ezCater", values })).toBe(
      "---\ntitle: CaterCow vs. ezCater\ncompetitor: ezCater\ncontact_cta: false\nrank: 3\n---\n\n"
    );
    expect(missingRequiredFields(competitors, values)).toEqual(["description", "logo", "faqs"]);
    expect(missingRequiredFields(competitors, { ...values, description: "x", faqs: [{ question: "q" }], logo: "/l.svg" }))
      .toEqual([]);
    expect(newPageText(parseCollectionsSchema(schema)[0]!, { title: "Lunch: a guide", values: { published_at: "2026-10-26" } }))
      .toBe("---\ntitle: 'Lunch: a guide'\npublished_at: '2026-10-26'\n---\n\n");
  });

  it("links files under public/ by their site path", () => {
    expect(publicSitePath("/repo", "/repo/public/blog/a/b.jpg")).toBe("/blog/a/b.jpg");
    expect(publicSitePath("C:\\Repo", "c:\\repo\\public\\x.png")).toBe("/x.png");
    expect(publicSitePath("/repo", "/repo/content/blog/b.jpg")).toBeNull();
    expect(publicSitePath("/repo", "/elsewhere/public/b.jpg")).toBeNull();
  });
});

describe("collections from the site's schema", () => {
  const [competitors, dietary, fixed, blog] = parseCollectionsSchema(siteSchema);

  it("reads labels, help text and image formats, ignoring unknown keys", () => {
    expect(competitors).toMatchObject({
      creatable: true,
      description: "CaterCow compared with another service.",
      label: "Competitor comparison",
      slugExample: "cater2me",
    });
    expect(competitors!.fields.find((field) => field.name === "logo")).toEqual({
      description: "Logo for light backgrounds.",
      format: "image",
      name: "logo",
      required: true,
      type: "string",
    });
    expect(fixed!.creatable).toBe(false);
    expect(blog!.creatable).toBe(true);
  });

  it("picks image controls from the format, not the name", () => {
    const kind = (collection: typeof competitors, name: string) =>
      pageFieldKind(collection!.fields.find((field) => field.name === name)!);
    expect(kind(competitors, "logo")).toBe("image");
    expect(kind(competitors, "photo")).toBe("text");
    expect(kind(dietary, "hero_images")).toBe("images");
    expect(kind(dietary, "published_on")).toBe("date");
  });

  it("falls back to names and labels for older schemas", () => {
    const [cuisines] = parseCollectionsSchema(JSON.stringify({
      collections: [{ name: "cuisines", folder: "content/cuisines", fields: [{ name: "hero_image", type: "string" }] }],
    }));
    expect(cuisines).toMatchObject({ creatable: true, label: "cuisines" });
    expect(cuisines!.description).toBeUndefined();
    expect(pageFieldKind(cuisines!.fields[0]!)).toBe("image");
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
