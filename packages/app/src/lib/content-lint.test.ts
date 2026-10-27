import { afterEach, describe, expect, it, vi } from "vitest";
import { setCatercowPreviewSiteForTests } from "./content-access";
import {
  contentCsvSource,
  contentLintMessages,
  contentLintSource,
  formatContentPage,
  managedContentPage,
  type ContentLint,
} from "./content-lint";

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
    expect(Object.keys(messages)).toHaveLength(63);
    for (const [key, text] of Object.entries(messages)) {
      expect(text, key).not.toMatch(/^editor\./u);
    }
    expect(messages.missingImage).toContain("`{src}`");
    expect(messages.brokenLink).toContain("`{path}`");
  });
});

describe("formatContentPage", () => {
  const root = "/Users/me/Documents/catercow-content";
  const page = `${root}/content/blog/a.md`;

  it("formats managed content pages only, and reports no change as null", async () => {
    setCatercowPreviewSiteForTests("https://www.catercow.com");
    const format = vi.fn(async (_root: string, _path: string, text: string) => ({
      changed: text !== "fixed",
      diagnostics: [],
      text: "fixed",
    }));
    expect(managedContentPage(root, page)).toBe("content/blog/a.md");
    await expect(formatContentPage({ documentPath: page, format, language: "en", root, text: "typed" })).resolves.toBe("fixed");
    expect(format).toHaveBeenCalledWith(root, "content/blog/a.md", "typed", expect.objectContaining({ cellFix: expect.any(String) }));
    await expect(formatContentPage({ documentPath: page, format, language: "en", root, text: "fixed" })).resolves.toBeNull();
    await expect(formatContentPage({ documentPath: "/elsewhere/a.md", format, language: "en", root, text: "x" })).resolves.toBeNull();
    await expect(formatContentPage({ documentPath: page, format: undefined, language: "en", root, text: "x" })).resolves.toBeNull();
    expect(format).toHaveBeenCalledTimes(2);
  });

  it("saves as typed when formatting fails or outside CowMark", async () => {
    setCatercowPreviewSiteForTests("https://www.catercow.com");
    const failing = vi.fn(async () => {
      throw new Error("boom");
    });
    await expect(formatContentPage({ documentPath: page, format: failing, language: "en", root, text: "x" })).resolves.toBeNull();
    setCatercowPreviewSiteForTests(null);
    expect(managedContentPage(root, page)).toBeNull();
  });
});

describe("contentCsvSource", () => {
  const root = "/Users/me/Documents/catercow-content";

  it("converts with the component on content pages and is absent in stock Markra", async () => {
    setCatercowPreviewSiteForTests("https://www.catercow.com");
    const csvTable = vi.fn(async () => ({ markdown: "| a |", problems: [] }));
    const convert = contentCsvSource({ contentRoot: () => root, csvTable, documentPath: () => `${root}/content/a.md` });
    await expect(convert!({ component: "pricing-grid", csvType: true, text: "a" })).resolves.toBe("| a |");
    expect(csvTable).toHaveBeenCalledWith(root, "a", true, "pricing-grid", false);

    const outside = contentCsvSource({ contentRoot: () => root, csvTable, documentPath: () => "/notes/a.md" });
    await expect(outside!({ component: null, csvType: false, text: "a" })).resolves.toBeNull();

    setCatercowPreviewSiteForTests(null);
    expect(contentCsvSource({ contentRoot: () => root, csvTable, documentPath: () => `${root}/content/a.md` })).toBeUndefined();
  });
});
