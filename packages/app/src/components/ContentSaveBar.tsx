import { useCallback, useEffect, useState } from "react";
import { t, type AppLanguage, type I18nKey } from "@markra/shared";
import { Button, Textarea } from "@markra/ui";
import { getAppRuntime } from "../runtime";
import { contentAccessError, type ContentGitStatus, type ManagedContent } from "../lib/content-access";

const statusPollMs = 4000;

type Notice = { tone: "error" | "info"; text: string } | null;

type ContentSaveBarProps = {
  language: AppLanguage;
  managed: ManagedContent;
  /** Writes open documents to disk first, so git sees unsaved edits. */
  beforeSave?: () => Promise<unknown>;
};

/** The preview panel's footer in CowMark: who is signed in, what changed, Save to studio. */
export function ContentSaveBar({ beforeSave, language, managed }: ContentSaveBarProps) {
  const label = useCallback((key: I18nKey) => t(language, key), [language]);
  const access = getAppRuntime().files.contentAccess;
  const [status, setStatus] = useState<ContentGitStatus | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<"save" | "update" | "sign-in" | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [sessionLost, setSessionLost] = useState(false);

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

  const handleFailure = (error: unknown, failedKey: I18nKey) => {
    const { code, message: text } = contentAccessError(error);
    if (code === "signed_out") {
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
      await access.save(managed.root, message);
      setMessage("");
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
  const pending = changeCount > 0 || (status?.ahead ?? 0) > 0;
  const summary = changeCount === 0
    ? (status?.ahead ? label("app.cowmark.waitingToSend") : label("app.cowmark.noChanges"))
    : changeCount === 1
      ? label("app.cowmark.changeOne")
      : label("app.cowmark.changeCount").replace("{count}", String(changeCount));

  return (
    <footer className="content-save-bar flex shrink-0 flex-col gap-2 border-t border-(--border-default) px-3 py-2 text-[12px]">
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
        <ul className="m-0 max-h-24 list-none overflow-auto p-0 text-[11px] text-(--text-secondary)" aria-label={summary}>
          {status.changes.map((change) => (
            <li className="truncate" key={change.path}>{change.path}</li>
          ))}
        </ul>
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
            <Button disabled={busy !== null || !pending} size="sm" variant="primary" onClick={() => { save().catch(() => {}); }}>
              {busy === "save" ? label("app.cowmark.saving") : label("app.cowmark.save")}
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
