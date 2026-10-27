import { EditorView } from "@codemirror/view";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CowMarkPagesContext, type CowMarkPages } from "../hooks/useCowMarkPages";
import { parseCollectionsSchema } from "../lib/cowmark-pages";
import { CodeMirrorPaperSurface } from "./CodeMirrorPaperSurface";

const collections = parseCollectionsSchema(JSON.stringify({
  collections: [{
    name: "blog",
    folder: "content/blog",
    fields: [
      { name: "title", type: "string", required: true },
      { name: "description", type: "string", required: false, default: "" },
      { name: "feature_image", type: "string", required: false },
      { name: "published_at", type: "string", required: true },
      { name: "category", type: "string", required: false },
      { name: "featured", type: "boolean", required: false, default: false },
      { name: "schema_org", type: "list of object", required: false, default: [] },
    ],
  }],
}));

const post = [
  "---",
  "title: 'Lunch: a guide'",
  "category: tips # keep this comment",
  "published_at: '2026-10-06'",
  "---",
  "",
  "# Body",
].join("\n");

function renderSurface(doc: string, documentPath = "/repo/content/blog/lunch.md") {
  let view: EditorView | null = null;
  const pages: CowMarkPages = {
    collections,
    imageFolder: () => "/blog/lunch",
    loadFieldValues: vi.fn(async () => ({ category: ["tips", "news"] })),
    pickImage: vi.fn(async () => "/blog/lunch/tacos.jpg"),
    resolveImage: (_path, src) => `asset://${src}`,
    rootPath: "/repo",
    saveImage: vi.fn(async () => "/blog/lunch/tacos.jpg"),
  };
  render(
    <CowMarkPagesContext.Provider value={pages}>
      <CodeMirrorPaperSurface
        documentPath={documentPath}
        initialContent={doc}
        onEditorReady={(next) => {
          view = next;
        }}
        onMarkdownChange={() => {}}
      />
    </CowMarkPagesContext.Provider>,
  );
  const editor = view! as EditorView;
  return { doc: () => editor.state.doc.toString(), pages, view: editor };
}

function openPanel() {
  fireEvent.click(screen.getByRole("button", { name: "Page details" }));
  return screen.getByRole("complementary", { name: "Page details" });
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("PageDetailsPanel", () => {
  it("opens from the front matter card with the collection's fields", () => {
    renderSurface(post);
    const panel = openPanel();
    expect(within(panel).getByLabelText("Title")).toHaveProperty("value", "Lunch: a guide");
    expect(within(panel).getByLabelText("Published at")).toHaveProperty("value", "2026-10-06");
    expect(within(panel).getByText("14 / 60 characters")).toBeTruthy();
    expect(within(panel).getByText("Advanced")).toBeTruthy();
  });

  it("rewrites only the edited key", () => {
    const { doc } = renderSurface(post);
    const panel = openPanel();
    fireEvent.change(within(panel).getByLabelText("Title"), { target: { value: "Lunch: the guide" } });
    expect(doc()).toBe(post.replace("'Lunch: a guide'", "'Lunch: the guide'"));

    fireEvent.change(within(panel).getByLabelText("Published at"), { target: { value: "2026-11-02" } });
    expect(doc()).toBe(post.replace("'Lunch: a guide'", "'Lunch: the guide'").replace("2026-10-06", "2026-11-02"));

    fireEvent.click(within(panel).getByRole("switch", { name: "Featured" }));
    expect(doc()).toContain("category: tips # keep this comment\nfeatured: true\npublished_at");
  });

  it("offers values other pages use and stores picked images relatively", async () => {
    const { doc, pages } = renderSurface(post);
    const panel = openPanel();
    await waitFor(() => expect(panel.querySelector("datalist option[value='news']")).not.toBeNull());

    await act(async () => {
      fireEvent.click(within(panel).getByRole("button", { name: /Choose image/u }));
    });
    expect(pages.pickImage).toHaveBeenCalledWith("/repo/content/blog/lunch.md");
    expect(doc()).toContain("\nfeature_image: /blog/lunch/tacos.jpg\n");
    expect(panel.querySelector("img")?.getAttribute("src")).toBe("asset:///blog/lunch/tacos.jpg");
  });

  it("has no button outside a known collection", () => {
    renderSurface(post, "/repo/content/help/a.md");
    expect(screen.queryByRole("button", { name: "Page details" })).toBeNull();
  });
});
