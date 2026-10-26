import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { frontmatterPreviewPlugin, liveMarkdown, openFrontmatterDetails } from "@markra/editor/codemirror";
import {
  MarkraEditorProvider,
  markraEditorReactBridge,
  useMarkraFrontmatterDetails,
  type MarkraFrontmatterDetailsState,
} from "./index.ts";

const cleanups: Array<() => void> = [];

afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
  document.body.replaceChildren();
});

function setup(doc: string) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc,
      extensions: [liveMarkdown({ plugins: [frontmatterPreviewPlugin()] }), markraEditorReactBridge],
    }),
  });
  const latest: { current: MarkraFrontmatterDetailsState | null } = { current: null };
  function Probe() {
    latest.current = useMarkraFrontmatterDetails();
    return null;
  }
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  act(() => root.render(<MarkraEditorProvider view={view}><Probe /></MarkraEditorProvider>));
  cleanups.push(() => {
    act(() => root.unmount());
    view.destroy();
  });
  return { latest, view };
}

describe("useMarkraFrontmatterDetails", () => {
  it("opens, reads the YAML, and applies only the changed span", () => {
    const source = "---\ntitle: Old\nfeatured: false\n---\n\n# Body\n";
    const { latest, view } = setup(source);
    expect(latest.current?.open).toBe(false);

    act(() => view.dispatch({ effects: openFrontmatterDetails.of(true) }));
    expect(latest.current?.open).toBe(true);
    expect(latest.current?.yaml).toBe("title: Old\nfeatured: false\n");

    const changes: Array<[number, number, string]> = [];
    const original = view.dispatch.bind(view);
    view.dispatch = ((...specs: Parameters<EditorView["dispatch"]>) => {
      const transaction = view.state.update(...(specs as [never]));
      transaction.changes.iterChanges((from, to, _fromB, _toB, inserted) => changes.push([from, to, inserted.toString()]));
      original(transaction);
    }) as EditorView["dispatch"];

    act(() => {
      latest.current?.apply("title: New\nfeatured: false\n");
    });
    expect(view.state.doc.toString()).toBe(source.replace("Old", "New"));
    expect(changes).toEqual([[11, 14, "New"]]);

    act(() => {
      latest.current?.close();
    });
    expect(latest.current?.open).toBe(false);
  });

  it("is closed without YAML front matter", () => {
    const { latest, view } = setup("# Body\n");
    act(() => view.dispatch({ effects: openFrontmatterDetails.of(true) }));
    expect(latest.current?.open).toBe(false);
    expect(latest.current?.yaml).toBeNull();
  });
});
