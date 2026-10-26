import { companionPaths, pruneSelection, toggleSelection } from "./content-selection";

const changed = [
  "content/blog/ready.md",
  "content/blog/ready/photo.jpg",
  "content/blog/ready-two.md",
  "content/blog/draft.md"
];

describe("content selection", () => {
  it("finds a page's own folder, not pages that share a prefix", () => {
    expect(companionPaths("content/blog/ready.md", changed)).toEqual(["content/blog/ready/photo.jpg"]);
    expect(companionPaths("content/blog/ready/photo.jpg", changed)).toEqual([]);
  });

  it("ticks and unticks a page together with its folder", () => {
    const on = toggleSelection(new Set(), "content/blog/ready.md", changed);
    expect([...on].sort()).toEqual(["content/blog/ready.md", "content/blog/ready/photo.jpg"]);
    const photoOff = toggleSelection(on, "content/blog/ready/photo.jpg", changed);
    expect([...photoOff]).toEqual(["content/blog/ready.md"]);
    expect([...toggleSelection(on, "content/blog/ready.md", changed)]).toEqual([]);
  });

  it("drops paths that are no longer changed and keeps the same set otherwise", () => {
    const selected = new Set(["content/blog/draft.md"]);
    expect(pruneSelection(selected, changed)).toBe(selected);
    expect([...pruneSelection(selected, ["content/blog/ready.md"])]).toEqual([]);
  });
});
