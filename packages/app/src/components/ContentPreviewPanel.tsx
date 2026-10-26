import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent
} from "react";
import { clampNumber, t, type AppLanguage, type I18nKey } from "@markra/shared";
import { IconButton } from "@markra/ui";
import { ExternalLink, FilePlus, RotateCw, TriangleAlert, X } from "lucide-react";
import { getAppRuntime } from "../runtime";
import type { ManagedContent } from "../lib/content-access";
import { ContentSaveBar } from "./ContentSaveBar";
import {
  contentFsPathFor,
  contentPreviewEventLabel,
  draftMessage,
  messageForEvent,
  orderContentPreviewEvents,
  type ContentPreviewConfig,
  type ContentPreviewEvent,
  type ContentPreviewMessage
} from "../lib/content-preview";
import { navigationFlags, type ContentSaveProposal } from "../lib/cowmark-companion";

const pollIntervalMs = 1000;
const typingDelayMs = 300;
const connectTimeoutMs = 20_000;

type ConnectionStatus = "connecting" | "ready" | "offline";

type ContentPreviewPanelProps = {
  /** CowMark's companion layout: the panel fills the window and can't be closed or resized. */
  fill?: boolean;
  /** CowMark: opens the page in the browser preview; rejects with a message to show. */
  onOpenInBrowser?: (pagePath: string | null) => Promise<unknown>;
  /** Passed to the save bar; see ContentSaveBar. */
  proposal?: ContentSaveProposal | null;
  /** Passed to the save bar; see ContentSaveBar. */
  getLatestRequest?: number;
  onGetLatestStarted?: () => unknown;
  /** Bumped to show the open document again, e.g. after the `cowmark` command opened it. */
  revealRequest?: number;
  /** Writes open documents to disk before Save to studio. */
  beforeSave?: () => Promise<unknown>;
  /** CowMark's managed folder: shows the sign-in and Save to studio footer. */
  managed?: ManagedContent;
  onConflictFiles?: (paths: string[]) => unknown;
  activeDocument: { content: string; path: string | null } | null;
  config: ContentPreviewConfig;
  language: AppLanguage;
  maxWidth: number;
  minWidth: number;
  rootPath: string;
  width: number;
  onClose: () => unknown;
  /** CowMark: asks for a title and creates a blog post. */
  onNewPost?: () => unknown;
  onResize: (width: number) => unknown;
  onResizeEnd?: () => unknown;
  onResizeStart?: () => unknown;
};

