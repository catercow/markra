// CowMark page details: the front matter form for a content file, generated from the content
// repository's `schema/collections.json`, and the "New page" template. See docs/mdc.md.

import { readFrontmatterFields, writeFrontmatterField } from "@markra/editor";

export const collectionsSchemaRelativePath = "schema/collections.json";

export type PageField = {
  name: string;
  /** "string", "boolean", "number", "{ slug: string, name: string }", "list of …" */
  type: string;
  required: boolean;
  default?: unknown;
  /** Help text shown under the field. */
  description?: string;
  /** `image`: a string field holds one image, a `list of string` field a list of images. */
  format?: string;
};

export type PageCollection = {
  name: string;
  /** What the site calls a page of this kind, e.g. "Blog post"; the name when the schema has none. */
  label: string;
  /** One sentence about the kind of page. */
  description?: string;
  /** False for a fixed set of pages: New page doesn't offer it. */
  creatable: boolean;
  /** An existing file name (without .md), showing the naming convention. */
  slugExample?: string;
  /** Relative to the repository root, e.g. "content/blog". */
  folder: string;
  url?: string;
  fields: PageField[];
};

export type PageFieldKind = "text" | "textarea" | "boolean" | "number" | "date" | "image" | "images" | "yaml";

/** String values each key already has across a collection's files, most used first. */
export type FieldValues = Record<string, string[]>;

function normalizePath(path: string) {
  return path.replace(/\\/gu, "/").replace(/\/+$/u, "");
}

const imageNames = /(image|photo|thumbnail|logo)$/u;

function optionalText(value: unknown) {
  return typeof value === "string" && value.trim() ? value : undefined;
}

type RawField = Record<string, unknown> & { name: string };

function rawFields(collection: unknown): RawField[] {
  const fields = (collection as { fields?: unknown } | null)?.fields;
  return Array.isArray(fields)
    ? fields.filter((field): field is RawField => Boolean(field) && typeof (field as RawField).name === "string")
    : [];
}

export function parseCollectionsSchema(text: string | null): PageCollection[] {
  if (!text) return [];
  try {
    const collections = (JSON.parse(text) as { collections?: unknown }).collections;
    if (!Array.isArray(collections)) return [];
    // Older schemas don't mark image fields; then the field name decides.
    const marksFormats = collections.some((collection) =>
      rawFields(collection).some((field) => typeof field.format === "string"));
    return collections.flatMap((collection) => {
      if (!collection || typeof collection.name !== "string" || typeof collection.folder !== "string") return [];
      const description = optionalText(collection.description);
      const slugExample = optionalText(collection.slugExample);
      return [{
        creatable: collection.creatable !== false,
        ...(description ? { description } : {}),
        fields: rawFields(collection).map((field): PageField => {
          const type = typeof field.type === "string" ? field.type : "";
          const fieldDescription = optionalText(field.description);
          const format = marksFormats
            ? optionalText(field.format)
            : type.trim() === "string" && imageNames.test(field.name) ? "image" : undefined;
          return {
            ...("default" in field && field.default !== null ? { default: field.default } : {}),
            ...(fieldDescription ? { description: fieldDescription } : {}),
            ...(format ? { format } : {}),
            name: field.name,
            required: field.required === true,
            type,
          };
        }),
        folder: normalizePath(collection.folder),
        label: optionalText(collection.label) ?? collection.name,
        name: collection.name,
        ...(slugExample ? { slugExample } : {}),
        ...(typeof collection.url === "string" ? { url: collection.url } : {}),
      }];
    });
  } catch {
    return [];
  }
}

/** The path below the root, with forward slashes, or null when the file is outside it. */
export function rootRelativePath(rootPath: string, path: string | null | undefined) {
  if (!path) return null;
  const root = `${normalizePath(rootPath)}/`;
  const file = normalizePath(path);
  const insensitive = /^[a-z]:\//iu.test(root);
  const inside = insensitive ? file.toLowerCase().startsWith(root.toLowerCase()) : file.startsWith(root);
  return inside ? file.slice(root.length) : null;
}

/** The collection whose folder holds the file; the deepest folder wins. */
export function collectionForDocument(
  collections: readonly PageCollection[],
  rootPath: string,
  documentPath: string | null | undefined
) {
  const relative = rootRelativePath(rootPath, documentPath);
  if (!relative || !/\.md$/iu.test(relative)) return null;
  return [...collections]
    .filter((collection) => relative.startsWith(`${collection.folder}/`))
    .sort((left, right) => right.folder.length - left.folder.length)[0] ?? null;
}

