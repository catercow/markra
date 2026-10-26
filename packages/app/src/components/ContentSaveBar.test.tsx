import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { configureAppRuntime, getAppRuntime, resetAppRuntimeForTests } from "../runtime";
import type { ContentAccessRuntime, ManagedContent } from "../lib/content-access";
import { ContentSaveBar } from "./ContentSaveBar";

const changed = { ahead: 0, branch: "studio", changes: [{ path: "content/blog/a.md", status: "modified" }] };

function mockAccess(overrides: Partial<Record<keyof ContentAccessRuntime, unknown>> = {}) {
  const access = {
    clone: vi.fn(),
    isCheckout: vi.fn(),
    save: vi.fn(async () => ({ committed: true, head: "abc" })),
    setFolder: vi.fn(),
    signIn: vi.fn(async () => ({ email: "editor@catercow.com", expiresAt: "2999-01-01T00:00:00.000Z" })),
    signOut: vi.fn(async () => undefined),
    state: vi.fn(),
    status: vi.fn(async () => changed),
    update: vi.fn(async () => ({ committed: false, head: "abc" })),
    ...overrides
  };
  const runtime = getAppRuntime();
  configureAppRuntime({ ...runtime, files: { ...runtime.files, contentAccess: access as unknown as ContentAccessRuntime } });
  return access;
}

function managed(email: string | null, overrides: Partial<ManagedContent> = {}): ManagedContent {
  return {
    email,
    root: "/work/c",
    site: "https://preview.example.com",
    onSignedIn: vi.fn(),
    onSignedOut: vi.fn(),
    ...overrides
  };
}

