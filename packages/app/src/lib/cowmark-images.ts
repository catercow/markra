// CowMark: images added to a content page are stored in the site's public/ folder under the
// page's image folder (content/blog/my-post.md → public/blog/my-post/photo.jpg) and referenced
// by site path (`/blog/my-post/photo.jpg`); see `pageImageFolder`.
// Large photos are scaled down first so phone pictures don't bloat the content repository.

import type { SavedNativeClipboardImage, SaveNativeContentSiteImageInput } from "./tauri";
import { slugify } from "./cowmark-pages";

// The stored image is the original the site resizes for each slot (@nuxt/image), so these
// only keep originals to a sensible size for the content repository.
export const contentImageMaxSide = 3000;
/** What an image should come to at most; a larger one is compressed harder, then scaled down. */
export const contentImageTargetBytes = 2_000_000;
/** Scaling down to meet the target stops here. */
export const contentImageMinSide = 1600;
/** GIFs can't be re-encoded here, so a larger one is refused. */
export const contentGifMaxBytes = 5_000_000;
const lossyQuality = 0.85;
const reencodeAboveBytes = contentImageTargetBytes;
const pngToJpegAboveBytes = contentImageTargetBytes;
const genericNames = /^(image|img|photo|picture|pasted.*|clipboard.*|untitled.*|blob|download.*)$/u;

export function documentStem(documentPath: string) {
  const name = documentPath.replace(/\\/gu, "/").split("/").pop() ?? "";
  return name.replace(/\.[^.]+$/u, "");
}

/** A lowercase, hyphenated file name without extension: the image's own name, or `<page>-image`. */
export function contentImageBaseName(fileName: string, documentPath: string) {
  return contentImageName(fileName, documentPath).base;
}

/** `own`: the name came from the image itself, so a file with that name is the same image. */
export function contentImageName(fileName: string, documentPath: string) {
  const base = slugify(fileName.replace(/\.[^.]*$/u, ""));
  if (base && !genericNames.test(base)) return { base, own: true };
  return { base: `${slugify(documentStem(documentPath)) || "page"}-image`, own: false };
}

const extensions: Record<string, string> = {
  "image/avif": "avif",
  "image/gif": "gif",
  "image/jpeg": "jpg",
  "image/png": "png",
  "image/svg+xml": "svg",
  "image/webp": "webp",
};

export type ImageFacts = {
  hasAlpha: boolean;
  height: number;
  size: number;
  type: string;
  width: number;
};

export type ImageImportPlan =
  | { action: "keep" }
  | {
      action: "encode";
      height: number;
      /** Encodings to try, in order; the smallest acceptable result wins. */
      outputs: Array<{ quality?: number; type: string }>;
      /** Keep the original when no encoding is smaller (nothing to scale down). */
      onlyIfSmaller: boolean;
      width: number;
    };

function scaled(width: number, height: number) {
  const longest = Math.max(width, height);
  if (longest <= contentImageMaxSide) return { height, resized: false, width };
  const ratio = contentImageMaxSide / longest;
  return { height: Math.max(1, Math.round(height * ratio)), resized: true, width: Math.max(1, Math.round(width * ratio)) };
}

/** What to do with an imported image. SVG and GIF (animation) are never touched. */
export function planImageImport(facts: ImageFacts): ImageImportPlan {
  const type = facts.type.toLowerCase();
  if (type === "image/svg+xml" || type === "image/gif") return { action: "keep" };
  const { height, resized, width } = scaled(facts.width, facts.height);
  const heic = type === "image/heic" || type === "image/heif";

  let outputs: Array<{ quality?: number; type: string }>;
  if (type === "image/jpeg" || heic) outputs = [{ quality: lossyQuality, type: "image/jpeg" }];
  else if (type === "image/webp") outputs = [{ quality: lossyQuality, type: "image/webp" }];
  else if (facts.hasAlpha) outputs = [{ type: "image/png" }];
  // Opaque PNG/BMP: screenshots and graphics stay PNG; photos become JPEG when that is much smaller.
  else outputs = [{ type: "image/png" }, { quality: lossyQuality, type: "image/jpeg" }];

  if (resized || heic || type === "image/bmp") return { action: "encode", height, onlyIfSmaller: false, outputs, width };
  if (type === "image/png" && !facts.hasAlpha && facts.size > pngToJpegAboveBytes) {
    return { action: "encode", height, onlyIfSmaller: true, outputs: [{ quality: lossyQuality, type: "image/jpeg" }], width };
  }
  if ((type === "image/jpeg" || type === "image/webp") && facts.size > reencodeAboveBytes) {
    return { action: "encode", height, onlyIfSmaller: true, outputs, width };
  }
  // Over the target in any other format (e.g. a large transparent PNG): the target steps apply.
  if (facts.size > contentImageTargetBytes) return { action: "encode", height, onlyIfSmaller: true, outputs, width };
  return { action: "keep" };
}

/**
 * Further encodings to try, in order, while an image is over `contentImageTargetBytes`: lower
 * quality first (lossy formats only), then 80% steps down to `contentImageMinSide`.
 */
export function budgetSteps(width: number, height: number, lossy: boolean) {
  const steps: Array<{ height: number; quality?: number; width: number }> = [];
  if (lossy) steps.push({ height, quality: 0.75, width }, { height, quality: 0.65, width });
  let next = { height, width };
  while (Math.max(next.width, next.height) * 0.8 >= contentImageMinSide) {
    next = { height: Math.round(next.height * 0.8), width: Math.round(next.width * 0.8) };
    steps.push({ ...next, ...(lossy ? { quality: 0.75 } : {}) });
  }
  return steps;
}

