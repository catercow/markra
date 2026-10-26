import { describe, expect, it, vi } from "vitest";
import { contentImageBaseName, pickEncoding, planImageImport, saveContentImage } from "./cowmark-images";

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

  it("converts HEIC to JPEG", () => {
    expect(planImageImport({ ...photo, height: 100, size: 10, type: "image/heic", width: 100 })).toMatchObject({
      action: "encode",
      outputs: [{ type: "image/jpeg" }],
    });
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
  it("saves into the page's folder with a clean name", async () => {
    const saveLocalImage = vi.fn(async () => ({ alt: "x", src: "my-post/team-lunch.jpg" }));
    const image = new File([new Uint8Array([1])], "Team Lunch.jpeg", { type: "image/jpeg" });
    const saved = await saveContentImage({
      documentPath: "/r/content/blog/my-post.md",
      image,
      prepare: async (file) => file,
      saveLocalImage,
    });
    expect(saved).toEqual({ alt: "", src: "my-post/team-lunch.jpg" });
    expect(saveLocalImage).toHaveBeenCalledWith({
      documentPath: "/r/content/blog/my-post.md",
      fileName: "team-lunch.jpg",
      folder: "${filename}",
      image,
    });
  });

  it("refuses types the site can't show", async () => {
    await expect(
      saveContentImage({
        documentPath: "/r/content/blog/a.md",
        image: new File([new Uint8Array([1])], "a.tiff", { type: "image/tiff" }),
        prepare: async (file) => file,
        saveLocalImage: vi.fn(),
      })
    ).rejects.toThrow(/can't be used/u);
  });
});
