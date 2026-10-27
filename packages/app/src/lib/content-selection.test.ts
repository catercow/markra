import { companionPaths, pruneSelection, toggleSelection } from "./content-selection";

const changed = [
  "content/blog/ready.md",
  "public/blog/ready/photo.jpg",
  "public/blog/ready-two/photo.jpg",
  "public/blog/shared/banner.png",
  "content/blog/ready/old.jpg",
  "content/blog/ready-two.md",
  "content/blog/shared.md",
  "content/blog/draft.md",
  "content/cuisines/thai.md",
  "public/cuisines/thai/curry.jpg"
];

describe("content selection", () => {
  it("finds a page's image folder under public/, not pages that share a prefix", () => {
    expect(companionPaths("content/blog/ready.md", changed)).toEqual(["public/blog/ready/photo.jpg"]);
    expect(companionPaths("public/blog/ready/photo.jpg", changed)).toEqual([]);
    expect(companionPaths("content/blog/draft.md", changed)).toEqual([]);
  });

  it("leaves the shared image folder alone", () => {
    expect(companionPaths("content/blog/shared.md", changed)).toEqual([]);
  });

  it("follows the collection's URL prefix", () => {
    const collections = [
      { fields: [], folder: "content/cuisines", name: "cuisines", url: "/cuisines/<file name without .md>-catering" }
    ];
    expect(companionPaths("content/cuisines/thai.md", changed, collections)).toEqual(["public/cuisines/thai/curry.jpg"]);
    const renamed = [{ fields: [], folder: "content/cuisines", name: "cuisines", url: "/food/<file name without .md>" }];
    expect(companionPaths("content/cuisines/thai.md", ["public/food/thai/a.jpg"], renamed)).toEqual(["public/food/thai/a.jpg"]);
  });

  it("ticks and unticks a page together with its images", () => {
    const on = toggleSelection(new Set(), "content/blog/ready.md", changed);
    expect([...on].sort()).toEqual(["content/blog/ready.md", "public/blog/ready/photo.jpg"]);
    const photoOff = toggleSelection(on, "public/blog/ready/photo.jpg", changed);
    expect([...photoOff]).toEqual(["content/blog/ready.md"]);
    expect([...toggleSelection(on, "content/blog/ready.md", changed)]).toEqual([]);
  });

  it("drops paths that are no longer changed and keeps the same set otherwise", () => {
    const selected = new Set(["content/blog/draft.md"]);
    expect(pruneSelection(selected, changed)).toBe(selected);
    expect([...pruneSelection(selected, ["content/blog/ready.md"])]).toEqual([]);
  });
});
