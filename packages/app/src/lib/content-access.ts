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
  save: (root: string, message: string) => Promise<ContentSyncResult>;
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

export type ContentAccessErrorCode = "conflict" | "not_allowed" | "offline" | "other" | "signed_out";

/** Splits the backend's `code: message` errors. */
export function contentAccessError(error: unknown): { code: ContentAccessErrorCode; message: string } {
  const text = String((error as Error)?.message ?? error ?? "").trim();
  const match = /^(conflict|not_allowed|offline|signed_out):\s*/u.exec(text);
  if (!match) return { code: "other", message: text };

  return { code: match[1] as ContentAccessErrorCode, message: text.slice(match[0].length) };
}

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

export const managedFolderName = "CaterCow Content";
