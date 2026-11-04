import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import desktopPackage from "../package.json";
import { setCatercowPreviewSiteForTests } from "./lib/content-access";
import {
  installAppTestHarness,
  mockedConsumeWelcomeDocumentState,
  mockedInstallNativeApplicationMenu,
  mockedResolveDesktopPlatform,
  renderApp
} from "./test/app-harness";

installAppTestHarness();

const site = "https://preview.example.com";
const cowmarkHiddenHandlers = [
  "formatCodeBlock",
  "formatInlineCode",
  "importLocalFiles",
  "openDocument",
  "openFolder",
  "openRecentFile",
  "saveDocumentAs",
  "syncNow",
  "toggleAiAgent",
  "toggleAiCommand",
  "toggleAllFolds",
  "toggleReadOnlyMode",
  "toggleSourceMode"
];

const titlebarActionIds = () =>
  Array.from(document.querySelectorAll("[data-titlebar-action]")).map((action) => action.getAttribute("data-titlebar-action"));

async function nativeMenuHandlerNames() {
  await waitFor(() => expect(mockedInstallNativeApplicationMenu).toHaveBeenCalled());
  return Object.keys(mockedInstallNativeApplicationMenu.mock.calls.at(-1)![0]);
}

async function settingsCategories() {
  window.history.pushState({}, "", "/?settings=1");
  const { container } = renderApp();
  await waitFor(() => expect(container.querySelector(".settings-sidebar nav")).toBeInTheDocument());
  return Array.from(container.querySelectorAll(".settings-sidebar nav button")).map((button) => button.textContent);
}

describe("CowMark edition", () => {
  beforeEach(() => {
    mockedConsumeWelcomeDocumentState.mockResolvedValue(false);
  });

  afterEach(() => {
    setCatercowPreviewSiteForTests(undefined);
  });

  it("keeps every menu command and title bar action in stock Markra", async () => {
    setCatercowPreviewSiteForTests(null);
    renderApp();

    expect(await nativeMenuHandlerNames()).toEqual(expect.arrayContaining(cowmarkHiddenHandlers));
    await waitFor(() => expect(titlebarActionIds()).toEqual(expect.arrayContaining(["aiAgent", "viewMode", "sourceMode"])));
  });

  it("leaves out AI, other files, export, sync and unused editing modes in CowMark", async () => {
    setCatercowPreviewSiteForTests(site);
    renderApp();

    const handlers = await nativeMenuHandlerNames();
    for (const name of cowmarkHiddenHandlers) expect(handlers).not.toContain(name);
    expect(handlers).toEqual(expect.arrayContaining([
      "closeDocument",
      "formatBold",
      "formatHeading3",
      "importLocalImages",
      "insertImage",
      "insertTable",
      "openQuickOpen",
      "saveDocument",
      "toggleMarkdownFiles"
    ]));
    expect(handlers.some((name) => name.startsWith("export"))).toBe(false);

    await waitFor(() => expect(titlebarActionIds()).toEqual(expect.arrayContaining(["save", "theme"])));
    expect(titlebarActionIds()).not.toContain("aiAgent");
    expect(titlebarActionIds()).not.toContain("viewMode");
    expect(titlebarActionIds()).not.toContain("sourceMode");
  });

  it("names CowMark and leaves unused items out of the Windows menus", async () => {
    setCatercowPreviewSiteForTests(site);
    mockedResolveDesktopPlatform.mockReturnValue("windows");
    renderApp();

    const menuItems = async (menu: string) => {
      fireEvent.click(await screen.findByRole("menuitem", { name: menu }));
      const items = within(await screen.findByRole("menu", { name: menu })).getAllByRole("menuitem");
      const names = items.map((item) => item.textContent ?? "");
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("menu", { name: menu })).toBeNull());
      return names;
    };

    expect(await menuItems("CowMark")).toEqual(expect.arrayContaining(["About CowMark", "Quit CowMark"]));
    expect((await menuItems("CowMark")).some((item) => item.startsWith("Check for updates"))).toBe(false);
    const fileItems = await menuItems("File");
    expect(fileItems.some((item) => item.startsWith("Save"))).toBe(true);
    for (const hidden of ["New", "Open", "Save As", "Export", "Sync"]) {
      expect(fileItems.some((item) => item.startsWith(hidden))).toBe(false);
    }
    const formatItems = await menuItems("Format");
    expect(formatItems.some((item) => item.startsWith("Bold"))).toBe(true);
    expect(formatItems.some((item) => item.startsWith("Inline Code") || item.startsWith("Code Block"))).toBe(false);
    const viewItems = await menuItems("View");
    for (const hidden of ["Toggle AI", "AI", "Toggle all folds", "Switch to source", "Read-only"]) {
      expect(viewItems.some((item) => item.startsWith(hidden))).toBe(false);
    }
  });

  it("shows only the settings CowMark editors use", async () => {
    setCatercowPreviewSiteForTests(site);

    expect(await settingsCategories()).toEqual([
      "General",
      "Logs",
      "Appearance",
      "Editor",
      "Spellcheck",
      "Keyboard shortcuts"
    ]);
    expect(screen.getByText(`CowMark v${desktopPackage.version}`)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Editor" }));
    await screen.findByText("Top-right buttons");
    expect(screen.queryByText("MDC components")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Keyboard shortcuts" }));
    await screen.findByText("Bold");
    for (const hidden of ["Inline Code", "Code Block", "Save As", "Open Folder"]) {
      expect(screen.queryByText(hidden)).toBeNull();
    }
  });
});