/**
 * The site path of a page's image folder, served from the root's `public/` folder:
 * `content/blog/my-post.md` → `/blog/my-post`. Follows the collection's URL prefix
 * (`crates/cowmark-lint` `page_image_folder`); null for files outside `content/`.
 */
export function pageImageFolder(collections: readonly PageCollection[], relativePath: string) {
  const path = relativePath.replace(/\\/gu, "/");
  if (!/^content\/.+\.md$/iu.test(path)) return null;
  const withoutExtension = path.replace(/\.md$/iu, "");
  const collection = [...collections]
    .filter((candidate) => withoutExtension.startsWith(`${candidate.folder}/`))
    .sort((left, right) => right.folder.length - left.folder.length)[0];
  const slug = collection ? withoutExtension.slice(collection.folder.length + 1) : "";
  if (!collection || !slug || slug.includes("/")) return `/${withoutExtension.slice("content/".length)}`;
  const open = collection.url?.indexOf("<") ?? -1;
  const fromUrl = open >= 0 ? collection.url!.slice(0, open).replace(/^\/+|\/+$/gu, "") : "";
  const prefix = fromUrl || collection.folder.replace(/^content\//u, "");
  return prefix ? `/${prefix}/${slug}` : `/${slug}`;
}

/** The site path of a file under the root's `public/` folder (`/blog/a/b.jpg`), else null. */
export function publicSitePath(rootPath: string, path: string | null | undefined) {
  const relative = rootRelativePath(rootPath, path);
  return relative && relative.startsWith("public/") && relative.length > "public/".length
    ? `/${relative.slice("public/".length)}`
    : null;
}

const textareaNames = /(^|_)(description|excerpt|summary|intro)$/u;
const dateNames = /(_at|_on|^date)$/u;

/**
 * The control for a field. Anything that is not a scalar or a list of images is edited as
 * YAML. Image fields are the ones the schema marks `format: "image"` (see parseCollectionsSchema
 * for older schemas).
 */
export function pageFieldKind(field: PageField): PageFieldKind {
  const type = field.type.trim();
  const image = field.format === "image";
  if (type === "boolean") return "boolean";
  if (type === "number") return "number";
  if (type === "date") return "date";
  if (type === "string") {
    if (image) return "image";
    if (dateNames.test(field.name)) return "date";
    return textareaNames.test(field.name) ? "textarea" : "text";
  }
  if (image && type === "list of string") return "images";
  return "yaml";
}

/** Recommended maximum length for search results, or null. */
export function pageFieldCharLimit(field: PageField) {
  if (/(^|_)title$/u.test(field.name)) return 60;
  if (/(^|_)description$/u.test(field.name)) return 160;
  return null;
}

/** The value a field shows when its key is missing from the file. */
export function pageFieldDefault(field: PageField) {
  return field.default;
}

// --- Dates ---------------------------------------------------------------------------------

/** Today as YYYY-MM-DD, in local time. */
export function localDate(date: Date) {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** The YYYY-MM-DD part of a stored date, or "". */
export function datePart(value: unknown) {
  return typeof value === "string" ? /^(\d{4}-\d{2}-\d{2})/u.exec(value)?.[1] ?? "" : "";
}

/** The new date as YYYY-MM-DD; an older full timestamp keeps its time and offset. */
export function withDatePart(previous: unknown, date: string) {
  if (typeof previous === "string" && /^\d{4}-\d{2}-\d{2}T/u.test(previous)) {
    return date + previous.slice(10);
  }
  return date;
}

// --- New pages -----------------------------------------------------------------------------

export function slugify(text: string) {
  return text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/gu, "")
    .toLowerCase()
    .replace(/['’]/gu, "")
    .replace(/[^a-z0-9]+/gu, "-")
    .replace(/^-+|-+$/gu, "")
    .slice(0, 80)
    .replace(/-+$/u, "");
}

/**
 * A file name while it is typed: the slug convention (a–z, 0–9, single hyphens), keeping one
 * trailing hyphen so the next word can follow. `slugify` gives the final name.
 */
export function slugDraft(text: string) {
  const slug = slugify(text);
  const trailing = /[^\p{L}\p{N}'’]$/u.test(text);
  return slug && trailing && slug.length < 80 ? `${slug}-` : slug;
}

/** The site path a page will have, from its collection's `url` pattern (cowmark-cli `page_url`). */
export function pageUrl(collection: PageCollection, slug: string) {
  const url = collection.url;
  const open = url ? url.indexOf("<") : -1;
  const close = url ? url.lastIndexOf(">") : -1;
  if (url && open >= 0 && close > open) return `${url.slice(0, open)}${slug}${url.slice(close + 1)}`;
  if (url) return url;
  const prefix = collection.folder.replace(/^content\/?/u, "");
  return prefix ? `/${prefix}/${slug}` : `/${slug}`;
}

/** Collections New page offers: creatable ones, blog first, then in schema order. */
export function creatableCollections(collections: readonly PageCollection[]) {
  const offered = collections.filter((collection) => collection.creatable);
  return [
    ...offered.filter((collection) => collection.name === "blog"),
    ...offered.filter((collection) => collection.name !== "blog"),
  ];
}

const askedKinds = new Set<PageFieldKind>(["text", "textarea", "number", "boolean", "date"]);

function isEmptyValue(value: unknown) {
  return value === undefined
    || value === null
    || (typeof value === "string" && !value.trim())
    || (Array.isArray(value) && value.length === 0)
    || (typeof value === "number" && !Number.isFinite(value));
}

/** Required fields without a default, other than the title: the ones a new page must fill. */
function requiredFields(collection: PageCollection) {
  return collection.fields.filter((field) =>
    field.name !== "title" && field.required && field.default === undefined);
}

/**
 * The required fields the New page dialog asks for, and the ones left to Page details: images
 * (the page's image folder doesn't exist until the page does) and lists or objects.
 */
export function newPageFields(collection: PageCollection) {
  const fields = requiredFields(collection);
  return {
    asked: fields.filter((field) => askedKinds.has(pageFieldKind(field))),
    later: fields.filter((field) => !askedKinds.has(pageFieldKind(field))),
  };
}

/** The dialog's starting values: today for dates, off for switches. */
export function newPageInitialValues(collection: PageCollection, now = new Date()) {
  const values: Record<string, unknown> = {};
  for (const field of newPageFields(collection).asked) {
    const kind = pageFieldKind(field);
    if (kind === "date") values[field.name] = localDate(now);
    else if (kind === "boolean") values[field.name] = false;
  }
  return values;
}

/** Required fields (the title aside) that `values` leaves empty. */
export function missingRequiredFields(collection: PageCollection, values: Readonly<Record<string, unknown>>) {
  return requiredFields(collection).filter((field) => isEmptyValue(values[field.name])).map((field) => field.name);
}

/**
 * A new file's text: the title and the filled `values`, in schema order, then a blank body.
 * Empty values are left out, so an unfilled field never becomes an empty key.
 */
export function newPageText(
  collection: PageCollection,
  { title, values = {} }: { title: string; values?: Readonly<Record<string, unknown>> }
) {
  let yaml = "";
  const set = (key: string, value: unknown) => {
    yaml = writeFrontmatterField(yaml, key, value).text;
  };
  if (!collection.fields.some((field) => field.name === "title")) set("title", title);
  for (const field of collection.fields) {
    if (field.name === "title") set("title", title);
    else if (!isEmptyValue(values[field.name])) set(field.name, values[field.name]);
  }
  return `---\n${yaml}---\n\n`;
}

// --- Values already used -------------------------------------------------------------------

function frontmatterYaml(text: string) {
  const match = /^﻿?---[ \t]*\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/u.exec(text);
  return match ? match[1]! : null;
}

/**
 * Values each top-level key already has across `texts` (Markdown files). Only keys where some
 * value repeats are kept: those are the ones with a set of choices (a category, a layout).
 */
export function collectFieldValues(texts: Iterable<string>): FieldValues {
  const counts = new Map<string, Map<string, number>>();
  for (const text of texts) {
    const yaml = frontmatterYaml(text);
    if (!yaml) continue;
    for (const [key, value] of Object.entries(readFrontmatterFields(yaml).values)) {
      if (typeof value !== "string" || !value.trim()) continue;
      const values = counts.get(key) ?? new Map<string, number>();
      values.set(value, (values.get(value) ?? 0) + 1);
      counts.set(key, values);
    }
  }
  const result: FieldValues = {};
  for (const [key, values] of counts) {
    if (![...values.values()].some((count) => count > 1)) continue;
    result[key] = [...values.entries()]
      .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
      .map(([value]) => value);
  }
  return result;
}
