// CowMark, CaterCow's edition of Markra: one managed content folder, Google sign-in
// through the content preview, and "Save to studio". The GitHub token stays in the
// desktop backend; this side only sees emails, paths and results.

export type ContentAccessState = {
  defaultFolder: string;
  email: string | null;
  expiresAt: string | null;
  folder: string | null;
};

export type ContentChange = { path: string; status: string };

export type ContentGitStatus = {
  ahead: number;
  branch: string;
  changes: ContentChange[];
};

export type ContentSyncResult = { committed: boolean; head: string };

export type ContentAccessRuntime = {
  clone: (dest: string) => Promise<unknown>;
  isCheckout: (path: string) => Promise<boolean>;
  /** Commits only `paths`; other changes stay local as drafts. */
  save: (root: string, message: string, paths: string[]) => Promise<ContentSyncResult>;
  setFolder: (path: string | null) => Promise<unknown>;
  signIn: (site: string) => Promise<{ email: string; expiresAt: string }>;
  signOut: () => Promise<unknown>;
  state: () => Promise<ContentAccessState>;
  status: (root: string) => Promise<ContentGitStatus>;
  update: (root: string) => Promise<ContentSyncResult>;
};

/** What the workspace needs to run as CowMark's managed folder. */
export type ManagedContent = {
  email: string | null;
  root: string;
  site: string;
  onSignedIn: (email: string) => unknown;
  onSignedOut: () => unknown;
};

export type ContentAccessErrorCode = "conflict" | "lint" | "not_allowed" | "offline" | "other" | "signed_out";

export type ContentAccessError = {
  code: ContentAccessErrorCode;
  /** Folder-relative files a `conflict` left with markers to resolve, or that `lint` refused. */
  files: string[];
  message: string;
  /** For `lint`: the first error in each refused file. */
  problems: LintProblem[];
};

/** A ticked page's first lint error, which made Save to studio refuse it. */
export type LintProblem = { file: string; line: number; message: string };

const conflictFilesPrefix = "\nconflict-files: ";
const lintProblemsPrefix = "\nlint-problems: ";

function jsonList(list: string): unknown[] {
  try {
    const parsed: unknown = JSON.parse(list);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function conflictFiles(list: string) {
  return jsonList(list).filter((file): file is string => typeof file === "string");
}

function lintProblems(list: string) {
  return jsonList(list).filter((problem): problem is LintProblem => {
    const value = problem as Partial<LintProblem> | null;
    return typeof value?.file === "string" && typeof value.line === "number" && typeof value.message === "string";
  });
}

/** Splits the backend's `code: message` errors, and a conflict's or lint refusal's trailing list. */
export function contentAccessError(error: unknown): ContentAccessError {
  const text = String((error as Error)?.message ?? error ?? "").trim();
  const match = /^(conflict|lint|not_allowed|offline|signed_out):\s*/u.exec(text);
  if (!match) return { code: "other", files: [], message: text, problems: [] };

  const code = match[1] as ContentAccessErrorCode;
  const rest = text.slice(match[0].length);
  const prefix = code === "lint" ? lintProblemsPrefix : conflictFilesPrefix;
  const split = rest.indexOf(prefix);
  const list = split < 0 ? "" : rest.slice(split + prefix.length);
  const problems = code === "lint" ? lintProblems(list) : [];
  return {
    code,
    files: code === "lint" ? problems.map((problem) => problem.file) : split < 0 ? [] : conflictFiles(list),
    message: split < 0 ? rest : rest.slice(0, split),
    problems
  };
}

/** A three-way merge of an open document with a change made on disk. */
export type MergedText = { conflicts: number; text: string };
export type MergeText = (base: string, mine: string, theirs: string) => Promise<MergedText>;

let previewSiteOverride: string | null | undefined;

/** Set only in the CowMark build (VITE_CATERCOW_PREVIEW_SITE); null in stock Markra. */
export function catercowPreviewSite(): string | null {
  if (previewSiteOverride !== undefined) return previewSiteOverride;

  const value = String(import.meta.env.VITE_CATERCOW_PREVIEW_SITE ?? "").trim();
  return value || null;
}

export function setCatercowPreviewSiteForTests(site: string | null | undefined) {
  previewSiteOverride = site;
}

export function joinFolderPath(parent: string, name: string) {
  const separator = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  return `${parent.replace(/[\\/]+$/u, "")}${separator}${name}`;
}

export const managedFolderName = "catercow-content";

/** A git path (always `/`) inside the managed folder, in the folder's own separators. */
export function managedFilePath(root: string, relativePath: string) {
  const backslashes = root.includes("\\") && !root.includes("/");
  return joinFolderPath(root, backslashes ? relativePath.replaceAll("/", "\\") : relativePath);
}
