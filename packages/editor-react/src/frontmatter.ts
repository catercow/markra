import {
  frontmatterDetailsField,
  openFrontmatterDetails,
  readCodeMirrorFrontmatter,
} from "@markra/editor/codemirror";
import { useCallback, useMemo } from "react";
import { useMarkraEditorContext } from "./MarkraEditorProvider.tsx";

export interface MarkraFrontmatterDetailsState {
  /** True while the host's form is open and the document starts with YAML front matter. */
  readonly open: boolean;
  /** The YAML between the fences, read fresh on every editor revision; null without front matter. */
  readonly yaml: string | null;
  readonly readOnly: boolean;
  /** Replaces the YAML between the fences with `next`, as one undoable edit of only the changed span. */
  readonly apply: (next: string) => boolean;
  readonly close: () => boolean;
}

function commonPrefix(a: string, b: string) {
  let index = 0;
  while (index < a.length && index < b.length && a[index] === b[index]) index += 1;
  return index;
}

function commonSuffix(a: string, b: string, prefix: number) {
  let index = 0;
  while (
    index < a.length - prefix &&
    index < b.length - prefix &&
    a[a.length - 1 - index] === b[b.length - 1 - index]
  ) {
    index += 1;
  }
  return index;
}

export function useMarkraFrontmatterDetails(): MarkraFrontmatterDetailsState {
  const { revision, view } = useMarkraEditorContext();

  const current = useMemo(() => {
    const range = view ? readCodeMirrorFrontmatter(view.state.doc.toString()) : null;
    const yaml = range?.kind === "yaml" ? view!.state.sliceDoc(range.contentFrom, range.contentTo) : null;
    return {
      open: Boolean(view?.state.field(frontmatterDetailsField, false)) && yaml !== null,
      readOnly: view?.state.readOnly ?? true,
      yaml,
    };
  }, [revision, view]);

  const apply = useCallback((next: string) => {
    const range = view ? readCodeMirrorFrontmatter(view.state.doc.toString()) : null;
    if (!view || !range || range.kind !== "yaml" || view.state.readOnly) return false;
    const previous = view.state.sliceDoc(range.contentFrom, range.contentTo);
    if (previous === next) return true;
    const prefix = commonPrefix(previous, next);
    const suffix = commonSuffix(previous, next, prefix);
    view.dispatch({
      changes: {
        from: range.contentFrom + prefix,
        insert: next.slice(prefix, next.length - suffix),
        to: range.contentTo - suffix,
      },
      userEvent: "input.frontmatter",
    });
    return true;
  }, [view]);

  const close = useCallback(() => {
    if (!view || !view.state.field(frontmatterDetailsField, false)) return false;
    view.dispatch({ effects: openFrontmatterDetails.of(false) });
    return true;
  }, [view]);

  return useMemo(() => ({ ...current, apply, close }), [apply, close, current]);
}
