import { act, render, screen, waitFor } from "@testing-library/react";
import { configureAppRuntime, getAppRuntime, resetAppRuntimeForTests } from "../runtime";
import type { ContentPreviewRuntime } from "../lib/content-preview";
import { ContentPreviewPanel } from "./ContentPreviewPanel";

const origin = "https://preview.example.com";
const config = { origin, site: `${origin}/` };

function mockContentPreview() {
  const preview = {
    pending: vi.fn<ContentPreviewRuntime["pending"]>(),
    poll: vi.fn<ContentPreviewRuntime["poll"]>(async () => []),
    readMedia: vi.fn<ContentPreviewRuntime["readMedia"]>(async () => new ArrayBuffer(2)),
    stop: vi.fn<ContentPreviewRuntime["stop"]>(async () => undefined)
  };
  const runtime = getAppRuntime();
  configureAppRuntime({ ...runtime, files: { ...runtime.files, contentPreview: preview } });
  return preview;
}

type PanelProps = Partial<Parameters<typeof ContentPreviewPanel>[0]>;

function renderPanel(activeDocument: { content: string; path: string | null } | null, props: PanelProps = {}) {
  const view = render(
    <ContentPreviewPanel
      {...props}
      activeDocument={activeDocument}
      config={config}
      language="en"
      maxWidth={1200}
      minWidth={360}
      rootPath="/work/mc"
      width={520}
      onClose={() => {}}
      onResize={() => {}}
    />
  );
  const frame = view.container.querySelector("iframe")!;
  const postMessage = vi.spyOn(frame.contentWindow!, "postMessage").mockImplementation(() => {});
  const fromSite = (data: unknown, from = origin) => {
    act(() => {
      window.dispatchEvent(new MessageEvent("message", { data, origin: from, source: frame.contentWindow }));
    });
  };
  const posted = () => postMessage.mock.calls.map((call) => call[0] as { type: string; fsPath?: string; navigate?: boolean });
  return { ...view, fromSite, postMessage, posted };
}

