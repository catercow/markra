import { act, renderHook, waitFor } from "@testing-library/react";
import { pageImageFolder, type PageCollection } from "../lib/cowmark-pages";
import { openNativeLocalImages, saveNativeContentSiteImage } from "../lib/tauri";
import { resolveContentImageSrc, useCowMarkPages } from "./useCowMarkPages";

vi.mock("../lib/tauri", () => ({
  openNativeLocalImages: vi.fn(),
  saveNativeContentSiteImage: vi.fn(),
}));

const schema = JSON.stringify({
  collections: [
    { name: "blog", folder: "content/blog", url: "/blog/<file name without .md>", fields: [] },
    { name: "cuisines", folder: "content/cuisines", url: "/cuisines/<file name without .md>-catering", fields: [] },
  ],
});

vi.mock("../runtime", () => ({
  getAppRuntime: () => ({ files: { readWorkspaceTextFile: async () => schema } }),
}));

const blog: PageCollection = { fields: [], folder: "content/blog", name: "blog", url: "/blog/<file name without .md>" };

describe("pageImageFolder", () => {
  it("maps a page to its folder under public/ by the collection's URL prefix", () => {
    expect(pageImageFolder([blog], "content/blog/my-post.md")).toBe("/blog/my-post");
    const guides = { fields: [], folder: "content/catering-guides", name: "guides", url: "/guides/<file name without .md>" };
    expect(pageImageFolder([guides], "content/catering-guides/office.md")).toBe("/guides/office");
    // Without the schema, the folder under content/ stands in.
    expect(pageImageFolder([], "content/dishes/pizza.md")).toBe("/dishes/pizza");
  });

  it("is null outside content/", () => {
    expect(pageImageFolder([blog], "README.md")).toBeNull();
    expect(pageImageFolder([blog], "public/blog/a.md")).toBeNull();
    expect(pageImageFolder([blog], "content/blog/photo.jpg")).toBeNull();
  });
});

describe("resolveContentImageSrc", () => {
  const tauriWindow = window as unknown as { __TAURI_INTERNALS__?: unknown };
  beforeEach(() => {
    tauriWindow.__TAURI_INTERNALS__ = { convertFileSrc: (path: string) => `asset://${path}` };
  });
  afterEach(() => {
    delete tauriWindow.__TAURI_INTERNALS__;
  });

  it("resolves site paths from public/ for pages in the content folder", () => {
    expect(resolveContentImageSrc("/repo", "/repo/content/blog/a.md", "/blog/a/photo.jpg")).toBe(
      "asset:///repo/public/blog/a/photo.jpg"
    );
    expect(resolveContentImageSrc("/repo", "/repo/content/blog/a.md", "/blog/shared/banner%201.png")).toBe(
      "asset:///repo/public/blog/shared/banner 1.png"
    );
  });

  it("leaves documents outside the content folder as stock Markra resolves them", () => {
    expect(resolveContentImageSrc("/repo", "/notes/a.md", "/Users/me/photo.jpg")).toBe("asset:///Users/me/photo.jpg");
    expect(resolveContentImageSrc("/repo", "/notes/a.md", "photo.jpg")).toBe("asset:///notes/photo.jpg");
  });
});

describe("useCowMarkPages images", () => {
  beforeEach(() => {
    vi.mocked(saveNativeContentSiteImage).mockReset();
    vi.mocked(openNativeLocalImages).mockReset();
  });

  it("saves a page's image into public/<collection>/<slug>/ and returns its site path", async () => {
    vi.mocked(saveNativeContentSiteImage).mockResolvedValue({ alt: "", src: "/cuisines/thai/curry.jpg" });
    const refreshTree = vi.fn(async () => {});
    const { result } = renderHook(() => useCowMarkPages({ onImageError: vi.fn(), refreshTree, rootPath: "/repo" }));
    await waitFor(() => expect(result.current?.collections).toHaveLength(2));

    const image = new File([new Uint8Array([1])], "Curry.svg", { type: "image/svg+xml" });
    let src = "";
    await act(async () => {
      src = await result.current!.saveImage("/repo/content/cuisines/thai.md", image);
    });

    expect(src).toBe("/cuisines/thai/curry.jpg");
    expect(saveNativeContentSiteImage).toHaveBeenCalledWith({
      fileName: "curry.svg",
      folder: "cuisines/thai",
      image,
      root: "/repo",
    });
    expect(refreshTree).toHaveBeenCalledWith("/repo/content/cuisines/thai.md");
    expect(result.current!.imageFolder("/repo/content/blog/my-post.md")).toBe("/blog/my-post");
    expect(result.current!.imageFolder("/repo/README.md")).toBeNull();
    expect(result.current!.imageFolder("/elsewhere/content/blog/a.md")).toBeNull();
  });

  it("refuses files outside content/ and reports picker failures", async () => {
    const onImageError = vi.fn();
    const { result } = renderHook(() => useCowMarkPages({ onImageError, refreshTree: vi.fn(), rootPath: "/repo" }));
    const image = new File([new Uint8Array([1])], "a.svg", { type: "image/svg+xml" });
    await expect(result.current!.saveImage("/repo/README.md", image)).rejects.toThrow(/content/u);
    expect(saveNativeContentSiteImage).not.toHaveBeenCalled();

    vi.mocked(openNativeLocalImages).mockResolvedValue([image]);
    vi.mocked(saveNativeContentSiteImage).mockRejectedValue(new Error("disk full"));
    let picked: string | null = "x";
    await act(async () => {
      picked = await result.current!.pickImage("/repo/content/blog/a.md");
    });
    expect(picked).toBeNull();
    expect(onImageError).toHaveBeenCalled();
  });

  it("is null outside CowMark", () => {
    const { result } = renderHook(() => useCowMarkPages({ onImageError: vi.fn(), refreshTree: vi.fn(), rootPath: null }));
    expect(result.current).toBeNull();
  });
});
