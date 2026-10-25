import {
  contentFsPathFor,
  contentPreviewConfigPathForRoot,
  messageForEvent,
  orderContentPreviewEvents,
  parseContentPreviewConfig
} from "./content-preview";

describe("content preview", () => {
  it("reads the site from preview.json", () => {
    expect(contentPreviewConfigPathForRoot("/work/marketing-content")).toBe("/work/marketing-content/preview.json");
    expect(parseContentPreviewConfig('{"site":"https://preview.example.com/"}')).toEqual({
      origin: "https://preview.example.com",
      site: "https://preview.example.com/"
    });
    expect(parseContentPreviewConfig('{"site":"javascript:alert(1)"}')).toBeNull();
    expect(parseContentPreviewConfig('{"site":42}')).toBeNull();
    expect(parseContentPreviewConfig("not json")).toBeNull();
    expect(parseContentPreviewConfig(null)).toBeNull();
  });

  it("maps documents under content/ to the site's paths", () => {
    expect(contentFsPathFor("/work/mc", "/work/mc/content/blog/post.md")).toBe("blog/post.md");
    expect(contentFsPathFor("/work/mc/", "/work/mc/content/data/x.yml")).toBe("data/x.yml");
    expect(contentFsPathFor("C:\\Users\\a\\mc", "c:\\users\\a\\mc\\content\\blog\\post.md")).toBe("blog/post.md");
    expect(contentFsPathFor("/work/mc", "/work/mc/README.md")).toBeNull();
    expect(contentFsPathFor("/work/mc", "/work/mc/content/blog/image.png")).toBeNull();
    expect(contentFsPathFor("/work/mc", "/work/mcx/content/blog/post.md")).toBeNull();
    expect(contentFsPathFor("/work/mc", null)).toBeNull();
  });

  it("builds draft-preview messages", async () => {
    const bytes = new Uint8Array([1, 2, 3]).buffer;
    const readMedia = vi.fn(async () => bytes);

    await expect(messageForEvent({ kind: "draft", fsPath: "blog/a.md", content: "# A" }, true, readMedia)).resolves.toEqual({
      message: { type: "catercow:draft", fsPath: "blog/a.md", content: "# A", navigate: true },
      transfer: []
    });
    await expect(messageForEvent({ kind: "remove", fsPath: "blog/a.md" }, true, readMedia)).resolves.toEqual({
      message: { type: "catercow:draft:remove", fsPath: "blog/a.md" },
      transfer: []
    });
    await expect(messageForEvent({ kind: "media", path: "/blog/a.png", mime: "image/png" }, false, readMedia)).resolves.toEqual({
      message: { type: "catercow:draft:media", path: "/blog/a.png", bytes, mime: "image/png" },
      transfer: [bytes]
    });
    expect(readMedia).toHaveBeenCalledWith("/blog/a.png");
    await expect(messageForEvent({ kind: "media", path: "/blog/gone.png", mime: null }, false, readMedia)).resolves.toEqual({
      message: { type: "catercow:draft:media", path: "/blog/gone.png", bytes: null, mime: "" },
      transfer: []
    });
    await expect(messageForEvent({ kind: "skipped", path: "/big.png", reason: "too big" }, false, readMedia)).resolves.toBeNull();
  });

  it("sends images before the pages that use them", () => {
    expect(orderContentPreviewEvents([
      { kind: "draft", fsPath: "blog/a.md", content: "" },
      { kind: "media", path: "/blog/a.png", mime: "image/png" }
    ]).map((event) => event.kind)).toEqual(["media", "draft"]);
  });
});
