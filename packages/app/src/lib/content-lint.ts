// CowMark's content lint: the shared Rust linter (`crates/cowmark-lint`, also behind
// `cowmark check` and CI) checks documents in the managed content folder, so the editor
// reports exactly what those do. Stock Markra and other files keep the editor's MDC lint.

import type { ExternalLintDiagnostic, ExternalMdcLint, MdcCsvConvert } from "@markra/editor/codemirror";
import { t, type AppLanguage } from "@markra/shared";
import { catercowPreviewSite } from "./content-access";
import { rootRelativePath } from "./cowmark-pages";

/** Message templates for the Rust linter, by its camelCase names. */
export type ContentLintMessages = Record<(typeof mdcLintKeys)[number] | (typeof contentLintKeys)[number], string>;

/** Lints `text` as the folder-relative `path` (`content/blog/a.md`) in the content folder `root`. */
export type ContentLint = (
  root: string,
  path: string,
  text: string,
  messages: ContentLintMessages
) => Promise<ExternalLintDiagnostic[]>;

/** A page with component input in canonical form (`cowmark_format`), and the problems left. */
export interface ContentFormatResult {
  text: string;
  changed: boolean;
  diagnostics: ExternalLintDiagnostic[];
}

export type ContentFormat = (
  root: string,
  path: string,
  text: string,
  messages: ContentLintMessages
) => Promise<ContentFormatResult>;

/** CSV as a Markdown table (`cowmark_csv_table`); null when the text isn't a table. */
export type ContentCsvTable = (
  root: string,
  text: string,
  csvType: boolean,
  component: string | null,
  wrap: boolean
) => Promise<{ markdown: string; problems: string[] } | null>;

// Shared with the editor's MDC lint: `editor.mdcLint.*`.
const mdcLintKeys = [
  "expectBoolean",
  "expectItem",
  "expectList",
  "expectNumber",
  "expectOption",
  "expectText",
  "invalidYaml",
  "missingItemField",
  "missingProp",
  "unclosed",
  "unclosedProps",
  "unknownComponent",
  "unknownItemField",
  "unknownProp"
] as const;

// Rules only the Rust linter has: `editor.contentLint.*`.
const contentLintKeys = [
  "brokenLink",
  "cellFix",
  "codeInvalid",
  "dateFormat",
  "emptyField",
  "fieldItem",
  "fieldObject",
  "frontmatterNotMapping",
  "frontmatterUnclosed",
  "frontmatterYaml",
  "galleryEmpty",
  "galleryText",
  "imageAlt",
  "imageRelative",
  "linkInvalid",
  "missingField",
  "missingFieldHere",
  "missingImage",
  "missingItemFieldHere",
  "priceApproximate",
  "priceComma",
  "priceCurrency",
  "priceDecimals",
  "priceInvalid",
  "priceRange",
  "propFix",
  "rawHtml",
  "reviewLinkInvalid",
  "searchLinkInvalid",
  "searchLinkParam",
  "searchLinkSuggest",
  "searchLinkValue",
  "tableColumns",
  "tableDelimiter",
  "tableDuplicateColumn",
  "tableEmptyCell",
  "tableExtra",
  "tableExtraCells",
  "tableHeaderRow",
  "tableMissing",
  "tableMissingColumn",
  "tableNoRows",
  "tableUnknownColumn",
  "tableUnnamedColumn",
  "unknownField",
  "unknownFieldItem",
  "unresolvedConflict",
  "unsafeAttribute",
  "unsafeLink"
] as const;

export function contentLintMessages(language: AppLanguage): ContentLintMessages {
  const messages = {} as ContentLintMessages;
  for (const key of mdcLintKeys) messages[key] = t(language, `editor.mdcLint.${key}`);
  for (const key of contentLintKeys) messages[key] = t(language, `editor.contentLint.${key}`);
  return messages;
}

type ContentLintOptions = {
  /** The runtime's linter; absent outside the desktop app. */
  lint: ContentLint | undefined;
  /** The managed content folder; null outside CowMark. */
  contentRoot: () => string | null | undefined;
  documentPath: () => string | null | undefined;
  language: AppLanguage;
};

/**
 * The editor's external lint for CowMark: Markdown files inside the content folder go to the
 * Rust linter; anything else returns null and keeps the built-in MDC lint. Undefined in stock
 * Markra or without the runtime linter, so the editor behaves as before.
 */
export function contentLintSource({ contentRoot, documentPath, language, lint }: ContentLintOptions): ExternalMdcLint | undefined {
  if (!lint || catercowPreviewSite() === null) return undefined;
  const messages = contentLintMessages(language);
  return (text) => {
    const root = contentRoot();
    const path = root ? rootRelativePath(root, documentPath()) : null;
    if (!root || !path || !/\.md$/iu.test(path)) return null;
    return lint(root, path, text, messages);
  };
}

/** The folder-relative path of a content page (`content/…/*.md`) in the managed folder, else null. */
export function managedContentPage(root: string | null | undefined, documentPath: string | null | undefined) {
  if (!root || catercowPreviewSite() === null) return null;
  const path = rootRelativePath(root, documentPath);
  return path && /^content\/.+\.md$/iu.test(path) ? path : null;
}

/**
 * The text a content page is saved as: tables, prices, codes and links in canonical form.
 * Null when nothing changes, the page isn't a managed content page, or formatting isn't
 * available or fails (the page is then saved as typed and the save guard still applies).
 */
export async function formatContentPage({
  documentPath,
  format,
  language,
  root,
  text,
}: {
  documentPath: string | null | undefined;
  format: ContentFormat | undefined;
  language: AppLanguage;
  root: string | null | undefined;
  text: string;
}) {
  const path = managedContentPage(root, documentPath);
  if (!format || !root || !path) return null;
  try {
    const result = await format(root, path, text, contentLintMessages(language));
    return result.changed && result.text !== text ? result.text : null;
  } catch {
    return null;
  }
}

/**
 * The editor's CSV paste for CowMark content pages: converted by the shared Rust code, with a
 * table component's columns when the caret is in one. Undefined in stock Markra or without the
 * runtime command, so pasting stays as before.
 */
export function contentCsvSource({
  contentRoot,
  csvTable,
  documentPath,
}: {
  contentRoot: () => string | null | undefined;
  csvTable: ContentCsvTable | undefined;
  documentPath: () => string | null | undefined;
}): MdcCsvConvert | undefined {
  if (!csvTable || catercowPreviewSite() === null) return undefined;
  return async ({ component, csvType, text }) => {
    const root = contentRoot();
    if (!root || !managedContentPage(root, documentPath())) return null;
    const table = await csvTable(root, text, csvType, component, false);
    return table?.markdown ?? null;
  };
}
