import { afterEach, describe, expect, it, vi } from "vitest";
import { setCatercowPreviewSiteForTests } from "./content-access";
import { contentLintMessages, contentLintSource, type ContentLint } from "./content-lint";

afterEach(() => setCatercowPreviewSiteForTests(undefined));

describe("contentLintSource", () => {
  const root = "/Users/me/Documents/catercow-content";

  function source(documentPath: string | null, lint: ContentLint | undefined = vi.fn(async () => [])) {
    return { lint, source: contentLintSource({ contentRoot: () => root, documentPath: () => documentPath, language: "en", lint }) };
  }

  it("is absent in stock Markra or without the runtime linter", () => {
    setCatercowPreviewSiteForTests(null);
    expect(source(`${root}/content/a.md`).source).toBeUndefined();

    setCatercowPreviewSiteForTests("https://www.catercow.com");
    expect(contentLintSource({ contentRoot: () => root, documentPath: () => `${root}/content/a.md`, language: "en", lint: undefined })).toBeUndefined();
  });

  it("lints Markdown files in the content folder by their folder-relative path", async () => {
    setCatercowPreviewSiteForTests("https://www.catercow.com");
    const found = [{ fromUtf16: 0, message: "m", rule: "image-alt", severity: "warning" as const, toUtf16: 1 }];
    const lint = vi.fn(async () => found);
    const { source: external } = source(`${root}/content/blog/a.md`, lint);

    await expect(external!("text")).resolves.toEqual(found);
    expect(lint).toHaveBeenCalledWith(root, "content/blog/a.md", "text", expect.objectContaining({
      imageAlt: expect.stringContaining("no description"),
      unknownComponent: expect.stringContaining("{name}"),
    }));
  });

  it("leaves other files to the built-in lint", () => {
    setCatercowPreviewSiteForTests("https://www.catercow.com");
    expect(source("/Users/me/notes/a.md").source!("text")).toBeNull();
    expect(source(`${root}/content/data.yml`).source!("text")).toBeNull();
    expect(source(null).source!("text")).toBeNull();
    expect(contentLintSource({ contentRoot: () => null, documentPath: () => `${root}/a.md`, language: "en", lint: vi.fn() })!("t")).toBeNull();
  });
});

describe("contentLintMessages", () => {
  it("has a template for every message the Rust linter uses", () => {
    const messages = contentLintMessages("en");
    expect(Object.keys(messages)).toHaveLength(31);
    for (const [key, text] of Object.entries(messages)) {
      expect(text, key).not.toMatch(/^editor\./u);
    }
    expect(messages.missingImage).toContain("`{src}`");
    expect(messages.brokenLink).toContain("`{path}`");
  });
});
