import { contentAccessError, managedFilePath } from "./content-access";

describe("contentAccessError", () => {
  it("splits the code, the plain message and a conflict's file list", () => {
    expect(contentAccessError(new Error('conflict: Resolve them.\nconflict-files: ["content/a.md","content/b.md"]'))).toEqual({
      code: "conflict",
      files: ["content/a.md", "content/b.md"],
      message: "Resolve them.",
      problems: []
    });
    expect(contentAccessError("offline: No internet.")).toEqual({ code: "offline", files: [], message: "No internet.", problems: [] });
    expect(contentAccessError("conflict: Broken list.\nconflict-files: [oops")).toEqual({
      code: "conflict",
      files: [],
      message: "Broken list.",
      problems: []
    });
    expect(contentAccessError(new Error("Something else"))).toEqual({ code: "other", files: [], message: "Something else", problems: [] });
  });

  it("reads a lint refusal's problems and their files", () => {
    const problem = { file: "content/a.md", line: 3, message: "HTML isn't allowed in pages." };
    expect(contentAccessError(`lint: Fix these problems.\ncontent/a.md, line 3: HTML isn't allowed in pages.\nlint-problems: ${JSON.stringify([problem, { file: 1 }])}`)).toEqual({
      code: "lint",
      files: ["content/a.md"],
      message: "Fix these problems.\ncontent/a.md, line 3: HTML isn't allowed in pages.",
      problems: [problem]
    });
  });
});

describe("managedFilePath", () => {
  it("joins git paths in the folder's own separators", () => {
    expect(managedFilePath("/Users/a/catercow-content", "content/blog/a.md")).toBe("/Users/a/catercow-content/content/blog/a.md");
    expect(managedFilePath("C:\\Users\\a\\catercow-content", "content/blog/a.md")).toBe("C:\\Users\\a\\catercow-content\\content\\blog\\a.md");
  });
});
