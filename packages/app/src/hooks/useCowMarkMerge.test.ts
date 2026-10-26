import { openConflictFiles } from "./useCowMarkMerge";

describe("openConflictFiles", () => {
  it("opens the Markdown files, the first one last so it ends up in front", async () => {
    const calls: string[] = [];
    const open = vi.fn(async (file: { path: string }) => { calls.push(`open ${file.path}`); });
    const openInBackground = vi.fn(async (file: { path: string }) => { calls.push(`background ${file.path}`); });

    await openConflictFiles("/work/c", ["content/a.md", "public/logo.png", "content/b.md"], open, openInBackground);

    expect(calls).toEqual(["background /work/c/content/b.md", "open /work/c/content/a.md"]);
    expect(open).toHaveBeenCalledWith({ name: "a.md", path: "/work/c/content/a.md", relativePath: "content/a.md" });
  });

  it("opens only the first file without tabs", async () => {
    const open = vi.fn(async () => undefined);
    await openConflictFiles("/work/c", ["content/a.md", "content/b.md"], open);
    expect(open).toHaveBeenCalledTimes(1);
  });
});
