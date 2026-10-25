import {
  mdcBlockAtSelection,
  mdcPropsChanges,
  openMdcBlock,
  openMdcPropsEditor,
  readMdcPropsText,
  type MdcBlock,
  type MdcPropsText,
} from "@markra/editor/codemirror";
import { useCallback, useMemo } from "react";
import { useMarkraEditorContext } from "./MarkraEditorProvider.tsx";

export interface MarkraMdcPropsEditorState {
  /** The block whose props form is open, read fresh on every editor revision. */
  readonly block: MdcBlock | null;
  /** The block's attribute and YAML text; null while no form is open. */
  readonly text: MdcPropsText | null;
  readonly readOnly: boolean;
  /** Writes new props text into the open block as one undoable edit. */
  readonly apply: (next: MdcPropsText) => boolean;
  readonly close: () => boolean;
  /** Opens the form for the innermost component around the caret. */
  readonly openAtSelection: () => boolean;
}

export function useMarkraMdcPropsEditor(): MarkraMdcPropsEditorState {
  const { revision, view } = useMarkraEditorContext();

  const current = useMemo(() => {
    const block = view ? openMdcBlock(view.state) : null;
    return {
      block,
      readOnly: view?.state.readOnly ?? true,
      text: view && block ? readMdcPropsText(view.state, block) : null,
    };
  }, [revision, view]);

  const apply = useCallback((next: MdcPropsText) => {
    // Re-read the block so positions match the state the edit applies to.
    const block = view ? openMdcBlock(view.state) : null;
    if (!view || !block || view.state.readOnly) return false;
    const changes = mdcPropsChanges(view.state, block, next);
    if (changes.length > 0) view.dispatch({ changes, userEvent: "input.mdc-props" });
    return true;
  }, [view]);

  const close = useCallback(() => {
    if (!view || !openMdcBlock(view.state)) return false;
    view.dispatch({ effects: openMdcPropsEditor.of(null) });
    return true;
  }, [view]);

  const openAtSelection = useCallback(() => {
    const block = view ? mdcBlockAtSelection(view.state) : null;
    if (!view || !block) return false;
    view.dispatch({ effects: openMdcPropsEditor.of(block.open.from) });
    return true;
  }, [view]);

  return useMemo(
    () => ({ ...current, apply, close, openAtSelection }),
    [apply, close, current, openAtSelection],
  );
}
