// CowMark's content lint: the shared Rust linter (`crates/cowmark-lint`, also behind
// `cowmark check` and CI) checks documents in the managed content folder, so the editor
// reports exactly what those do. Stock Markra and other files keep the editor's MDC lint.

import type { ExternalLintDiagnostic, ExternalMdcLint } from "@markra/editor/codemirror";
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
  "dateFormat",
  "emptyField",
  "fieldItem",
  "fieldObject",
  "frontmatterNotMapping",
  "frontmatterUnclosed",
  "frontmatterYaml",
  "imageAlt",
  "imageOutsideContent",
  "missingField",
  "missingFieldHere",
  "missingImage",
  "missingItemFieldHere",
  "unknownField",
  "unknownFieldItem",
  "unresolvedConflict"
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
