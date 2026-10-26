import { useCallback, useEffect, useRef, useState } from "react";
import { t, type AppLanguage } from "@markra/shared";
import { getAppRuntime } from "../runtime";
import {
  browserPreviewUrl,
  isPortBusy,
  readCompanionMode,
  writeCompanionMode,
  type ContentSaveProposal,
  type CowMarkCompanionStatus
} from "../lib/cowmark-companion";

/** The companion layout preference, remembered for this user. */
export function useCompanionMode() {
  const [preferred, setPreferred] = useState(readCompanionMode);
  const toggle = useCallback(() => {
    setPreferred((current) => {
      writeCompanionMode(!current);
      return !current;
    });
  }, []);

  return [preferred, toggle] as const;
}

type UseCowMarkCompanionOptions = {
  /** Off outside CowMark's managed folder. */
  enabled: boolean;
  language: AppLanguage;
  rootPath: string | null;
  site: string | null;
  /** Opens a file of the content folder in the editor. */
  onOpenFile: (path: string) => Promise<unknown>;
  /** Makes the save bar visible. */
  onShowSaveBar: () => unknown;
};

/**
 * Runs the browser preview and answers the `cowmark` command for the managed
 * folder: opens files it names and turns its publish proposals into a ticked
 * checklist for a person to save.
 */
export function useCowMarkCompanion({
  enabled,
  language,
  rootPath,
  site,
  onOpenFile,
  onShowSaveBar
}: UseCowMarkCompanionOptions) {
  const companion = getAppRuntime().files.cowmarkCompanion;
  const active = Boolean(enabled && companion && rootPath && site);
  const [proposal, setProposal] = useState<ContentSaveProposal | null>(null);
  const [revealRequest, setRevealRequest] = useState(0);
  // Non-zero while a Get latest from the `cowmark` command waits for the save bar.
  const [getLatestRequest, setGetLatestRequest] = useState(0);
  const statusRef = useRef<CowMarkCompanionStatus | null>(null);
  const handlersRef = useRef({ onOpenFile, onShowSaveBar });
  handlersRef.current = { onOpenFile, onShowSaveBar };

  // Listen before starting, so no request arrives without a handler.
  useEffect(() => {
    if (!active || !companion || !rootPath || !site) return;

    let disposed = false;
    let cleanup: (() => unknown) | null = null;
    companion
      .listen((request) => {
        if (request.action === "open") {
          handlersRef.current.onOpenFile(request.path)
            .then(() => setRevealRequest((count) => count + 1))
            .catch(() => {});
        } else if (request.action === "proposePublish") {
          handlersRef.current.onShowSaveBar();
          setProposal((current) => ({
            message: request.message,
            nonce: (current?.nonce ?? 0) + 1,
            paths: [...request.paths]
          }));
        } else if (request.action === "getLatest") {
          handlersRef.current.onShowSaveBar();
          setGetLatestRequest((count) => count + 1);
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else cleanup = stop;
      })
      .then(() => (disposed ? null : companion.start(rootPath, site)))
      .then((status) => {
        if (status) statusRef.current = status;
      })
      .catch(() => {});

    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [active, companion, rootPath, site]);

  const openInBrowser = useCallback(async (pagePath: string | null) => {
    if (!companion || !rootPath || !site) return;
    let status = statusRef.current;
    if (!status?.previewOrigin) {
      status = await companion.start(rootPath, site);
      statusRef.current = status;
    }
    if (status.previewOrigin && status.previewToken) {
      await getAppRuntime().window.openExternalUrl(browserPreviewUrl(status.previewOrigin, status.previewToken, pagePath));
      return;
    }
    throw new Error(
      isPortBusy(status.previewError)
        ? t(language, "app.cowmark.browserPreviewPortBusy")
        : t(language, "app.cowmark.browserPreviewFailed").replace("{message}", status.previewError ?? "")
    );
  }, [companion, language, rootPath, site]);

  const getLatestStarted = useCallback(() => setGetLatestRequest(0), []);

  return {
    active,
    getLatestRequest,
    getLatestStarted,
    openInBrowser: active ? openInBrowser : undefined,
    proposal,
    revealRequest
  };
}
