import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { parseCollectionsSchema } from "../lib/cowmark-pages";
import { NewPageDialog } from "./CowMarkDialogs";

const collections = parseCollectionsSchema(JSON.stringify({
  collections: [
    {
      name: "cuisines",
      label: "Cuisine",
      description: "A cuisine's catering page.",
      slugExample: "american",
      folder: "content/cuisines",
      url: "/cuisines/<file name without .md>-catering",
      fields: [{ name: "title", type: "string", required: true }],
    },
    {
      name: "blog",
      label: "Blog post",
      description: "An article on the blog.",
      folder: "content/blog",
      url: "/blog/<file name without .md>",
      fields: [
        { name: "title", type: "string", required: true },
        { name: "description", type: "string", required: true, description: "Meta description." },
        { name: "feature_image", type: "string", required: false, format: "image" },
        { name: "published_at", type: "string", required: true },
      ],
    },
    {
      name: "competitors",
      label: "Competitor comparison",
      folder: "content/competitors",
      url: "/competitors/<file name without .md>",
      fields: [
        { name: "title", type: "string", required: true },
        { name: "logo", type: "string", required: true, format: "image" },
      ],
    },
    { name: "minorityOwned", label: "Minority-owned", creatable: false, folder: "content/minority-owned", fields: [] },
  ],
}));

async function renderDialog(initialCollection?: string) {
  const onClose = vi.fn();
  const pages = {
    collections,
    listPages: vi.fn(async (collection: { name: string }) => (collection.name === "cuisines" ? ["thai"] : ["lunch"])),
    loadFieldValues: vi.fn(async () => ({})),
  };
  await act(async () => {
    render(<NewPageDialog initialCollection={initialCollection} language="en" pages={pages} onClose={onClose} />);
  });
  return { dialog: screen.getByRole("dialog", { name: "New page" }), onClose };
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("NewPageDialog", () => {
  it("offers creatable page types, blog first, and asks for the blog's required fields", async () => {
    const { dialog, onClose } = await renderDialog();
    const types = within(dialog).getAllByRole("radio");
    expect(types.map((radio) => radio.closest("label")?.textContent)).toEqual([
      "Blog postAn article on the blog.",
      "CuisineA cuisine's catering page.",
      "Competitor comparison",
    ]);
    expect(within(dialog).getByRole("radio", { name: /Blog post/u })).toBeChecked();
    expect(within(dialog).getByLabelText("Description")).toHaveAccessibleDescription(expect.stringContaining("Meta description."));
    expect(within(dialog).queryByLabelText("Feature image")).toBeNull();

    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Lunch: A Guide!" } });
    expect(within(dialog).getByLabelText("URL name")).toHaveValue("lunch-a-guide");
    expect(dialog).toHaveTextContent("/blog/lunch-a-guide");
    fireEvent.change(within(dialog).getByLabelText("Description"), { target: { value: "All about lunch." } });
    fireEvent.change(within(dialog).getByLabelText("Published at"), { target: { value: "2026-11-05" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create page" }));

    expect(onClose).toHaveBeenCalledWith(expect.objectContaining({
      slug: "lunch-a-guide",
      title: "Lunch: A Guide!",
      values: { description: "All about lunch.", published_at: "2026-11-05" },
    }));
    expect(onClose.mock.calls[0]![0].collection.name).toBe("blog");
  });

  it("normalizes an edited URL name and refuses one that exists", async () => {
    const { dialog, onClose } = await renderDialog("cuisines");
    expect(within(dialog).getByRole("radio", { name: /Cuisine/u })).toBeChecked();
    const slug = within(dialog).getByLabelText("URL name");
    expect(slug).toHaveAttribute("placeholder", "american");
    expect(dialog).toHaveTextContent("like american");

    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Thai" } });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("already exists");
    expect(within(dialog).getByRole("button", { name: "Create page" })).toBeDisabled();

    fireEvent.change(slug, { target: { value: "Thai Street " } });
    expect(slug).toHaveValue("thai-street-");
    fireEvent.blur(slug);
    expect(slug).toHaveValue("thai-street");
    expect(dialog).toHaveTextContent("/cuisines/thai-street-catering");
    // The title no longer changes an edited name.
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Thai food" } });
    expect(slug).toHaveValue("thai-street");

    fireEvent.change(slug, { target: { value: "" } });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Enter a URL name.");
    fireEvent.submit(slug.closest("form")!);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("leaves image fields for Page details", async () => {
    const { dialog, onClose } = await renderDialog("competitors");
    expect(dialog).toHaveTextContent("Add Logo in Page details after you create the page.");
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "CaterCow vs. ezCater" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create page" }));
    expect(onClose).toHaveBeenCalledWith(expect.objectContaining({ slug: "catercow-vs-ezcater", values: {} }));
  });
});
