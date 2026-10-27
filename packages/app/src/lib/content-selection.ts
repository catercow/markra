// Which changed files "Save to studio" publishes. Anything not ticked stays on
// this computer as a draft.

import { pageImageFolder, type PageCollection } from "./cowmark-pages";

/** The folder shared by several posts; it never travels with one page. */
const sharedImageFolder = "shared";

/**
 * Changed files in a page's image folder (`content/blog/x.md` → `public/blog/x/…`), which travel
 * with it. Not `public/blog/shared/`.
 */
export function companionPaths(page: string, changed: readonly string[], collections: readonly PageCollection[] = []) {
  const imageFolder = pageImageFolder(collections, page);
  if (!imageFolder || imageFolder.split("/").pop() === sharedImageFolder) return [];
  const folder = `public${imageFolder}/`;
  return changed.filter((path) => path.startsWith(folder));
}

/** Ticks or unticks `path`; a page brings its image folder's changed files along. */
export function toggleSelection(
  selected: ReadonlySet<string>,
  path: string,
  changed: readonly string[],
  collections: readonly PageCollection[] = []
) {
  const next = new Set(selected);
  const on = !selected.has(path);
  for (const item of [path, ...companionPaths(path, changed, collections)]) {
    if (on) next.add(item);
    else next.delete(item);
  }
  return next;
}

/** Drops ticked paths that no longer have changes. Returns the same set when nothing changed. */
export function pruneSelection(selected: ReadonlySet<string>, changed: readonly string[]) {
  const present = new Set(changed);
  const kept = [...selected].filter((path) => present.has(path));
  return kept.length === selected.size ? selected : new Set(kept);
}
