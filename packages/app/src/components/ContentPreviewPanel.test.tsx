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

function renderPanel(activeDocument: { content: string; path: string | null } | null) {
  const view = render(
    <ContentPreviewPanel
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
  return { ...view, fromSite, postMessage };
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
});
