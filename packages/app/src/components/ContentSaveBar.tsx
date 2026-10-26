import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { t, type AppLanguage, type I18nKey } from "@markra/shared";
import { Button, Textarea } from "@markra/ui";
import { getAppRuntime } from "../runtime";
import { contentAccessError, type ContentGitStatus, type ManagedContent } from "../lib/content-access";
import { pruneSelection, toggleSelection } from "../lib/content-selection";
import type { ContentSaveProposal } from "../lib/cowmark-companion";

const statusPollMs = 4000;

type Notice = { tone: "error" | "info"; text: string } | null;

type ContentSaveBarProps = {
  language: AppLanguage;
  managed: ManagedContent;
  /** Writes open documents to disk first, so git sees unsaved edits. */
  beforeSave?: () => Promise<unknown>;
  /** Folder-relative files left with conflict markers; shown so the conflicts can be resolved. */
  onConflictFiles?: (paths: string[]) => unknown;
  /** From the `cowmark` command: tick exactly these files and fill in the message. Never saves. */
  proposal?: ContentSaveProposal | null;
  /** Non-zero when the `cowmark` command asked for Get latest; cleared through onGetLatestStarted. */
  getLatestRequest?: number;
  onGetLatestStarted?: () => unknown;
};

/** The preview panel's footer in CowMark: who is signed in, what changed, Save to studio. */
export function ContentSaveBar({
  beforeSave,
  getLatestRequest = 0,
  language,
  managed,
  onConflictFiles,
  onGetLatestStarted,
  proposal
}: ContentSaveBarProps) {
  const label = useCallback((key: I18nKey) => t(language, key), [language]);
  const access = getAppRuntime().files.contentAccess;
  const [status, setStatus] = useState<ContentGitStatus | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<"save" | "update" | "sign-in" | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [sessionLost, setSessionLost] = useState(false);
  // Nothing is ticked by default: publishing is a deliberate choice per file.
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
  const changedPaths = useMemo(() => status?.changes.map((change) => change.path) ?? [], [status]);

  useEffect(() => {
    setSelected((current) => pruneSelection(current, changedPaths));
  }, [changedPaths]);

  const refresh = useCallback(async () => {
    if (!access) return;
    try {
      setStatus(await access.status(managed.root));
    } catch {
      // A failed listing is retried on the next tick.
    }
  }, [access, managed.root]);

  useEffect(() => {
    refresh();
    const timer = setInterval(() => { refresh(); }, statusPollMs);
    return () => clearInterval(timer);
  }, [refresh]);

  const footerRef = useRef<HTMLElement | null>(null);
  const [flash, setFlash] = useState(false);
  const proposalNonce = proposal?.nonce;
  const proposalRef = useRef(proposal);
  proposalRef.current = proposal;
  const appliedProposalRef = useRef<number | null>(null);
  useEffect(() => {
    const current = proposalRef.current;
    if (!current || appliedProposalRef.current === current.nonce) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // Refresh first, so the proposed files are in the list before they're ticked.
    refresh().finally(() => {
      if (!active) return;
      appliedProposalRef.current = current.nonce;
      setSelected(new Set(current.paths));
      setMessage(current.message);
      setNotice({ tone: "info", text: label("app.cowmark.proposalReady") });
      setFlash(true);
      footerRef.current?.scrollIntoView?.({ block: "nearest" });
      timer = setTimeout(() => setFlash(false), 1600);
    });
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [label, proposalNonce, refresh]);

  const handleFailure = (error: unknown, failedKey: I18nKey) => {
    const { code, files, message: text } = contentAccessError(error);
    if (code === "conflict" && files.length > 0) {
      setNotice({ tone: "error", text: label("app.cowmark.resolveConflicts").replace("{files}", files.join(", ")) });
      onConflictFiles?.(files);
    } else if (code === "signed_out") {
      setSessionLost(true);
      managed.onSignedOut();
      setNotice({ tone: "error", text: label("app.cowmark.signInAgain") });
    } else if (code === "not_allowed") {
      managed.onSignedOut();
      setNotice({ tone: "error", text: label("app.cowmark.notAllowed") });
    } else if (code === "conflict" || code === "offline") {
      setNotice({ tone: "error", text });
    } else {
      setNotice({ tone: "error", text: label(failedKey).replace("{message}", text) });
    }
  };

  const save = async () => {
    if (!access) return;
    setBusy("save");
    setNotice(null);
    try {
      await beforeSave?.();
      await access.save(managed.root, message, [...selected]);
      setMessage("");
      setSelected(new Set());
      setNotice({ tone: "info", text: label("app.cowmark.saved") });
    } catch (error) {
      handleFailure(error, "app.cowmark.saveFailed");
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const update = async () => {
    if (!access) return;
    setBusy("update");
    setNotice(null);
    try {
      await beforeSave?.();
      await access.update(managed.root);
      setNotice({ tone: "info", text: label("app.cowmark.updated") });
    } catch (error) {
      handleFailure(error, "app.cowmark.updateFailed");
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const updateRef = useRef(update);
  updateRef.current = update;
  useEffect(() => {
    // A request that arrives mid-save waits until the bar is free again.
    if (!getLatestRequest || busy !== null) return;
    onGetLatestStarted?.();
    updateRef.current().catch(() => {});
  }, [busy, getLatestRequest, onGetLatestStarted]);

  const signIn = async () => {
    if (!access) return;
    setBusy("sign-in");
    setNotice(null);
    try {
      const result = await access.signIn(managed.site);
      setSessionLost(false);
      managed.onSignedIn(result.email);
    } catch (error) {
      const { code, message: text } = contentAccessError(error);
      setNotice({
        tone: "error",
        text: code === "not_allowed"
          ? label("app.cowmark.notAllowed")
          : label("app.cowmark.signInFailed").replace("{message}", text)
      });
    } finally {
      setBusy(null);
    }
  };

  const signOut = () => {
    access?.signOut().catch(() => {});
    setSessionLost(false);
    setNotice(null);
    managed.onSignedOut();
  };

  const changeCount = status?.changes.length ?? 0;
  const selectedCount = selected.size;
  const canSave = selectedCount > 0 || (status?.ahead ?? 0) > 0;
  const allSelected = changeCount > 0 && selectedCount === changeCount;
  const toggleAll = () => setSelected(allSelected ? new Set() : new Set(changedPaths));
  const summary = changeCount === 0
    ? (status?.ahead ? label("app.cowmark.waitingToSend") : label("app.cowmark.noChanges"))
    : changeCount === 1
      ? label("app.cowmark.changeOne")
      : label("app.cowmark.changeCount").replace("{count}", String(changeCount));

  return (
    <footer
      ref={footerRef}
      className={`content-save-bar flex shrink-0 flex-col gap-2 border-t border-(--border-default) px-3 py-2 text-[12px] transition-shadow duration-300 ${
        flash ? "shadow-[inset_0_0_0_2px_var(--accent)]" : ""
      }`}
      data-proposal-flash={flash ? "true" : undefined}
    >
      <div className="flex min-w-0 items-center justify-between gap-2">
        <p className="m-0 truncate text-(--text-secondary)" title={status?.changes.map((change) => `${change.status}: ${change.path}`).join("\n")}>
          {summary}
        </p>
        {managed.email ? (
          <div className="flex min-w-0 shrink items-center gap-1">
            <span className="truncate text-(--text-secondary)">
              {label("app.cowmark.signedInAs").replace("{email}", managed.email)}
            </span>
            <Button size="sm" variant="ghost" onClick={signOut}>{label("app.cowmark.signOut")}</Button>
          </div>
        ) : null}
      </div>

      {status && changeCount > 0 ? (
        <div className="flex flex-col gap-1">
          <label className="flex cursor-pointer items-center gap-2 text-[11px] text-(--text-secondary)">
            <input
              checked={allSelected}
              className="size-3.5 shrink-0 accent-(--accent)"
              ref={(input) => {
                if (input) input.indeterminate = selectedCount > 0 && !allSelected;
              }}
              type="checkbox"
              onChange={toggleAll}
            />
            {label("app.cowmark.selectAll")}
          </label>
          <ul className="m-0 max-h-40 list-none overflow-auto p-0 text-[11px] text-(--text-secondary)" aria-label={summary}>
            {status.changes.map((change) => (
              <li key={change.path}>
                <label className="flex min-w-0 cursor-pointer items-center gap-2 rounded-sm px-0.5 hover:bg-(--bg-hover)">
                  <input
                    aria-label={change.path}
                    checked={selected.has(change.path)}
                    className="size-3.5 shrink-0 accent-(--accent)"
                    type="checkbox"
                    onChange={() => setSelected((current) => toggleSelection(current, change.path, changedPaths))}
                  />
                  <span className="min-w-0 flex-1 truncate" title={change.path}>{change.path}</span>
                  {change.status === "added" || change.status === "deleted" ? (
                    <span className="shrink-0 text-[10px] uppercase tracking-wide">
                      {label(change.status === "added" ? "app.cowmark.statusAdded" : "app.cowmark.statusDeleted")}
                    </span>
                  ) : null}
                </label>
              </li>
            ))}
          </ul>
          <p className="m-0 text-[11px] leading-4 text-(--text-secondary)">{label("app.cowmark.draftHint")}</p>
        </div>
      ) : null}

      {managed.email ? (
        <>
          <Textarea
            aria-label={label("app.cowmark.messageLabel")}
            className="min-h-14 text-[12px]"
            placeholder={label("app.cowmark.messagePlaceholder")}
            rows={2}
            value={message}
            onChange={(event) => setMessage(event.target.value)}
          />
          <div className="flex items-center justify-end gap-2">
            <Button disabled={busy !== null} size="sm" onClick={() => { update().catch(() => {}); }}>
              {busy === "update" ? label("app.cowmark.updating") : label("app.cowmark.getLatest")}
            </Button>
            <Button disabled={busy !== null || !canSave} size="sm" variant="primary" onClick={() => { save().catch(() => {}); }}>
              {busy === "save"
                ? label("app.cowmark.saving")
                : selectedCount > 0
                  ? label("app.cowmark.saveSelected").replace("{count}", String(selectedCount))
                  : label("app.cowmark.save")}
            </Button>
          </div>
        </>
      ) : (
        <div className="flex items-center justify-between gap-2">
          <p className="m-0 text-(--text-secondary)">
            {sessionLost ? label("app.cowmark.signInAgain") : label("app.cowmark.signInToSave")}
          </p>
          <Button disabled={busy !== null} size="sm" variant="primary" onClick={() => { signIn().catch(() => {}); }}>
            {label("app.cowmark.signIn")}
          </Button>
        </div>
      )}

      {notice && !(notice.tone === "error" && notice.text === label("app.cowmark.signInAgain") && !managed.email) ? (
        <p
          className={`m-0 leading-4 ${notice.tone === "error" ? "text-(--danger,#c2410c)" : "text-(--text-secondary)"}`}
          role={notice.tone === "error" ? "alert" : "status"}
        >
          {notice.text}
        </p>
      ) : null}
    </footer>
  );
}
