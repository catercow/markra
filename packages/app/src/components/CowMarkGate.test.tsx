import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "../App";
import { configureAppRuntime, getAppRuntime, resetAppRuntimeForTests } from "../runtime";
import { setCatercowPreviewSiteForTests, type ContentAccessRuntime, type ManagedContent } from "../lib/content-access";
import { CowMarkGate } from "./CowMarkGate";

const site = "https://preview.example.com";

function mockAccess(overrides: Partial<ContentAccessRuntime> = {}) {
  const access: { [K in keyof ContentAccessRuntime]: ReturnType<typeof vi.fn> } = {
    clone: vi.fn(async () => undefined),
    isCheckout: vi.fn(async () => false),
    save: vi.fn(async () => ({ committed: true, head: "abc" })),
    setFolder: vi.fn(async () => undefined),
    signIn: vi.fn(async () => ({ email: "editor@catercow.com", expiresAt: "2999-01-01T00:00:00.000Z" })),
    signOut: vi.fn(async () => undefined),
    state: vi.fn(async () => ({ defaultFolder: "/Users/e/Documents/CaterCow Content", email: null, expiresAt: null, folder: null })),
    status: vi.fn(async () => ({ ahead: 0, branch: "studio", changes: [] })),
    update: vi.fn(async () => ({ committed: false, head: "abc" }))
  };
  Object.assign(access, overrides);
  const runtime = getAppRuntime();
  configureAppRuntime({ ...runtime, files: { ...runtime.files, contentAccess: access as unknown as ContentAccessRuntime } });
  return access;
}

function renderGate() {
  const renderWorkspace = vi.fn((managed: ManagedContent) => (
    <div data-testid="workspace">{managed.root} | {managed.email ?? "signed out"}</div>
  ));
  render(<CowMarkGate site={site} renderWorkspace={renderWorkspace} />);
  return renderWorkspace;
}

describe("CowMarkGate", () => {
  afterEach(() => {
    resetAppRuntimeForTests();
    setCatercowPreviewSiteForTests(undefined);
  });

  it("signs in, downloads the content to the default folder, then opens it", async () => {
    const access = mockAccess();
    const renderWorkspace = renderGate();

    fireEvent.click(await screen.findByRole("button", { name: "Sign in with Google" }));
    expect(access.signIn).toHaveBeenCalledWith(site);
    expect(await screen.findByTestId("cowmark-dest")).toHaveTextContent("/Users/e/Documents/CaterCow Content");

    fireEvent.click(screen.getByRole("button", { name: "Download the content" }));
    expect(await screen.findByTestId("workspace")).toHaveTextContent(
      "/Users/e/Documents/CaterCow Content | editor@catercow.com"
    );
    expect(access.clone).toHaveBeenCalledWith("/Users/e/Documents/CaterCow Content");
    expect(renderWorkspace).toHaveBeenCalled();
  });

  it("opens an existing clone without signing in, and skips get-latest when signed out", async () => {
    const access = mockAccess({
      isCheckout: vi.fn(async () => true),
      state: vi.fn(async () => ({ defaultFolder: "/d", email: null, expiresAt: null, folder: "/work/CaterCow Content" }))
    });
    renderGate();

    expect(await screen.findByTestId("workspace")).toHaveTextContent("/work/CaterCow Content | signed out");
    expect(screen.queryByRole("button", { name: "Sign in with Google" })).not.toBeInTheDocument();
    expect(access.status).not.toHaveBeenCalled();
    expect(access.update).not.toHaveBeenCalled();
  });

  it("quietly gets latest on launch when signed in with nothing to combine, ignoring failures", async () => {
    const access = mockAccess({
      isCheckout: vi.fn(async () => true),
      state: vi.fn(async () => ({ defaultFolder: "/d", email: "editor@catercow.com", expiresAt: "x", folder: "/work/c" })),
      update: vi.fn(async () => { throw new Error("offline: no network"); })
    });
    renderGate();

    expect(await screen.findByTestId("workspace")).toHaveTextContent("/work/c | editor@catercow.com");
    await waitFor(() => expect(access.update).toHaveBeenCalledWith("/work/c"));
    expect(screen.getByTestId("workspace")).toHaveTextContent("editor@catercow.com");
  });

  it("explains when the email isn't allowed", async () => {
    mockAccess({ signIn: vi.fn(async () => { throw new Error("not_allowed: nope"); }) });
    renderGate();

    fireEvent.click(await screen.findByRole("button", { name: "Sign in with Google" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("This email can't edit CaterCow content.");
  });

  it("lets the first setup pick another location", async () => {
    mockAccess({ state: vi.fn(async () => ({ defaultFolder: "/d/CaterCow Content", email: "e@catercow.com", expiresAt: "x", folder: null })) });
    const runtime = getAppRuntime();
    configureAppRuntime({
      ...runtime,
      files: { ...runtime.files, openMarkdownFolder: vi.fn(async () => ({ name: "Work", path: "/Volumes/Work" })) }
    });
    renderGate();

    fireEvent.click(await screen.findByRole("button", { name: "Choose another location…" }));
    await waitFor(() => expect(screen.getByTestId("cowmark-dest")).toHaveTextContent("/Volumes/Work/CaterCow Content"));
  });
});

describe("App in the CowMark build", () => {
  afterEach(() => {
    resetAppRuntimeForTests();
    setCatercowPreviewSiteForTests(undefined);
  });

  it("shows the CowMark sign-in screen only when the CowMark site is built in", async () => {
    mockAccess();
    setCatercowPreviewSiteForTests(site);
    const { unmount } = render(<App />);
    expect(await screen.findByRole("heading", { name: "CowMark" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
    unmount();

    setCatercowPreviewSiteForTests(null);
    render(<App />);
    await waitFor(() => expect(document.querySelector(".app-shell")).not.toBeNull());
    expect(screen.queryByRole("heading", { name: "CowMark" })).not.toBeInTheDocument();
  });

  it("opens the managed folder on launch even when signed out", async () => {
    mockAccess({
      isCheckout: vi.fn(async () => true),
      state: vi.fn(async () => ({ defaultFolder: "/d", email: null, expiresAt: null, folder: "/work/CaterCow Content" }))
    });
    const runtime = getAppRuntime();
    const loadMarkdownFilesForPath = vi.fn(async () => []);
    const listMarkdownFilesForPath = vi.fn(async () => []);
    configureAppRuntime({
      ...runtime,
      files: { ...runtime.files, listMarkdownFilesForPath, loadMarkdownFilesForPath }
    });
    setCatercowPreviewSiteForTests(site);
    render(<App />);

    await waitFor(() => {
      const paths = [...loadMarkdownFilesForPath.mock.calls, ...listMarkdownFilesForPath.mock.calls].map((call) => (call as unknown[])[0]);
      expect(paths).toContain("/work/CaterCow Content");
    });
    expect(screen.queryByRole("heading", { name: "CowMark" })).toBeNull();
  });
});