/** Picks the encoding to keep: a JPEG only replaces a PNG when it is under half its size. */
export function pickEncoding(candidates: Array<{ blob: Blob; type: string }>, original: Blob | null) {
  const png = candidates.find((candidate) => candidate.type === "image/png");
  let best: { blob: Blob; type: string } | null = null;
  for (const candidate of candidates) {
    if (candidate.type === "image/jpeg" && png && candidate.blob.size > png.blob.size / 2) continue;
    if (!best || candidate.blob.size < best.blob.size) best = candidate;
  }
  if (best && original && best.blob.size >= original.size) return null;
  return best;
}

type Canvas = HTMLCanvasElement | OffscreenCanvas;

function createCanvas(width: number, height: number): Canvas {
  if (typeof OffscreenCanvas !== "undefined") return new OffscreenCanvas(width, height);
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
}

async function encode(canvas: Canvas, type: string, quality?: number): Promise<Blob | null> {
  if ("convertToBlob" in canvas) return canvas.convertToBlob({ quality, type }).catch(() => null);
  return new Promise((resolve) => (canvas as HTMLCanvasElement).toBlob(resolve, type, quality));
}

function hasTransparency(bitmap: ImageBitmap) {
  const side = 256;
  const ratio = Math.min(1, side / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * ratio));
  const height = Math.max(1, Math.round(bitmap.height * ratio));
  const context = createCanvas(width, height).getContext("2d") as CanvasRenderingContext2D | null;
  if (!context) return true;
  context.drawImage(bitmap, 0, 0, width, height);
  const { data } = context.getImageData(0, 0, width, height);
  for (let index = 3; index < data.length; index += 4) if (data[index]! < 255) return true;
  return false;
}

/**
 * The image to store: scaled to at most 3000 px on its longest side, re-encoded when that saves
 * space, then compressed harder or scaled down while it is over `contentImageTargetBytes`.
 * Returns the original when the browser cannot decode it.
 */
export async function prepareContentImage(image: File): Promise<File> {
  const type = image.type.toLowerCase();
  if (type === "image/svg+xml" || type === "image/gif" || typeof createImageBitmap !== "function") return image;

  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(image);
  } catch {
    return image;
  }
  try {
    const lossless = type === "image/png" || type === "image/webp" || type === "image/bmp" || type === "image/avif";
    const plan = planImageImport({
      hasAlpha: lossless ? hasTransparency(bitmap) : false,
      height: bitmap.height,
      size: image.size,
      type,
      width: bitmap.width,
    });
    if (plan.action === "keep") return image;

    const canvas = createCanvas(plan.width, plan.height);
    const context = canvas.getContext("2d") as CanvasRenderingContext2D | null;
    if (!context) return image;
    context.imageSmoothingQuality = "high";
    context.drawImage(bitmap, 0, 0, plan.width, plan.height);

    const candidates: Array<{ blob: Blob; type: string }> = [];
    for (const output of plan.outputs) {
      const blob = await encode(canvas, output.type, output.quality);
      // A browser that cannot write the type returns PNG instead.
      if (blob && blob.type === output.type) candidates.push({ blob, type: output.type });
    }
    let chosen = pickEncoding(candidates, plan.onlyIfSmaller ? image : null);
    if (!chosen && !extensions[type]) return image;
    chosen ??= { blob: image, type };

    if (chosen.blob.size > contentImageTargetBytes) {
      for (const step of budgetSteps(plan.width, plan.height, chosen.type !== "image/png")) {
        const smaller = createCanvas(step.width, step.height);
        const smallerContext = smaller.getContext("2d") as CanvasRenderingContext2D | null;
        if (!smallerContext) break;
        smallerContext.imageSmoothingQuality = "high";
        smallerContext.drawImage(bitmap, 0, 0, step.width, step.height);
        const blob = await encode(smaller, chosen.type, step.quality);
        if (!blob || blob.type !== chosen.type || blob.size >= chosen.blob.size) continue;
        chosen = { blob, type: chosen.type };
        if (blob.size <= contentImageTargetBytes) break;
      }
    }
    if (chosen.blob === image) return image;
    const name = `${image.name.replace(/\.[^.]*$/u, "") || "image"}.${extensions[chosen.type]}`;
    return new File([chosen.blob], name, { type: chosen.type });
  } finally {
    bitmap.close();
  }
}

export type SaveContentImageInput = {
  documentPath: string;
  /** The page's image folder as a site path, e.g. `/blog/my-post` (see `pageImageFolder`). */
  imageFolder: string;
  image: File;
  prepare?: (image: File) => Promise<File>;
  rootPath: string;
  saveSiteImage: (input: SaveNativeContentSiteImageInput) => Promise<SavedNativeClipboardImage>;
};

/** Saves an image into `public/<image folder>/`; `src` is its site path. */
export async function saveContentImage({
  documentPath,
  image,
  imageFolder,
  prepare = prepareContentImage,
  rootPath,
  saveSiteImage,
}: SaveContentImageInput): Promise<SavedNativeClipboardImage> {
  if (image.type.toLowerCase() === "image/gif" && image.size > contentGifMaxBytes) {
    const megabytes = Math.round(image.size / 100_000) / 10;
    throw new Error(`This GIF is ${megabytes} MB, over the 5 MB limit. Use a shorter or smaller GIF, or a video.`);
  }
  const prepared = await prepare(image);
  const extension = extensions[prepared.type.toLowerCase()];
  if (!extension) throw new Error(`Images of type ${prepared.type || "unknown"} can't be used on the site.`);
  const name = contentImageName(image.name, documentPath);
  const saved = await saveSiteImage({
    fileName: `${name.base}.${extension}`,
    folder: imageFolder.replace(/^\/+/u, ""),
    image: prepared,
    replace: name.own,
    root: rootPath,
  });
  return { alt: "", src: saved.src };
}
