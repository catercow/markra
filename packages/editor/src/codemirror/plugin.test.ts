import { EditorState, Facet, type Extension } from "@codemirror/state";
import { EditorView, runScopeHandlers } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import {
  defineMarkraPlugin,
  listMarkraUi,
  liveMarkdown,
  markraPlugins,
  runMarkraCommand,
  searchMarkraUi,
} from "./index.ts";

import "./dom.test-support.ts";

const views: EditorView[] = [];

function createView(extensions: Extension[]) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: "Synthetic document",
      extensions,
    }),
  });
  views.push(view);
  return view;
}

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.replaceChildren();
});

describe("Markra plugins", () => {
  it("composes CodeMirror extensions and exposes command-backed UI", () => {
    const syntheticFacet = Facet.define<string, string>({
      combine: (values) => values.join(","),
    });
    const plugin = defineMarkraPlugin({
      id: "markra.synthetic",
      extension: syntheticFacet.of("installed"),
      commands: [
        {
          id: "synthetic.append",
          label: "Append synthetic marker",
          isActive: (view) => view.state.doc.toString().endsWith("!"),
          run(view) {
            view.dispatch({
              changes: { from: view.state.doc.length, insert: "!" },
            });
            return true;
          },
        },
      ],
      ui: [
        {
          command: "synthetic.append",
          group: "insert",
          icon: "sparkles",
          order: 20,
          placement: "toolbar",
        },
      ],
    });
    const view = createView([liveMarkdown({ plugins: [plugin] })]);

    expect(view.state.facet(syntheticFacet)).toBe("installed");
    expect(listMarkraUi(view, "slash-menu")).toEqual([]);
    expect(listMarkraUi(view, "toolbar")).toMatchObject([
      {
        active: false,
        command: "synthetic.append",
        enabled: true,
        group: "insert",
        icon: "sparkles",
        label: "Append synthetic marker",
        order: 20,
        placement: "toolbar",
        plugin: "markra.synthetic",
      },
    ]);

    expect(runMarkraCommand(view, "synthetic.append")).toBe(true);
    expect(view.state.doc.toString()).toBe("Synthetic document!");
    expect(listMarkraUi(view, "toolbar")[0]?.active).toBe(true);
  });

  it("rejects duplicate plugin and command identifiers", () => {
    const first = defineMarkraPlugin({
      id: "markra.first",
      commands: [
        {
          id: "synthetic.command",
          label: "First command",
          run: () => true,
        },
      ],
    });
    const duplicatePlugin = defineMarkraPlugin({ id: "markra.first" });
    const duplicateCommand = defineMarkraPlugin({
      id: "markra.second",
      commands: [
        {
          id: "synthetic.command",
          label: "Second command",
          run: () => true,
        },
      ],
    });

    expect(() => createView([markraPlugins([first, duplicatePlugin])])).toThrow(
      'Duplicate Markra plugin id "markra.first"',
    );
    expect(() => createView([markraPlugins([first, duplicateCommand])])).toThrow(
      'Duplicate Markra command id "synthetic.command"',
    );
  });

  it("routes declared keybindings through the command registry", () => {
    const plugin = defineMarkraPlugin({
      id: "markra.shortcuts",
      commands: [
        {
          id: "synthetic.shortcut",
          keybindings: [{ key: "Ctrl-Enter", preventDefault: true }],
          label: "Synthetic shortcut",
          run(view) {
            view.dispatch({
              changes: { from: view.state.doc.length, insert: "!" },
            });
            return true;
          },
        },
      ],
    });
    const view = createView([markraPlugins([plugin])]);
    const event = new KeyboardEvent("keydown", {
      bubbles: true,
      ctrlKey: true,
      key: "Enter",
    });

    expect(runScopeHandlers(view, event, "editor")).toBe(true);
    expect(view.state.doc.toString()).toBe("Synthetic document!");
  });

  it("leaves hidden commands out of UI listings but still runs them", () => {
    const command = (id: string) => ({
      id,
      label: id,
      run(view: EditorView) {
        view.dispatch({ changes: { from: 0, insert: `${id} ` } });
        return true;
      },
    });
    const plugin = defineMarkraPlugin({
      id: "markra.synthetic-hidden",
      commands: [command("synthetic.kept"), command("synthetic.hidden")],
      ui: [
        { command: "synthetic.kept", placement: "slash-menu" },
        { command: "synthetic.hidden", placement: "slash-menu" },
      ],
    });
    const stock = createView([liveMarkdown({ plugins: [plugin] })]);
    const hidden = createView([liveMarkdown({ hiddenUi: ["synthetic.hidden"], plugins: [plugin] })]);

    expect(searchMarkraUi(stock, "slash-menu", "synthetic").map((action) => action.command))
      .toEqual(["synthetic.hidden", "synthetic.kept"]);
    expect(searchMarkraUi(hidden, "slash-menu", "synthetic").map((action) => action.command))
      .toEqual(["synthetic.kept"]);
    expect(runMarkraCommand(hidden, "synthetic.hidden")).toBe(true);
    expect(hidden.state.doc.toString()).toBe("synthetic.hidden Synthetic document");
  });

  it("rejects UI contributions that reference an unknown command", () => {
    const plugin = defineMarkraPlugin({
      id: "markra.invalid-ui",
      ui: [{ command: "missing.command", placement: "toolbar" }],
    });

    expect(() => createView([markraPlugins([plugin])])).toThrow(
      'Unknown Markra command id "missing.command"',
    );
  });
});
