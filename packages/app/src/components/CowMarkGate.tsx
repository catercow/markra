import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { t, type I18nKey } from "@markra/shared";
import { Button } from "@markra/ui";
import { useAppLanguage } from "../hooks/useAppLanguage";
import { getAppRuntime } from "../runtime";
import {
  contentAccessError,
  joinFolderPath,
  managedFolderName,
  type ManagedContent
} from "../lib/content-access";

type Phase =
  | { kind: "loading" }
  | { kind: "sign-in"; error: string | null; waiting: boolean }
  | { kind: "setup"; dest: string; error: string | null; cloning: boolean }
  | { kind: "ready"; root: string };

type CowMarkGateProps = {
  site: string;
  renderWorkspace: (managed: ManagedContent) => ReactNode;
};

/**
 * CowMark revolves around one managed content folder. Signing in is needed only to
 * download it the first time; after that the folder opens on every launch, signed in
 * or not, and a token is requested only to download, save, or get latest.
 */
export function CowMarkGate({ renderWorkspace, site }: CowMarkGateProps) {
  const { language } = useAppLanguage();
  const label = useCallback((key: I18nKey) => t(language, key), [language]);
  const access = getAppRuntime().files.contentAccess;
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [email, setEmail] = useState<string | null>(null);
  const defaultFolderRef = useRef<string>("");

  const openRoot = useCallback((root: string, signedInEmail: string | null) => {
    setPhase({ kind: "ready", root });
    if (!access || !signedInEmail) return;
    // Quietly get latest when there is nothing local to combine; offline or a lost
    // session just means the folder opens as it is.
    access
      .status(root)
      .then((status) => {
        if (status.changes.length === 0 && status.ahead === 0) return access.update(root);
        return undefined;
      })
      .catch((error) => {
        if (contentAccessError(error).code === "signed_out") setEmail(null);
      });
  }, [access]);

  useEffect(() => {
    if (!access) return;
    let active = true;
    (async () => {
      const state = await access.state();
      if (!active) return;
      defaultFolderRef.current = state.defaultFolder;
      setEmail(state.email);
      if (state.folder && await access.isCheckout(state.folder)) {
        if (active) openRoot(state.folder, state.email);
        return;
      }
      if (!active) return;
      setPhase(state.email
        ? { kind: "setup", cloning: false, dest: state.defaultFolder, error: null }
        : { kind: "sign-in", error: null, waiting: false });
    })().catch((error) => {
      if (active) setPhase({ kind: "sign-in", error: contentAccessError(error).message, waiting: false });
    });

    return () => {
      active = false;
    };
  }, [access, openRoot]);

  const signIn = async () => {
    if (!access) return;
    setPhase({ kind: "sign-in", error: null, waiting: true });
    try {
      const result = await access.signIn(site);
      setEmail(result.email);
      setPhase({ kind: "setup", cloning: false, dest: defaultFolderRef.current, error: null });
    } catch (error) {
      const { code, message } = contentAccessError(error);
      setPhase({
        kind: "sign-in",
        error: code === "not_allowed"
          ? label("app.cowmark.signInNotAllowed")
          : label("app.cowmark.signInFailed").replace("{message}", message),
        waiting: false
      });
    }
  };

  const chooseLocation = async () => {
    const folder = await getAppRuntime().files.openMarkdownFolder({
      title: label("app.cowmark.setupChange")
    }).catch(() => null);
    if (!folder || phase.kind !== "setup") return;
    setPhase({ ...phase, dest: joinFolderPath(folder.path, managedFolderName), error: null });
  };

  const download = async () => {
    if (!access || phase.kind !== "setup") return;
    const dest = phase.dest;
    setPhase({ ...phase, cloning: true, error: null });
    try {
      await access.clone(dest);
      openRoot(dest, null);
    } catch (error) {
      const { code, message } = contentAccessError(error);
      if (code === "signed_out" || code === "not_allowed") {
        setEmail(null);
        setPhase({
          kind: "sign-in",
          error: code === "not_allowed" ? label("app.cowmark.signInNotAllowed") : null,
          waiting: false
        });
        return;
      }
      setPhase({ kind: "setup", cloning: false, dest, error: label("app.cowmark.setupFailed").replace("{message}", message) });
    }
  };

  const managed = useMemo<ManagedContent | null>(() => phase.kind === "ready"
    ? {
      email,
      root: phase.root,
      site,
      onSignedIn: setEmail,
      onSignedOut: () => setEmail(null)
    }
    : null, [email, phase, site]);

  if (managed) return <>{renderWorkspace(managed)}</>;

  return (
    <main
      className="cowmark-gate grid h-full w-full place-items-center bg-(--bg-primary) px-6 text-(--text-primary)"
      data-tauri-drag-region
    >
      <section className="flex w-full max-w-sm flex-col items-center gap-4 text-center" aria-live="polite">
        <h1 className="m-0 text-[22px] leading-7 font-[650] text-(--text-heading)">{label("app.cowmark.title")}</h1>
        <p className="m-0 text-[13px] leading-5 text-(--text-secondary)">{label("app.cowmark.subtitle")}</p>

        {phase.kind === "loading" ? (
          <p className="m-0 text-[13px] text-(--text-secondary)">{label("app.cowmark.loading")}</p>
        ) : null}

        {phase.kind === "sign-in" ? (
          <>
            <Button disabled={phase.waiting} variant="primary" onClick={() => { signIn().catch(() => {}); }}>
              {label("app.cowmark.signIn")}
            </Button>
            {phase.waiting ? (
              <p className="m-0 text-[12px] text-(--text-secondary)">{label("app.cowmark.signInWaiting")}</p>
            ) : null}
          </>
        ) : null}

        {phase.kind === "setup" ? (
          <>
            <p className="m-0 text-[13px] leading-5">{label("app.cowmark.setupDescription")}</p>
            <div className="w-full rounded-md border border-(--border-default) bg-(--bg-secondary) px-3 py-2 text-left">
              <p className="m-0 text-[11px] text-(--text-secondary)">{label("app.cowmark.setupLocation")}</p>
              <p className="m-0 text-[12px] break-all text-(--text-heading)" data-testid="cowmark-dest">{phase.dest}</p>
            </div>
            <div className="flex flex-wrap justify-center gap-2">
              <Button disabled={phase.cloning} onClick={() => { chooseLocation().catch(() => {}); }}>
                {label("app.cowmark.setupChange")}
              </Button>
              <Button disabled={phase.cloning} variant="primary" onClick={() => { download().catch(() => {}); }}>
                {phase.cloning ? label("app.cowmark.setupDownloading") : label("app.cowmark.setupDownload")}
              </Button>
            </div>
          </>
        ) : null}

        {(phase.kind === "sign-in" || phase.kind === "setup") && phase.error ? (
          <p className="m-0 text-[12px] leading-5 text-(--danger,#c2410c)" role="alert">{phase.error}</p>
        ) : null}
      </section>
    </main>
  );
}
