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
    fireEvent.click(screen.getByRole("button", { name: "Save to studio" }));

    expect(await screen.findByText("Saved to studio.")).toBeInTheDocument();
    expect(beforeSave).toHaveBeenCalled();
    expect(access.save).toHaveBeenCalledWith("/work/c", "Fix typo");
    expect(screen.getByRole("textbox", { name: "What changed" })).toHaveValue("");
  });

  it("shows a conflict in plain words", async () => {
    mockAccess({
      save: vi.fn(async () => {
        throw new Error("conflict: Someone else changed the same part of content/blog/a.md. Your changes are still saved on this computer; ask a developer for help.");
      })
    });
    render(<ContentSaveBar language="en" managed={managed("editor@catercow.com")} />);

    fireEvent.click(await screen.findByRole("button", { name: "Save to studio" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Someone else changed the same part of content/blog/a.md.");
  });

  it("asks to sign in again when the session expired, keeping the message", async () => {
    const access = mockAccess({ save: vi.fn(async () => { throw new Error("signed_out: Session expired"); }) });
    const state = managed("editor@catercow.com");
    const { rerender } = render(<ContentSaveBar language="en" managed={state} />);

    fireEvent.change(await screen.findByRole("textbox", { name: "What changed" }), { target: { value: "My edit" } });
    fireEvent.click(screen.getByRole("button", { name: "Save to studio" }));
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
