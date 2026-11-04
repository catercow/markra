import { describe, expect, it, vi } from "vitest";
import { budgetSteps, contentImageBaseName, pickEncoding, planImageImport, saveContentImage } from "./cowmark-images";

describe("contentImageBaseName", () => {
  it("cleans the image's name", () => {
    expect(contentImageBaseName("IMG_1234 Final (2).JPG", "/r/content/blog/my-post.md")).toBe("img-1234-final-2");
    expect(contentImageBaseName("Screenshot 2026-10-26 at 3.14.15 PM.png", "/r/content/blog/my-post.md")).toBe(
      "screenshot-2026-10-26-at-3-14-15-pm"
    );
  });

  it("names generic pastes after the page", () => {
    expect(contentImageBaseName("image.png", "/r/content/blog/my-post.md")).toBe("my-post-image");
    expect(contentImageBaseName("", "C:\\r\\content\\blog\\my-post.md")).toBe("my-post-image");
  });
});

describe("planImageImport", () => {
  const photo = { hasAlpha: false, height: 3024, size: 3_500_000, type: "image/jpeg", width: 4032 };

  it("scales large photos to 2400 px on the longest side", () => {
    expect(planImageImport(photo)).toEqual({
      action: "encode",
      height: 1800,
      onlyIfSmaller: false,
      outputs: [{ quality: 0.85, type: "image/jpeg" }],
      width: 2400,
    });
  });

  it("leaves SVG, GIF and small images alone", () => {
    expect(planImageImport({ ...photo, type: "image/svg+xml" })).toEqual({ action: "keep" });
    expect(planImageImport({ ...photo, type: "image/gif" })).toEqual({ action: "keep" });
    expect(planImageImport({ ...photo, height: 900, size: 300_000, width: 1200 })).toEqual({ action: "keep" });
  });

  it("keeps PNG for transparent graphics", () => {
    const plan = planImageImport({ ...photo, hasAlpha: true, type: "image/png" });
    expect(plan.action === "encode" && plan.outputs).toEqual([{ type: "image/png" }]);
  });

  it("tries JPEG for big opaque PNGs only when it pays off", () => {
    const resized = planImageImport({ ...photo, type: "image/png" });
    expect(resized.action === "encode" && resized.outputs.map((output) => output.type)).toEqual(["image/png", "image/jpeg"]);
    const small = planImageImport({ ...photo, height: 1000, size: 2_000_000, type: "image/png", width: 1000 });
    expect(small).toMatchObject({ action: "encode", onlyIfSmaller: true, outputs: [{ type: "image/jpeg" }] });
  });

  it("re-encodes any image over the 1 MB target, such as a large transparent logo", () => {
    const logo = { hasAlpha: true, height: 1600, size: 5_400_000, type: "image/png", width: 1600 };
    expect(planImageImport(logo)).toMatchObject({ action: "encode", onlyIfSmaller: true, outputs: [{ type: "image/png" }] });
    expect(planImageImport({ ...photo, height: 1500, size: 1_200_000, width: 2000 })).toMatchObject({ action: "encode" });
  });

  it("converts HEIC to JPEG", () => {
    expect(planImageImport({ ...photo, height: 100, size: 10, type: "image/heic", width: 100 })).toMatchObject({
      action: "encode",
      outputs: [{ type: "image/jpeg" }],
    });
  });
});

describe("budgetSteps", () => {
  it("lowers JPEG quality first, then scales down in 80% steps to 1200 px", () => {
    expect(budgetSteps(2400, 1600, true)).toEqual([
      { height: 1600, quality: 0.75, width: 2400 },
      { height: 1600, quality: 0.65, width: 2400 },
      { height: 1280, quality: 0.75, width: 1920 },
      { height: 1024, quality: 0.75, width: 1536 },
      { height: 819, quality: 0.75, width: 1229 },
    ]);
  });

  it("only scales PNGs down, and leaves small images as they are", () => {
    expect(budgetSteps(1600, 1600, false)).toEqual([{ height: 1280, width: 1280 }]);
    expect(budgetSteps(1000, 800, false)).toEqual([]);
  });
});

describe("pickEncoding", () => {
  const blob = (size: number, type: string) => ({ blob: new Blob([new Uint8Array(size)], { type }), type });

  it("prefers JPEG only when it is under half the PNG", () => {
    expect(pickEncoding([blob(100, "image/png"), blob(60, "image/jpeg")], null)?.type).toBe("image/png");
    expect(pickEncoding([blob(100, "image/png"), blob(40, "image/jpeg")], null)?.type).toBe("image/jpeg");
  });

  it("keeps the original when nothing is smaller", () => {
    expect(pickEncoding([blob(100, "image/jpeg")], new Blob([new Uint8Array(90)]))).toBeNull();
  });
});

describe("saveContentImage", () => {
  it("saves into the page's image folder under public/ with a clean name", async () => {
    const saveSiteImage = vi.fn(async () => ({ alt: "x", src: "/blog/my-post/team-lunch.jpg" }));
    const image = new File([new Uint8Array([1])], "Team Lunch.jpeg", { type: "image/jpeg" });
    const saved = await saveContentImage({
      documentPath: "/r/content/blog/my-post.md",
      image,
      imageFolder: "/blog/my-post",
      prepare: async (file) => file,
      rootPath: "/r",
      saveSiteImage,
    });
    expect(saved).toEqual({ alt: "", src: "/blog/my-post/team-lunch.jpg" });
    expect(saveSiteImage).toHaveBeenCalledWith({
      fileName: "team-lunch.jpg",
      folder: "blog/my-post",
      image,
      replace: true,
      root: "/r",
    });
  });

  it("replaces an image of the same name, but never one a generic paste name could clash with", async () => {
    const saveSiteImage = vi.fn(async () => ({ alt: "", src: "/blog/my-post/my-post-image.png" }));
    await saveContentImage({
      documentPath: "/r/content/blog/my-post.md",
      image: new File([new Uint8Array([1])], "image.png", { type: "image/png" }),
      imageFolder: "/blog/my-post",
      prepare: async (file) => file,
      rootPath: "/r",
      saveSiteImage,
    });
    expect(saveSiteImage).toHaveBeenCalledWith(expect.objectContaining({ fileName: "my-post-image.png", replace: false }));
  });

  it("refuses a GIF over 5 MB", async () => {
    const saveSiteImage = vi.fn();
    await expect(
      saveContentImage({
        documentPath: "/r/content/blog/a.md",
        image: new File([new Uint8Array(5_100_000)], "demo.gif", { type: "image/gif" }),
        imageFolder: "/blog/a",
        prepare: async (file) => file,
        rootPath: "/r",
        saveSiteImage,
      })
    ).rejects.toThrow(/5\.1 MB, over the 5 MB limit/u);
    expect(saveSiteImage).not.toHaveBeenCalled();
  });

  it("refuses types the site can't show", async () => {
    await expect(
      saveContentImage({
        documentPath: "/r/content/blog/a.md",
        image: new File([new Uint8Array([1])], "a.tiff", { type: "image/tiff" }),
        imageFolder: "/blog/a",
        prepare: async (file) => file,
        rootPath: "/r",
        saveSiteImage: vi.fn(),
      })
    ).rejects.toThrow(/can't be used/u);
  });
});
