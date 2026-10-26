// Which changed files "Save to studio" publishes. Anything not ticked stays on
// this computer as a draft.

/** Changed files in a page's own folder (`content/blog/x.md` → `content/blog/x/…`), which travel with it. */
export function companionPaths(page: string, changed: readonly string[]) {
  if (!page.endsWith(".md")) return [];
  const folder = `${page.slice(0, -".md".length)}/`;
  return changed.filter((path) => path.startsWith(folder));
}

/** Ticks or unticks `path`; a page brings its folder's changed files along. */
export function toggleSelection(selected: ReadonlySet<string>, path: string, changed: readonly string[]) {
  const next = new Set(selected);
  const on = !selected.has(path);
  for (const item of [path, ...companionPaths(path, changed)]) {
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
