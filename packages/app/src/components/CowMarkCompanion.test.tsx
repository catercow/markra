import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../App";
import { configureAppRuntime, getAppRuntime, resetAppRuntimeForTests } from "../runtime";
import { setCatercowPreviewSiteForTests, type ContentAccessRuntime } from "../lib/content-access";
import type { ContentPreviewRuntime } from "../lib/content-preview";
import type { CowMarkCompanionRuntime, CowMarkControlRequest } from "../lib/cowmark-companion";

const site = "https://preview.example.com";
const root = "/work/catercow-content";

function mockCowMark() {
  const access = {
    clone: vi.fn(async () => undefined),
    isCheckout: vi.fn(async () => true),
    save: vi.fn(async () => ({ committed: true, head: "abc" })),
    setFolder: vi.fn(async () => undefined),
    signIn: vi.fn(),
    signOut: vi.fn(async () => undefined),
    state: vi.fn(async () => ({ defaultFolder: "/d", email: "editor@catercow.com", expiresAt: "2999-01-01T00:00:00Z", folder: root })),
    status: vi.fn(async () => ({
      ahead: 0,
      branch: "studio",
      changes: [
        { path: "content/blog/a.md", status: "modified" },
        { path: "content/blog/b.md", status: "added" }
      ]
    })),
    update: vi.fn(async () => ({ committed: false, head: "abc" }))
  };
  const preview: ContentPreviewRuntime = {
    pending: vi.fn(() => new Promise<never>(() => {})),
    poll: vi.fn(async () => []),
    readMedia: vi.fn(async () => new ArrayBuffer(0)),
    stop: vi.fn(async () => undefined)
  };
  let controlHandler: ((request: CowMarkControlRequest) => unknown) | null = null;
  const companion = {
    listen: vi.fn<CowMarkCompanionRuntime["listen"]>(async (handler) => {
      controlHandler = handler;
      return () => { controlHandler = null; };
    }),
    start: vi.fn<CowMarkCompanionRuntime["start"]>(async () => ({
      controlError: null,
      previewError: null,
      previewOrigin: "http://localhost:4100",
      previewToken: "t0k"
    }))
  };
  const runtime = getAppRuntime();
  const readMarkdownFile = vi.fn(async (path: string) => ({ content: `# ${path}`, name: path.split("/").pop()!, path }));
  configureAppRuntime({
    ...runtime,
    files: {
      ...runtime.files,
      contentAccess: access as unknown as ContentAccessRuntime,
      contentPreview: preview,
      cowmarkCompanion: companion,
      listMarkdownFilesForPath: vi.fn(async () => []),
      loadMarkdownFilesForPath: vi.fn(async () => []),
      readMarkdownFile: readMarkdownFile as unknown as typeof runtime.files.readMarkdownFile
    }
  });
  setCatercowPreviewSiteForTests(site);
  return {
    access,
    companion,
    readMarkdownFile,
    send: (request: CowMarkControlRequest) => act(async () => { await controlHandler?.(request); })
  };
}

const layoutColumns = () => (document.querySelector(".editor-agent-layout") as HTMLElement | null)?.style.gridTemplateColumns;
const companionButton = () => screen.findByRole("button", { name: "Companion view" });

describe("CowMark as a companion", () => {
  afterEach(() => {
    resetAppRuntimeForTests();
    setCatercowPreviewSiteForTests(undefined);
    window.localStorage.clear();
  });

  it("switches to a preview-only layout and back, and remembers the choice", async () => {
    const { companion } = mockCowMark();
    const { unmount } = render(<App />);

    const toggle = await companionButton();
    await waitFor(() => expect(companion.start).toHaveBeenCalledWith(root, `${site}/`));
    expect(companion.listen).toHaveBeenCalled();
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(layoutColumns()).toMatch(/^minmax\(0,\s?1fr\) 520px/u);

    fireEvent.click(toggle);
    expect(await companionButton()).toHaveAttribute("aria-pressed", "true");
    expect(layoutColumns()).toBe("0px minmax(0,1fr) 0px");
    await waitFor(() => expect(document.querySelector(".content-preview-panel")).toHaveStyle({ width: "100%" }));
    expect(await screen.findByRole("button", { name: "Save to studio" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close site preview" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Show site preview" })).toBeNull();
    expect(window.localStorage.getItem("cowmark.companionMode")).toBe("1");

    unmount();
    render(<App />);
    expect(await companionButton()).toHaveAttribute("aria-pressed", "true");
    await waitFor(() => expect(layoutColumns()).toBe("0px minmax(0,1fr) 0px"));

    fireEvent.click(await companionButton());
    expect(await companionButton()).toHaveAttribute("aria-pressed", "false");
    expect(layoutColumns()).toMatch(/^minmax\(0,\s?1fr\) 520px/u);
    expect(await screen.findByRole("button", { name: "Close site preview" })).toBeInTheDocument();
    expect(document.querySelector(".content-preview-panel")).toHaveStyle({ width: "520px" });
    expect(window.localStorage.getItem("cowmark.companionMode")).toBe("0");
  });

  it("turns a publish proposal into ticked files and a message, and never saves", async () => {
    const { access, send } = mockCowMark();
    render(<App />);
    expect(await screen.findByText("2 changed files")).toBeInTheDocument();

    await send({ action: "proposePublish", message: "Publish post B", paths: ["content/blog/b.md"] });

    await waitFor(() => expect(screen.getByRole("checkbox", { name: "content/blog/b.md" })).toBeChecked());
    expect(screen.getByRole("checkbox", { name: "content/blog/a.md" })).not.toBeChecked();
    expect(screen.getByRole("textbox", { name: "What changed" })).toHaveValue("Publish post B");
    expect(access.save).not.toHaveBeenCalled();
  });

  it("opens a file the cowmark command names", async () => {
    const { readMarkdownFile, send } = mockCowMark();
    render(<App />);
    await companionButton();

    await send({ action: "open", path: `${root}/content/blog/a.md` });

    await waitFor(() => expect(readMarkdownFile).toHaveBeenCalledWith(`${root}/content/blog/a.md`));
  });
});
