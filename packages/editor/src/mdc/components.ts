// The component list from a workspace's `schema/components.json`, as the editor uses it to
// offer components for insertion. Nothing here names a component: everything comes from the schema.

import type { MdcComponentSchema } from "./form-model.ts";

/** A column of a table component's slot (`input.columns` in the schema). */
export interface MdcInputColumn {
  /** The canonical header text. */
  name: string;
  aliases?: readonly string[] | null;
  required?: boolean | null;
  /** `text`, `price`, `code` or `link`. */
  format?: string | null;
  markdown?: boolean | null;
}

/** What goes in a component's slot: one Markdown table, or Markdown images. */
export type MdcComponentInput =
  | { kind: "table"; columns: readonly MdcInputColumn[] }
  | { kind: "images" };

/** Structurally compatible with `MdcComponentSchema` from `@markra/shared`. */
export interface MdcCatalogComponent extends MdcComponentSchema {
  /** Vue component name, e.g. `AddressCta`. */
  component?: string | null;
  group?: string | null;
  purpose?: string | null;
  /** Whether the component renders a default slot body. */
  content?: boolean | null;
  example?: { markdown: string } | null;
  input?: MdcComponentInput | null;
}

export interface MdcComponentGroup {
  /** The schema's `group`; empty for components without one. */
  group: string;
  components: MdcCatalogComponent[];
}

/**
 * Components by `group`, groups in the order the schema first lists them (ungrouped last),
 * components by name within a group.
 */
export function groupMdcComponents(components: readonly MdcCatalogComponent[]): MdcComponentGroup[] {
  const groups = new Map<string, MdcCatalogComponent[]>();
  for (const component of components) {
    const group = component.group?.trim() ?? "";
    groups.set(group, [...(groups.get(group) ?? []), component]);
  }

  return [...groups.entries()]
    .sort(([left], [right]) => Number(left === "") - Number(right === ""))
    .map(([group, members]) => ({
      components: [...members].sort((left, right) => left.name.localeCompare(right.name)),
      group,
    }));
}

const blockOpener = /^[ \t]*:{2,}[A-Za-z]/u;

/**
 * `| Item | Price |` with a `| --- |` row and one empty row, written the way CowMark formats
 * tables (`render_table` in cowmark-lint): one space around each cell, no padding.
 */
export function mdcTableSkeleton(columns: readonly MdcInputColumn[]): string {
  const row = (cells: string[]) => `| ${cells.join(" | ")} |`;
  return [
    row(columns.map((column) => column.name)),
    row(columns.map(() => "---")),
    // Two spaces, so the caret can sit between them and typing comes out as `| text |`.
    `|${columns.map(() => "  ").join("|")}|`,
  ].join("\n");
}

/** A gallery row to replace: images on one line form a row, a blank line starts the next. */
export const MDC_GALLERY_SKELETON = "![Photo description](/blog/photo-1.jpg) ![Photo description](/blog/photo-2.jpg)";

/**
 * The Markdown inserted for a component: a ready-to-fill body for table and image components,
 * else its schema example when that is a block, otherwise a bare block, with an empty body line
 * when the component renders content.
 */
export function mdcComponentSnippet(component: MdcCatalogComponent): string {
  const input = component.input;
  if (input?.kind === "table" && input.columns.length > 0) {
    return `::${component.name}\n${mdcTableSkeleton(input.columns)}\n::`;
  }
  if (input?.kind === "images") return `::${component.name}\n${MDC_GALLERY_SKELETON}\n::`;
  const example = component.example?.markdown.replace(/^\s*\n/u, "").trimEnd();
  if (example && blockOpener.test(example)) return example;
  return component.content === false ? `::${component.name}\n::` : `::${component.name}\n\n::`;
}

/**
 * The smallest shift that keeps every colon run in a snippet different from the colon runs of
 * the blocks it is inserted into, so its closers cannot be read as theirs.
 */
export function mdcColonShift(snippetColons: readonly number[], enclosingColons: readonly number[]) {
  const taken = new Set(enclosingColons);
  let shift = 0;
  while (snippetColons.some((colons) => taken.has(colons + shift))) shift += 1;
  return shift;
}