export function ContentPreviewPanel({
  activeDocument,
  fill = false,
  onOpenInBrowser,
  proposal,
  getLatestRequest = 0,
  onGetLatestStarted,
  revealRequest = 0,
  beforeSave,
  managed,
  onConflictFiles,
  config,
  language,
  maxWidth,
  minWidth,
  rootPath,
  width,
  onClose,
  onNewPost,
  onResize,
  onResizeEnd,
  onResizeStart
}: ContentPreviewPanelProps) {
  const label = (key: I18nKey) => t(language, key);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const [frameKey, setFrameKey] = useState(0);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [changes, setChanges] = useState<string[]>([]);
  const [warning, setWarning] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pagePath, setPagePath] = useState<string | null>(null);
  const statusRef = useRef<ConnectionStatus>("connecting");
  statusRef.current = status;
  const sentDocumentRef = useRef<{ content: string; fsPath: string } | null>(null);
  const activeDocumentRef = useRef(activeDocument);
  activeDocumentRef.current = activeDocument;
  const contentPreview = getAppRuntime().files.contentPreview;

  const post = useCallback((outgoing: ContentPreviewMessage) => {
    iframeRef.current?.contentWindow?.postMessage(outgoing.message, config.origin, outgoing.transfer);
  }, [config.origin]);

  const noteChanges = useCallback((events: ContentPreviewEvent[]) => {
    if (!events.length) return;
    setChanges((current) => {
      const next = new Set(current);
      for (const event of events) {
        if (event.kind !== "skipped") next.add(contentPreviewEventLabel(event));
      }
      return [...next].sort();
    });
  }, []);

  const send = useCallback(async (events: ContentPreviewEvent[], navigate: boolean) => {
    if (!contentPreview) return;
    const ordered = orderContentPreviewEvents(events);
    const navigateFlags = navigationFlags(ordered, navigate);
    for (const [index, event] of ordered.entries()) {
      try {
        const outgoing = await messageForEvent(event, navigateFlags[index], (path) => contentPreview.readMedia(rootPath, path));
        if (outgoing) post(outgoing);
      } catch (cause) {
        setError(`${contentPreviewEventLabel(event)}: ${String((cause as Error)?.message ?? cause)}`);
      }
    }
    noteChanges(events);
  }, [contentPreview, noteChanges, post, rootPath]);

  // The open document, unsaved edits included. Navigates when it is a different page.
  const sendActiveDocument = useCallback(() => {
    const current = activeDocumentRef.current;
    const fsPath = contentFsPathFor(rootPath, current?.path);
    if (!current || !fsPath || statusRef.current !== "ready") return;

    const previous = sentDocumentRef.current;
    if (previous?.fsPath === fsPath && previous.content === current.content) return;

    post(draftMessage(fsPath, current.content, previous?.fsPath !== fsPath));
    sentDocumentRef.current = { content: current.content, fsPath };
  }, [post, rootPath]);

  useEffect(() => {
    const handleMessage = (event: MessageEvent) => {
      if (event.origin !== config.origin || event.source !== iframeRef.current?.contentWindow) return;
      const data = (event.data ?? {}) as Record<string, unknown>;

      if (data.type === "catercow:draft:ready") {
        setStatus("ready");
        statusRef.current = "ready";
        setChanges([]);
        setError(null);
        sentDocumentRef.current = null;
        if (!contentPreview) return;
        contentPreview
          .pending(rootPath, String(data.contentCommit ?? ""))
          .then(async (pending) => {
            setWarning(pending.warning ?? null);
            await send(pending.events, false);
            sendActiveDocument();
          })
          .catch((cause) => {
            setError(t(language, "app.contentPreviewLoadFailed").replace("{message}", String(cause?.message ?? cause)));
          });
      } else if (data.type === "catercow:draft:rendered") {
        if (typeof data.path === "string") setPagePath(data.path);
      } else if (data.type === "catercow:draft:error") {
        setError(
          t(language, "app.contentPreviewRenderFailed")
            .replace("{file}", String(data.fsPath ?? ""))
            .replace("{message}", String(data.message ?? ""))
        );
      }
    };

    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, [config.origin, contentPreview, language, rootPath, send, sendActiveDocument]);

  // Saved files, from any app. The first poll only records the folder's state.
  useEffect(() => {
    if (!contentPreview || status !== "ready") return;

    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = async () => {
      try {
        const events = await contentPreview.poll(rootPath);
        if (active && events.length) await send(events, true);
      } catch {
        // A failed scan is retried on the next tick.
      }
      if (active) timer = setTimeout(tick, pollIntervalMs);
    };
    tick();

    return () => {
      active = false;
      clearTimeout(timer);
      contentPreview.stop(rootPath).catch(() => {});
    };
  }, [contentPreview, rootPath, send, status]);

  useEffect(() => {
    if (status !== "ready") return;
    const timer = setTimeout(sendActiveDocument, typingDelayMs);
    return () => clearTimeout(timer);
  }, [activeDocument?.content, activeDocument?.path, sendActiveDocument, status]);

  useEffect(() => {
    if (!revealRequest) return;
    sentDocumentRef.current = null;
    sendActiveDocument();
  }, [revealRequest, sendActiveDocument]);

  useEffect(() => {
    setStatus("connecting");
    statusRef.current = "connecting";
    const timer = setTimeout(() => {
      if (statusRef.current === "connecting") setStatus("offline");
    }, connectTimeoutMs);
    return () => clearTimeout(timer);
  }, [config.site, frameKey]);

  const reload = () => {
    setPagePath(null);
    setFrameKey((key) => key + 1);
  };

  const openInBrowser = () => {
    if (onOpenInBrowser) {
      setError(null);
      onOpenInBrowser(pagePath).catch((cause) => setError(String((cause as Error)?.message ?? cause)));
      return;
    }
    const target = pagePath ? new URL(pagePath, config.site).href : config.site;
    getAppRuntime().window.openExternalUrl(target).catch(() => {});
  };

  const handleResizePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const startX = event.clientX;
    const startWidth = width;
    const previousCursor = document.body.style.cursor;
    const previousUserSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    // The iframe would swallow pointer events while the cursor is over it.
    if (iframeRef.current) iframeRef.current.style.pointerEvents = "none";
    onResizeStart?.();

    const handlePointerMove = (moveEvent: PointerEvent) => {
      onResize(clampNumber(startWidth + startX - moveEvent.clientX, minWidth, maxWidth) ?? startWidth);
    };
    const handlePointerUp = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerUp);
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousUserSelect;
      if (iframeRef.current) iframeRef.current.style.pointerEvents = "";
      onResizeEnd?.();
    };
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerUp);
  };

  const handleResizeKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const next = {
      ArrowLeft: width + 24,
      ArrowRight: width - 24,
      End: maxWidth,
      Home: minWidth
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    onResize(clampNumber(next, minWidth, maxWidth) ?? width);
  };

  const summary = status === "connecting"
    ? label("app.contentPreviewConnecting")
    : status === "offline"
      ? label("app.contentPreviewNotConnected")
      : changes.length === 0
        ? label("app.contentPreviewNoChanges")
        : changes.length === 1
          ? label("app.contentPreviewChange")
          : label("app.contentPreviewChanges").replace("{count}", String(changes.length));

  return (
    <aside
      className="content-preview-panel relative flex h-full min-h-0 w-full flex-col border-l border-(--border-default) bg-(--bg-secondary) text-(--text-primary)"
      role="complementary"
      aria-label={label("app.contentPreview")}
      style={fill ? { width: "100%" } : { maxWidth: width, minWidth: width, width }}
    >
      {fill ? null : <div
        className="absolute top-10 bottom-0 left-0 z-30 w-2 cursor-col-resize touch-none outline-none"
        role="separator"
        tabIndex={0}
        aria-label={label("app.contentPreviewResize")}
        aria-orientation="vertical"
        aria-valuemin={minWidth}
        aria-valuemax={maxWidth}
        aria-valuenow={width}
        onKeyDown={handleResizeKeyDown}
        onPointerDown={handleResizePointerDown}
      />}
      <header className="flex min-h-12 shrink-0 items-center gap-1 border-b border-(--border-default) px-2 py-1.5">
        {fill ? null : (
          <IconButton label={label("app.contentPreviewClose")} size="icon-md" onClick={onClose}>
            <X aria-hidden="true" size={15} />
          </IconButton>
        )}
        <div className="min-w-0 flex-1 px-1">
          <h2 className="m-0 truncate text-[13px] leading-5 font-[560] text-(--text-heading)">
            {label("app.contentPreview")}
          </h2>
          <p
            className={`m-0 truncate text-[11px] leading-4 ${status === "offline" ? "text-(--danger,#c2410c)" : "text-(--text-secondary)"}`}
            title={changes.length ? changes.join("\n") : summary}
          >
            {summary}
          </p>
        </div>
        {onNewPost ? (
          <IconButton label={label("app.cowmark.newPost")} size="icon-md" onClick={onNewPost}>
            <FilePlus aria-hidden="true" size={14} />
          </IconButton>
        ) : null}
        <IconButton label={label("app.contentPreviewReload")} size="icon-md" onClick={reload}>
          <RotateCw aria-hidden="true" size={14} />
        </IconButton>
        <IconButton label={label("app.contentPreviewOpenInBrowser")} size="icon-md" onClick={openInBrowser}>
          <ExternalLink aria-hidden="true" size={14} />
        </IconButton>
      </header>
      {warning || error ? (
        <div className="flex shrink-0 items-start gap-1.5 border-b border-(--border-default) px-3 py-1.5 text-[11px] leading-4 text-(--text-secondary)">
          <TriangleAlert aria-hidden="true" className="mt-px shrink-0" size={12} />
          <span className="min-w-0 break-words">{error ?? warning}</span>
        </div>
      ) : null}
      <iframe
        key={frameKey}
        ref={iframeRef}
        className="min-h-0 w-full flex-1 border-0 bg-white"
        src={config.site}
        title={label("app.contentPreview")}
      />
      {managed ? (
        <ContentSaveBar
          beforeSave={beforeSave}
          language={language}
          managed={managed}
          onConflictFiles={onConflictFiles}
          proposal={proposal}
          getLatestRequest={getLatestRequest}
          onGetLatestStarted={onGetLatestStarted}
        />
      ) : null}
    </aside>
  );
}