describe("ContentPreviewPanel", () => {
  afterEach(() => {
    resetAppRuntimeForTests();
  });

  it("sends local changes and the open document once the site is ready", async () => {
    const preview = mockContentPreview();
    preview.pending.mockResolvedValue({
      base: "abc1234",
      events: [
        { kind: "draft", fsPath: "blog/a.md", content: "# A" },
        { kind: "media", path: "/blog/a.png", mime: "image/png" }
      ],
      warning: null
    });
    const { fromSite, postMessage } = renderPanel({ content: "# Open", path: "/work/mc/content/blog/open.md" });

    expect(screen.getByText("Loading the site…")).toBeInTheDocument();
    fromSite({ type: "catercow:draft:ready", contentCommit: "abc1234" });

    await waitFor(() => expect(postMessage).toHaveBeenCalledTimes(3));
    expect(preview.pending).toHaveBeenCalledWith("/work/mc", "abc1234");
    expect(postMessage.mock.calls.map((call) => (call[0] as { type: string; fsPath?: string; path?: string }))).toEqual([
      expect.objectContaining({ type: "catercow:draft:media", path: "/blog/a.png" }),
      expect.objectContaining({ type: "catercow:draft", fsPath: "blog/a.md", navigate: false }),
      expect.objectContaining({ type: "catercow:draft", fsPath: "blog/open.md", content: "# Open", navigate: true })
    ]);
    expect(postMessage.mock.calls[0][1]).toBe(origin);
    expect(screen.getByText("Showing 2 local changes")).toBeInTheDocument();
  });

  it("ignores messages from other origins", () => {
    const preview = mockContentPreview();
    const { fromSite } = renderPanel(null);

    fromSite({ type: "catercow:draft:ready", contentCommit: "abc1234" }, "https://evil.example.com");

    expect(preview.pending).not.toHaveBeenCalled();
    expect(screen.getByText("Loading the site…")).toBeInTheDocument();
  });

  it("shows the site's render errors and git warnings", async () => {
    const preview = mockContentPreview();
    preview.pending.mockResolvedValue({ events: [], warning: "No studio branch here." });
    const { fromSite } = renderPanel(null);

    fromSite({ type: "catercow:draft:ready", contentCommit: "" });
    expect(await screen.findByText("No studio branch here.")).toBeInTheDocument();

    fromSite({ type: "catercow:draft:error", fsPath: "blog/a.md", message: "bad YAML" });
    expect(screen.getByText("Couldn't show blog/a.md: bad YAML")).toBeInTheDocument();
  });

  it("follows the most recently saved file", async () => {
    const preview = mockContentPreview();
    preview.pending.mockResolvedValue({ events: [], warning: null });
    preview.poll.mockResolvedValueOnce([
      { kind: "draft", fsPath: "blog/a.md", content: "a" },
      { kind: "media", path: "/blog/b.png", mime: "image/png" },
      { kind: "draft", fsPath: "blog/b.md", content: "b" }
    ]);
    const { fromSite, posted } = renderPanel(null);

    fromSite({ type: "catercow:draft:ready", contentCommit: "abc1234" });

    await waitFor(() => expect(posted()).toHaveLength(3));
    expect(posted()).toEqual([
      expect.objectContaining({ type: "catercow:draft:media" }),
      expect.objectContaining({ type: "catercow:draft", fsPath: "blog/a.md", navigate: false }),
      expect.objectContaining({ type: "catercow:draft", fsPath: "blog/b.md", navigate: true })
    ]);
  });

  it("shows the open document again when asked, even if it was already sent", async () => {
    const preview = mockContentPreview();
    preview.pending.mockResolvedValue({ events: [], warning: null });
    const document = { content: "# Open", path: "/work/mc/content/blog/open.md" };
    const { fromSite, posted, rerender } = renderPanel(document, { revealRequest: 0 });
    fromSite({ type: "catercow:draft:ready", contentCommit: "abc1234" });
    await waitFor(() => expect(posted()).toHaveLength(1));

    rerender(
      <ContentPreviewPanel
        activeDocument={document}
        config={config}
        language="en"
        maxWidth={1200}
        minWidth={360}
        revealRequest={1}
        rootPath="/work/mc"
        width={520}
        onClose={() => {}}
        onResize={() => {}}
      />
    );
    await waitFor(() => expect(posted()).toHaveLength(2));
    expect(posted()[1]).toEqual(expect.objectContaining({ fsPath: "blog/open.md", navigate: true }));
  });

  it("fills the window in the companion layout, without close or resize", () => {
    mockContentPreview();
    const { container } = renderPanel(null, { fill: true });

    expect(container.querySelector(".content-preview-panel")).toHaveStyle({ width: "100%" });
    expect(screen.queryByRole("button", { name: "Close site preview" })).toBeNull();
    expect(screen.queryByRole("separator")).toBeNull();
  });

  it("opens the browser preview through CowMark and shows why it couldn't", async () => {
    mockContentPreview();
    const onOpenInBrowser = vi.fn(async () => { throw new Error("Port 4100 is in use by another program."); });
    const openExternalUrl = vi.spyOn(getAppRuntime().window, "openExternalUrl");
    renderPanel(null, { onOpenInBrowser });

    act(() => {
      screen.getByRole("button", { name: "Open this page in your browser" }).click();
    });

    expect(onOpenInBrowser).toHaveBeenCalledWith(null);
    expect(await screen.findByText("Port 4100 is in use by another program.")).toBeInTheDocument();
    expect(openExternalUrl).not.toHaveBeenCalled();
  });

  it("shows a notice when the component list couldn't be loaded from the site", () => {
    mockContentPreview().pending.mockResolvedValue({ events: [] });
    const { rerender } = renderPanel(null, { schemaNotice: { kind: "outOfDate", site: "content-preview.catercow.com" } });
    expect(screen.getByRole("status").textContent).toBe(
      "Component list may be out of date — couldn't reach content-preview.catercow.com."
    );
    rerender(
      <ContentPreviewPanel
        activeDocument={null}
        config={config}
        language="en"
        maxWidth={1200}
        minWidth={360}
        rootPath="/work/mc"
        schemaNotice={{ kind: "missing", site: "localhost:3000" }}
        width={520}
        onClose={() => {}}
        onResize={() => {}}
      />
    );
    expect(screen.getByRole("status").textContent).toBe("No component list — couldn't reach localhost:3000.");
  });

  it("shows no schema notice when the site's schema is in use", () => {
    mockContentPreview().pending.mockResolvedValue({ events: [] });
    renderPanel(null);
    expect(screen.queryByRole("status")).toBeNull();
  });
});
