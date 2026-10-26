import { browserPreviewUrl, isPortBusy, navigationFlags, readCompanionMode, writeCompanionMode } from "./cowmark-companion";

describe("cowmark-companion", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("builds browser preview URLs on the page's path with the token", () => {
    const origin = "http://localhost:4100";
    expect(browserPreviewUrl(origin, "t0k", "/blog/a")).toBe("http://localhost:4100/blog/a?token=t0k");
    expect(browserPreviewUrl(origin, "t0k", "/blog/a?x=1&token=old")).toBe("http://localhost:4100/blog/a?x=1&token=t0k");
    expect(browserPreviewUrl(origin, "t0k", null)).toBe("http://localhost:4100/?token=t0k");
    expect(browserPreviewUrl(origin, "t0k", "//evil.example/x")).toBe("http://localhost:4100/?token=t0k");
    expect(browserPreviewUrl(origin, "t0k", "/__cowmark/events")).toBe("http://localhost:4100/?token=t0k");
  });

  it("navigates only to the last saved page of a batch", () => {
    expect(navigationFlags([
      { kind: "media", path: "/a.png", mime: "image/png" },
      { kind: "draft", fsPath: "blog/a.md", content: "a" },
      { kind: "remove", fsPath: "blog/gone.md" },
      { kind: "draft", fsPath: "blog/b.md", content: "b" },
      { kind: "media", path: "/b.png", mime: "image/png" }
    ], true)).toEqual([false, false, false, true, false]);
    expect(navigationFlags([{ kind: "draft", fsPath: "blog/a.md", content: "a" }], false)).toEqual([false]);
  });

  it("remembers the companion layout", () => {
    expect(readCompanionMode()).toBe(false);
    writeCompanionMode(true);
    expect(readCompanionMode()).toBe(true);
    writeCompanionMode(false);
    expect(readCompanionMode()).toBe(false);
  });

  it("recognizes a busy port", () => {
    expect(isPortBusy("port_busy: Port 4100 is already in use")).toBe(true);
    expect(isPortBusy("something else")).toBe(false);
    expect(isPortBusy(null)).toBe(false);
  });
});
