import type { MarkdownShortcutAction } from "@markra/editor";
import type { I18nKey } from "@markra/shared";
import type { SettingsCategory } from "../hooks/useSettingsWindowState";
import type { AppFeatureRuntime } from "../runtime";
import type { TitlebarActionId } from "./settings/app-settings";
import type { SelectionFormattingToolbarAction } from "./selection-formatting";
import { catercowPreviewSite } from "./content-access";

/**
 * What a CowMark build leaves out. CowMark's editors write site pages in one managed folder, so
 * AI, export, sync, templates, other folders and syntax the site doesn't render are hidden.
 * Every list is empty in stock Markra, so passing them through changes nothing there.
 */
export type EditionHidden = {
  /** Settings → Keyboard shortcuts rows for fixed shortcuts, by label. */
  fixedShortcuts: ReadonlySet<I18nKey>;
  /** Native, title bar and context menu command ids (`NativeMenuCommand`). */
  menuCommands: ReadonlySet<string>;
  selectionActions: ReadonlySet<SelectionFormattingToolbarAction>;
  settingsCategories: readonly SettingsCategory[];
  /** Configurable shortcuts that do nothing and aren't listed in settings. */
  shortcutActions: ReadonlySet<MarkdownShortcutAction>;
  /** Editor commands left out of the slash menu (they still exist). */
  slashCommands: readonly string[];
  titlebarActions: ReadonlySet<TitlebarActionId>;
};

// Keep in step with COWMARK_HIDDEN_MENU_COMMANDS in apps/desktop/src-tauri/src/menu.rs.
const cowmarkHidden: EditionHidden = {
  fixedShortcuts: new Set<I18nKey>([
    "menu.newDocument",
    "menu.openDocument",
    "app.openFolderDialog",
    "menu.saveDocumentAs",
    "menu.exportPdf",
    "menu.exportHtml"
  ]),
  menuCommands: new Set([
    "newDocument",
    "openDocument",
    "openRecentFile",
    "clearRecentFiles",
    "openFolder",
    "saveDocumentAs",
    "syncNow",
    "export",
    "checkForUpdates",
    "formatInlineCode",
    "formatCodeBlock",
    "importLocalFiles",
    "toggleAiAgent",
    "toggleAiCommand",
    "toggleAllFolds",
    "toggleReadOnlyMode",
    "toggleSourceMode"
  ]),
  // The site renders neither inline code styling editors would want nor `==highlight==`.
  selectionActions: new Set<SelectionFormattingToolbarAction>(["inlineCode", "highlight"]),
  settingsCategories: ["ai", "providers", "web", "network", "storage", "backup", "sync", "templates", "export", "view"],
  shortcutActions: new Set<MarkdownShortcutAction>([
    "syncNow",
    "toggleAiAgent",
    "toggleAiCommand",
    "toggleSourceMode",
    "toggleReadOnlyMode",
    "inlineCode",
    "codeBlock",
    "toggleAllFolds"
  ]),
  // The site doesn't render GitHub alerts or task lists; blog pages use H2 and H3.
  slashCommands: [
    "block.code",
    "block.callout",
    "block.task-list",
    "block.heading.4",
    "block.heading.5",
    "block.heading.6",
    "insert.today"
  ],
  titlebarActions: new Set<TitlebarActionId>(["aiAgent", "viewMode", "sourceMode"])
};

const stockHidden: EditionHidden = {
  fixedShortcuts: new Set(),
  menuCommands: new Set(),
  selectionActions: new Set(),
  settingsCategories: [],
  shortcutActions: new Set(),
  slashCommands: [],
  titlebarActions: new Set()
};

/** True in CowMark builds (`VITE_CATERCOW_PREVIEW_SITE` set), false in stock Markra. */
export function cowmarkEdition() {
  return catercowPreviewSite() !== null;
}

export function editionAppName() {
  return cowmarkEdition() ? "CowMark" : "Markra";
}

/** A label that names the app ("Quit Markra"), naming CowMark in CowMark builds. */
export function editionLabel(label: string) {
  return cowmarkEdition() ? label.replaceAll("Markra", "CowMark") : label;
}

let updatesOverride: boolean | undefined;

/**
 * True in CowMark release builds that publish updates (`VITE_COWMARK_UPDATES=true`, set by
 * .github/workflows/catercow-desktop.yml with the updater endpoint and signing key).
 */
export function cowmarkUpdates() {
  if (updatesOverride !== undefined) return updatesOverride;
  return String(import.meta.env.VITE_COWMARK_UPDATES ?? "").trim() === "true";
}

export function setCowmarkUpdatesForTests(value: boolean | undefined) {
  updatesOverride = value;
}

const cowmarkHiddenWithUpdates: EditionHidden = {
  ...cowmarkHidden,
  menuCommands: new Set([...cowmarkHidden.menuCommands].filter((command) => command !== "checkForUpdates"))
};

export function editionHidden(): EditionHidden {
  if (!cowmarkEdition()) return stockHidden;
  return cowmarkUpdates() ? cowmarkHiddenWithUpdates : cowmarkHidden;
}

/** The runtime's features, with the ones CowMark doesn't offer turned off. */
export function editionFeatures(features: AppFeatureRuntime): AppFeatureRuntime {
  if (!cowmarkEdition()) return features;

  return {
    ...features,
    ai: false,
    export: false,
    markdownBundle: false,
    networkProxy: false,
    pandoc: false,
    s3ImageUpload: false,
    // Only release builds have an updater endpoint; local and CI builds are installed by hand.
    updater: cowmarkUpdates()
  };
}

/** A copy of `options` without the handlers whose command is hidden. */
export function withoutHiddenCommands<T extends object>(options: T, hidden: ReadonlySet<string>): T {
  if (hidden.size === 0) return options;

  return Object.fromEntries(Object.entries(options).filter(([key]) => !hidden.has(key))) as T;
}
