// The component list from a workspace's `schema/components.json`, as the editor uses it to
// offer components for insertion. Nothing here names a component: everything comes from the schema.

import type { MdcComponentSchema } from "./form-model.ts";

/** Structurally compatible with `MdcComponentSchema` from `@markra/shared`. */
export interface MdcCatalogComponent extends MdcComponentSchema {
  /** Vue component name, e.g. `AddressCta`. */
  component?: string | null;
  group?: string | null;
  purpose?: string | null;
  /** Whether the component renders a default slot body. */
  content?: boolean | null;
  example?: { markdown: string } | null;
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
 * The Markdown inserted for a component: its schema example when that is a block, otherwise a
 * bare block, with an empty body line when the component renders content.
 */
export function mdcComponentSnippet(component: MdcCatalogComponent): string {
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
