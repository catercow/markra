// CaterCow content preview: frames the marketing site and posts local drafts to it
// with the `catercow:draft*` messages that js/nuxt4/modules/draft-preview handles.

export const contentPreviewConfigRelativePath = "preview.json";

export type ContentPreviewEvent =
  | { kind: "draft"; fsPath: string; content: string }
  | { kind: "remove"; fsPath: string }
  | { kind: "media"; path: string; mime?: string | null }
  | { kind: "skipped"; path: string; reason: string };

export type ContentPreviewPending = {
  base?: string | null;
  events: ContentPreviewEvent[];
  warning?: string | null;
};

export type ContentPreviewRuntime = {
  pending: (rootPath: string, previewCommit: string) => Promise<ContentPreviewPending>;
  poll: (rootPath: string) => Promise<ContentPreviewEvent[]>;
  readMedia: (rootPath: string, path: string) => Promise<ArrayBuffer>;
  stop: (rootPath: string) => Promise<unknown>;
};

export type ContentPreviewConfig = {
  origin: string;
  site: string;
};

export type ContentPreviewMessage = {
  message: Record<string, unknown>;
  transfer: Transferable[];
};

const contentExtensions = /\.(md|ya?ml|json)$/iu;

export function contentPreviewConfigPathForRoot(rootPath: string) {
  const separator = /[\\/]$/u.test(rootPath) ? "" : "/";

  return `${rootPath}${separator}${contentPreviewConfigRelativePath}`;
}

/** Reads `{ "site": "https://…" }`; only http(s) sites are accepted. */
export function parseContentPreviewConfig(text: string | null): ContentPreviewConfig | null {
  if (!text) return null;

  try {
    const site = (JSON.parse(text) as { site?: unknown })?.site;
    if (typeof site !== "string") return null;

    const url = new URL(site);
    if (url.protocol !== "https:" && url.protocol !== "http:") return null;

    return { origin: url.origin, site: url.href };
  } catch {
    return null;
  }
}

function normalizePath(path: string) {
  return path.replace(/\\/gu, "/").replace(/\/+$/u, "");
}

/** The path the site knows a document by (relative to `content/`), or null when it isn't site content. */
export function contentFsPathFor(rootPath: string, documentPath: string | null | undefined) {
  if (!documentPath) return null;

  const prefix = `${normalizePath(rootPath)}/content/`;
  const path = normalizePath(documentPath);
  const caseInsensitive = /^[a-z]:\//iu.test(prefix);
  const matches = caseInsensitive
    ? path.toLowerCase().startsWith(prefix.toLowerCase())
    : path.startsWith(prefix);
  if (!matches) return null;

  const fsPath = path.slice(prefix.length);
  return fsPath && contentExtensions.test(fsPath) ? fsPath : null;
}

export function draftMessage(fsPath: string, content: string, navigate: boolean): ContentPreviewMessage {
  return { message: { type: "catercow:draft", fsPath, content, navigate }, transfer: [] };
}

/** The message for one change, or null when the site can't use it. */
export async function messageForEvent(
  event: ContentPreviewEvent,
  navigate: boolean,
  readMedia: (path: string) => Promise<ArrayBuffer>
): Promise<ContentPreviewMessage | null> {
  switch (event.kind) {
    case "draft":
      return draftMessage(event.fsPath, event.content, navigate);
    case "remove":
      return { message: { type: "catercow:draft:remove", fsPath: event.fsPath }, transfer: [] };
    case "media": {
      if (!event.mime) {
        return { message: { type: "catercow:draft:media", path: event.path, bytes: null, mime: "" }, transfer: [] };
      }
      const bytes = await readMedia(event.path);
      return { message: { type: "catercow:draft:media", path: event.path, bytes, mime: event.mime }, transfer: [bytes] };
    }
    default:
      return null;
  }
}

/** Names a change for the panel's list. */
export function contentPreviewEventLabel(event: ContentPreviewEvent) {
  return event.kind === "draft" || event.kind === "remove" ? event.fsPath : event.path.replace(/^\//u, "");
}

/** Media first, so pages that use a new image find it when they render. */
export function orderContentPreviewEvents(events: ContentPreviewEvent[]) {
  return [...events].sort((left, right) => Number(right.kind === "media") - Number(left.kind === "media"));
}
