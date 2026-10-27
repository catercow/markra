// CowMark page details: the front matter form for a content file, generated from the content
// repository's `schema/collections.json`, and the "New post" template. See docs/mdc.md.

import { readFrontmatterFields, writeFrontmatterField } from "@markra/editor";

export const collectionsSchemaRelativePath = "schema/collections.json";

export type PageField = {
  name: string;
  /** "string", "boolean", "number", "{ slug: string, name: string }", "list of …" */
  type: string;
  required: boolean;
  default?: unknown;
};

export type PageCollection = {
  name: string;
  /** Relative to the repository root, e.g. "content/blog". */
  folder: string;
  url?: string;
  fields: PageField[];
};

export type PageFieldKind = "text" | "textarea" | "boolean" | "number" | "date" | "image" | "yaml";

/** String values each key already has across a collection's files, most used first. */
export type FieldValues = Record<string, string[]>;

function normalizePath(path: string) {
  return path.replace(/\\/gu, "/").replace(/\/+$/u, "");
}

export function parseCollectionsSchema(text: string | null): PageCollection[] {
  if (!text) return [];
  try {
    const collections = (JSON.parse(text) as { collections?: unknown }).collections;
    if (!Array.isArray(collections)) return [];
    return collections.flatMap((collection) => {
      if (!collection || typeof collection.name !== "string" || typeof collection.folder !== "string") return [];
      const fields = Array.isArray(collection.fields) ? collection.fields : [];
      return [{
        fields: fields
          .filter((field: unknown): field is PageField =>
            Boolean(field) && typeof (field as PageField).name === "string")
          .map((field: PageField) => ({
            ...("default" in field ? { default: field.default } : {}),
            name: field.name,
            required: field.required === true,
            type: typeof field.type === "string" ? field.type : "",
          })),
        folder: normalizePath(collection.folder),
        name: collection.name,
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

const textareaNames = /(^|_)(description|excerpt|summary)$/u;
const dateNames = /(_at|_on|^date)$/u;
const imageNames = /(image|photo|thumbnail|logo)$/u;

/** The control for a field. Anything that is not a scalar is edited as YAML. */
export function pageFieldKind(field: PageField): PageFieldKind {
  const type = field.type.trim();
  if (type === "boolean") return "boolean";
  if (type === "number") return "number";
  if (type === "date") return "date";
  if (type === "string") {
    if (dateNames.test(field.name)) return "date";
    if (imageNames.test(field.name)) return "image";
    return textareaNames.test(field.name) ? "textarea" : "text";
  }
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

// --- New posts -----------------------------------------------------------------------------

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

/** A new file's text: the title and the collection's other required fields, then a blank body. */
export function newPageText(collection: PageCollection, { now = new Date(), title }: { now?: Date; title: string }) {
  let yaml = "";
  const set = (key: string, value: unknown) => {
    yaml = writeFrontmatterField(yaml, key, value).text;
  };
  for (const field of collection.fields) {
    const kind = pageFieldKind(field);
    if (field.name === "title") set("title", title);
    else if (!field.required) continue;
    else if (kind === "date") set(field.name, localDate(now));
    else if (kind === "boolean") set(field.name, field.default === true);
    else if (kind === "text" || kind === "textarea" || kind === "image") set(field.name, "");
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