describe("ContentSaveBar", () => {
  afterEach(() => {
    resetAppRuntimeForTests();
  });

  it("saves open documents, then saves to studio with the message", async () => {
    const access = mockAccess();
    const beforeSave = vi.fn(async () => undefined);
    render(<ContentSaveBar beforeSave={beforeSave} language="en" managed={managed("editor@catercow.com")} />);

    expect(await screen.findByText("1 changed file")).toBeInTheDocument();
    expect(screen.getByText("Signed in as editor@catercow.com")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "What changed" }), { target: { value: "Fix typo" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "content/blog/a.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Save 1 to studio" }));

    expect(await screen.findByText("Saved to studio.")).toBeInTheDocument();
    expect(beforeSave).toHaveBeenCalled();
    expect(access.save).toHaveBeenCalledWith("/work/c", "Fix typo", ["content/blog/a.md"]);
    expect(screen.getByRole("textbox", { name: "What changed" })).toHaveValue("");
  });

  it("saves only the ticked files; a page brings its image folder along", async () => {
    const access = mockAccess({
      status: vi.fn(async () => ({
        ahead: 0,
        branch: "studio",
        changes: [
          { path: "content/blog/ready.md", status: "added" },
          { path: "content/blog/ready/photo.jpg", status: "added" },
          { path: "content/blog/draft.md", status: "modified" }
        ]
      }))
    });
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} />);

    expect(await screen.findByText("3 changed files")).toBeInTheDocument();
    expect(screen.getByText("Unticked changes stay on this computer as drafts.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save to studio" })).toBeDisabled();

    fireEvent.click(screen.getByRole("checkbox", { name: "content/blog/ready.md" }));
    expect(screen.getByRole("checkbox", { name: "content/blog/ready/photo.jpg" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "content/blog/draft.md" })).not.toBeChecked();
    expect((screen.getByRole("checkbox", { name: "Select all" }) as HTMLInputElement).indeterminate).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Save 2 to studio" }));
    await waitFor(() =>
      expect(access.save).toHaveBeenCalledWith("/work/c", "", ["content/blog/ready.md", "content/blog/ready/photo.jpg"])
    );
  });

  it("selects and clears every change", async () => {
    mockAccess({
      status: vi.fn(async () => ({
        ahead: 0,
        branch: "studio",
        changes: [
          { path: "content/blog/a.md", status: "modified" },
          { path: "content/blog/b.md", status: "deleted" }
        ]
      }))
    });
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} />);

    fireEvent.click(await screen.findByRole("checkbox", { name: "Select all" }));
    expect(screen.getByRole("button", { name: "Save 2 to studio" })).toBeEnabled();
    expect(screen.getByText("Deleted")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select all" }));
    expect(screen.getByRole("button", { name: "Save to studio" })).toBeDisabled();
  });

  it("can send commits waiting from an earlier save with nothing ticked", async () => {
    const access = mockAccess({ status: vi.fn(async () => ({ ahead: 1, branch: "studio", changes: [] })) });
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} />);

    const button = await screen.findByRole("button", { name: "Save to studio" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    await waitFor(() => expect(access.save).toHaveBeenCalledWith("/work/c", "", []));
  });

  it("shows a conflict without a file list in plain words", async () => {
    mockAccess({
      save: vi.fn(async () => {
        throw new Error("conflict: Someone else changed the same part of content/blog/a.md.");
      })
    });
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} />);

    fireEvent.click(await screen.findByRole("checkbox", { name: "content/blog/a.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Save 1 to studio" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Someone else changed the same part of content/blog/a.md.");
  });

  it("opens the files a conflict left to resolve and says to save again, keeping the ticks", async () => {
    mockAccess({
      save: vi.fn(async () => {
        throw new Error(
          'conflict: Someone else changed the same part of content/blog/a.md and content/blog/b.md. Both versions are in the file now.\nconflict-files: ["content/blog/a.md","content/blog/b.md"]'
        );
      })
    });
    const onConflictFiles = vi.fn();
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} onConflictFiles={onConflictFiles} />);

    fireEvent.click(await screen.findByRole("checkbox", { name: "content/blog/a.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Save 1 to studio" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Someone else changed the same lines. Resolve the highlighted conflicts in content/blog/a.md, content/blog/b.md, then save again."
    );
    expect(onConflictFiles).toHaveBeenCalledWith(["content/blog/a.md", "content/blog/b.md"]);
    expect(screen.getByRole("checkbox", { name: "content/blog/a.md" })).toBeChecked();
  });

  it("treats a Get latest conflict the same way", async () => {
    mockAccess({
      update: vi.fn(async () => {
        throw new Error('conflict: Someone else changed the same part of content/blog/a.md.\nconflict-files: ["content/blog/a.md"]');
      })
    });
    const onConflictFiles = vi.fn();
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} onConflictFiles={onConflictFiles} />);

    fireEvent.click(await screen.findByRole("button", { name: "Get latest" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Resolve the highlighted conflicts in content/blog/a.md, then save again.");
    expect(onConflictFiles).toHaveBeenCalledWith(["content/blog/a.md"]);
  });

  it("asks to sign in again when the session expired, keeping the message", async () => {
    const access = mockAccess({ save: vi.fn(async () => { throw new Error("signed_out: Session expired"); }) });
    const state = managed("editor@catercow.com");
    const { rerender } = render(<ContentSaveBar language="en" managed={state} />);

    fireEvent.change(await screen.findByRole("textbox", { name: "What changed" }), { target: { value: "My edit" } });
    fireEvent.click(await screen.findByRole("checkbox", { name: "content/blog/a.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Save 1 to studio" }));
    await waitFor(() => expect(state.onSignedOut).toHaveBeenCalled());

    const signedOut = { ...state, email: null };
    rerender(<ContentSaveBar language="en" managed={signedOut} />);
    expect(screen.getByText("Sign in again to save. Your edits are kept on this computer.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));
    await waitFor(() => expect(state.onSignedIn).toHaveBeenCalledWith("editor@catercow.com"));
    expect(access.signIn).toHaveBeenCalledWith("https://preview.example.com");

    rerender(<ContentSaveBar language="en" managed={state} />);
    expect(screen.getByRole("textbox", { name: "What changed" })).toHaveValue("My edit");
  });

  it("explains when the email may no longer save", async () => {
    mockAccess({ update: vi.fn(async () => { throw new Error("not_allowed: removed"); }) });
    const state = managed("editor@catercow.com");
    render(<ContentSaveBar language="en" managed={state} />);

    fireEvent.click(await screen.findByRole("button", { name: "Get latest" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("This email can no longer save CaterCow content. Ask a developer.");
    expect(state.onSignedOut).toHaveBeenCalled();
  });

  it("offers sign-in when signed out and signs out on request", async () => {
    const access = mockAccess();
    const state = managed(null);
    const { rerender } = render(<ContentSaveBar language="en" managed={state} />);
    expect(await screen.findByText("Sign in to save your changes to studio.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Save to studio" })).not.toBeInTheDocument();

    rerender(<ContentSaveBar language="en" managed={{ ...state, email: "editor@catercow.com" }} />);
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(access.signOut).toHaveBeenCalled();
    expect(state.onSignedOut).toHaveBeenCalled();
  });
});
