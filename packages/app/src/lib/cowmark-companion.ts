// CowMark as a hub beside another editor: the companion layout, the browser
// preview at http://localhost:4100 and requests from the `cowmark` command
// (open a file, propose files to save). The servers live in the desktop backend.

import type { ContentPreviewEvent } from "./content-preview";

export type CowMarkCompanionStatus = {
  /** `http://localhost:4100` while the browser preview runs. */
  previewOrigin: string | null;
  previewToken: string | null;
  /** Starts with `port_busy:` when another program holds the port. */
  previewError: string | null;
  controlError: string | null;
};

export type CowMarkControlRequest =
  | { action: "open"; path: string }
  | { action: "proposePublish"; paths: string[]; message: string }
  | { action: "getLatest" };

export type CowMarkCompanionRuntime = {
  /** Starts the browser preview and the control socket; safe to call again. */
  start: (root: string, site: string) => Promise<CowMarkCompanionStatus>;
  /** Requests from the `cowmark` command for this window. */
  listen: (handler: (request: CowMarkControlRequest) => unknown) => Promise<() => unknown>;
};

/** Files a client asked to save; ContentSaveBar ticks exactly these. */
export type ContentSaveProposal = {
  message: string;
  nonce: number;
  paths: string[];
};

export type BrowserPreview = {
  error: string | null;
  origin: string | null;
  token: string | null;
};

const companionModeStorageKey = "cowmark.companionMode";

function storage() {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

export function readCompanionMode() {
  try {
    return storage()?.getItem(companionModeStorageKey) === "1";
  } catch {
    return false;
  }
}

export function writeCompanionMode(on: boolean) {
  try {
    storage()?.setItem(companionModeStorageKey, on ? "1" : "0");
  } catch {
    // Not remembering the layout is harmless.
  }
}

/** The browser preview URL for a site path such as `/blog/my-post`. */
export function browserPreviewUrl(origin: string, token: string, pagePath: string | null) {
  const path = pagePath?.startsWith("/") && !pagePath.startsWith("/__cowmark/") ? pagePath : "/";
  const url = new URL(path, origin);
  if (url.origin !== new URL(origin).origin) return `${origin}/?token=${encodeURIComponent(token)}`;
  url.searchParams.delete("token");
  url.searchParams.set("token", token);
  return url.href;
}

export function isPortBusy(error: string | null | undefined) {
  return Boolean(error?.startsWith("port_busy:"));
}

/**
 * Whether each event should navigate the site: only the last saved page in a
 * batch, so the preview follows the most recently saved file.
 */
export function navigationFlags(events: ContentPreviewEvent[], navigate: boolean) {
  const last = events.map((event) => event.kind).lastIndexOf("draft");
  return events.map((_, index) => navigate && index === last);
}
