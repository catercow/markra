import { contentSchemaNotice, contentSchemaSiteLabel, type ContentSchemaResult } from "./content-schema";

const url = "https://content-preview.catercow.com/_cowmark/schema.json";

function result(partial: Partial<ContentSchemaResult>): ContentSchemaResult {
  return {
    collections: "{}",
    components: "{}",
    hash: "h",
    origin: { kind: "site", url },
    stale: false,
    url,
    warnings: [],
    ...partial
  };
}

describe("content schema notice", () => {
  it("says nothing while the site's schema or a recent cached copy is in use", () => {
    expect(contentSchemaNotice(null)).toBeNull();
    expect(contentSchemaNotice(result({}))).toBeNull();
    expect(
      contentSchemaNotice(result({ origin: { ageSecs: 3600, kind: "cache", url }, warnings: ["Couldn't get it."] }))
    ).toBeNull();
    // The folder's files when there is no site to ask.
    expect(contentSchemaNotice(result({ origin: { kind: "repo" }, url: null }))).toBeNull();
  });

  it("flags a week-old cached copy, a fallback after a failed fetch, and no schema", () => {
    expect(contentSchemaNotice(result({ origin: { ageSecs: 9 * 86400, kind: "cache", url }, stale: true }))).toEqual({
      kind: "outOfDate",
      site: "content-preview.catercow.com"
    });
    expect(contentSchemaNotice(result({ origin: { kind: "repo" }, warnings: ["Couldn't get it."] }))).toEqual({
      kind: "outOfDate",
      site: "content-preview.catercow.com"
    });
    expect(contentSchemaNotice(result({ components: null, collections: null, origin: { kind: "none" } }))).toEqual({
      kind: "missing",
      site: "content-preview.catercow.com"
    });
  });

  it("names the site by host", () => {
    expect(contentSchemaSiteLabel("http://localhost:3000/_cowmark/schema.json")).toBe("localhost:3000");
    expect(contentSchemaSiteLabel(null)).toBe("the site");
    expect(contentSchemaSiteLabel("not a url")).toBe("not a url");
  });
});
