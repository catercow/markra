import { t, type AppLanguage } from "@markra/shared";
import { Button, Modal, TextInput } from "@markra/ui";
import { useEffect, useId, useState, type FormEvent, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { slugify } from "../lib/cowmark-pages";

/** Mounts a dialog on its own root and resolves with what it closes with. */
function showDialog<T>(render: (close: (value: T) => void) => ReactNode, cancelled: T): Promise<T> {
  return new Promise((resolve) => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    let done = false;
    const close = (value: T) => {
      if (done) return;
      done = true;
      root.unmount();
      container.remove();
      resolve(value);
    };
    root.render(<DialogKeys onEscape={() => close(cancelled)}>{render(close)}</DialogKeys>);
  });
}

function DialogKeys({ children, onEscape }: { children: ReactNode; onEscape: () => unknown }) {
  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      onEscape();
    };
    window.addEventListener("keydown", handle, true);
    return () => window.removeEventListener("keydown", handle, true);
  }, [onEscape]);
  return <>{children}</>;
}

/**
 * Asks for an image's alt text before it is inserted. Resolves "" when skipped (the image is
 * then inserted with an empty alt) and null when cancelled.
 */
export function promptImageAltText(language: AppLanguage, image: Blob | null): Promise<string | null> {
  const preview = image ? URL.createObjectURL(image) : null;
  return showDialog<string | null>((close) => (
    <AltTextDialog language={language} preview={preview} onClose={close} />
  ), "").finally(() => {
    if (preview) URL.revokeObjectURL(preview);
  });
}

function AltTextDialog({
  language,
  onClose,
  preview,
}: {
  language: AppLanguage;
  onClose: (alt: string | null) => unknown;
  preview: string | null;
}) {
  const label = (key: Parameters<typeof t>[1]) => t(language, key);
  const [alt, setAlt] = useState("");
  const id = useId();
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onClose(alt.trim());
  };

  return (
    <Modal
      className="max-w-md"
      footer={
        <>
          <Button size="sm" onClick={() => onClose("")}>{label("app.cowmark.imageAltSkip")}</Button>
          <Button form={id} size="sm" type="submit" variant="primary">{label("app.cowmark.imageAltInsert")}</Button>
        </>
      }
      title={label("app.cowmark.imageAltTitle")}
    >
      <form className="grid gap-2" id={id} onSubmit={submit}>
        {preview ? (
          <img alt="" className="max-h-48 w-full rounded-md bg-(--bg-secondary) object-contain" src={preview} />
        ) : null}
        <p className="m-0 text-[12px] leading-4.5 text-(--text-secondary)">{label("app.cowmark.imageAltDescription")}</p>
        <TextInput
          aria-label={label("app.cowmark.imageAltTitle")}
          placeholder={label("app.cowmark.imageAltPlaceholder")}
          autoFocus
          value={alt}
          onChange={(event) => setAlt(event.currentTarget.value)}
        />
      </form>
    </Modal>
  );
}

/** Asks for a new post's title; resolves null when cancelled. */
export function promptNewPostTitle(language: AppLanguage, folder: string): Promise<string | null> {
  return showDialog<string | null>((close) => (
    <NewPostDialog folder={folder} language={language} onClose={close} />
  ), null);
}

function NewPostDialog({
  folder,
  language,
  onClose,
}: {
  folder: string;
  language: AppLanguage;
  onClose: (title: string | null) => unknown;
}) {
  const label = (key: Parameters<typeof t>[1]) => t(language, key);
  const [title, setTitle] = useState("");
  const id = useId();
  const slug = slugify(title);

  return (
    <Modal
      className="max-w-md"
      footer={
        <>
          <Button size="sm" onClick={() => onClose(null)}>{label("app.cowmark.newPostCancel")}</Button>
          <Button disabled={!slug} form={id} size="sm" type="submit" variant="primary">
            {label("app.cowmark.newPostCreate")}
          </Button>
        </>
      }
      title={label("app.cowmark.newPost")}
    >
      <form
        className="grid gap-2"
        id={id}
        onSubmit={(event) => {
          event.preventDefault();
          if (slug) onClose(title.trim());
        }}
      >
        <TextInput
          aria-label={label("app.cowmark.newPostTitle")}
          placeholder={label("app.cowmark.newPostTitle")}
          autoFocus
          value={title}
          onChange={(event) => setTitle(event.currentTarget.value)}
        />
        <p className="m-0 truncate font-mono text-[11px] text-(--text-secondary)">
          {slug ? `${folder}/${slug}.md` : " "}
        </p>
      </form>
    </Modal>
  );
}
