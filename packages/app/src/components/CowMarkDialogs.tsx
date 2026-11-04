import { t, type AppLanguage, type I18nKey } from "@markra/shared";
import { Button, Modal, TextInput } from "@markra/ui";
import { useEffect, useId, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import type { CowMarkPages } from "../hooks/useCowMarkPages";
import {
  creatableCollections,
  newPageFields,
  newPageInitialValues,
  pageUrl,
  slugDraft,
  slugify,
  type FieldValues,
  type PageCollection,
  type PageField,
} from "../lib/cowmark-pages";
import { humanName, PageFieldInput } from "./PageDetailsPanel";

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

export type NewPage = {
  collection: PageCollection;
  /** The file name without .md. */
  slug: string;
  title: string;
  /** The required fields filled in the dialog, by name. */
  values: Record<string, unknown>;
};

export type NewPageOptions = {
  /** The collection to start on, e.g. the open page's. */
  initialCollection?: string | null;
  pages: Pick<CowMarkPages, "collections" | "listPages" | "loadFieldValues">;
};

/** Asks for a new page's type, title, file name and required fields; resolves null when cancelled. */
export function promptNewPage(language: AppLanguage, options: NewPageOptions): Promise<NewPage | null> {
  return showDialog<NewPage | null>((close) => (
    <NewPageDialog {...options} language={language} onClose={close} />
  ), null);
}

const titleField: PageField = { name: "title", required: true, type: "string" };

export function NewPageDialog({
  initialCollection,
  language,
  onClose,
  pages,
}: NewPageOptions & {
  language: AppLanguage;
  onClose: (page: NewPage | null) => unknown;
}) {
  const label = (key: I18nKey) => t(language, key);
  const collections = useMemo(() => creatableCollections(pages.collections), [pages.collections]);
  const [collectionName, setCollectionName] = useState(
    () => collections.find((collection) => collection.name === initialCollection)?.name ?? collections[0]?.name ?? ""
  );
  const collection = collections.find((candidate) => candidate.name === collectionName) ?? null;
  const [title, setTitle] = useState("");
  // null until the URL name is edited: it follows the title until then.
  const [slugInput, setSlugInput] = useState<string | null>(null);
  const [values, setValues] = useState<Record<string, unknown>>(() => (collection ? newPageInitialValues(collection) : {}));
  const [existing, setExisting] = useState<ReadonlySet<string> | null>(null);
  const [fieldValues, setFieldValues] = useState<FieldValues>({});
  const formRef = useRef<HTMLFormElement | null>(null);
  const id = useId();

  useEffect(() => {
    setExisting(null);
    setFieldValues({});
    if (!collection) return;
    let active = true;
    pages.listPages(collection).then((names) => {
      if (active) setExisting(new Set(names));
    }).catch(() => {});
    pages.loadFieldValues(collection).then((loaded) => {
      if (active) setFieldValues(loaded);
    }).catch(() => {});
    return () => {
      active = false;
    };
  }, [collection, pages]);

  useEffect(() => {
    formRef.current?.querySelector<HTMLElement>("[data-page-field='title'] input")?.focus();
  }, []);

  if (!collection) return null;

  const { asked, later } = newPageFields(collection);
  const shownSlug = slugInput ?? slugify(title);
  const slug = slugify(shownSlug);
  const exists = Boolean(slug && existing?.has(slug));
  const blocked = !slug || exists || !title.trim();
  const slugError = exists
    ? label("app.cowmark.newPageExists")
    : !slug && (slugInput !== null || title.trim())
      ? label("app.cowmark.newPageSlugEmpty")
      : null;
  const hint = collection.slugExample
    ? label("app.cowmark.newPageSlugHint").replace("{example}", collection.slugExample)
    : label("app.cowmark.newPageSlugHintPlain");
  const slugId = `${id}-slug`;

  const chooseCollection = (name: string) => {
    const next = collections.find((candidate) => candidate.name === name);
    if (!next) return;
    setCollectionName(name);
    setValues(newPageInitialValues(next));
  };

  return (
    <Modal
      className="max-w-lg"
      footer={
        <>
          <Button size="sm" onClick={() => onClose(null)}>{label("app.cowmark.newPageCancel")}</Button>
          <Button disabled={blocked} form={id} size="sm" type="submit" variant="primary">
            {label("app.cowmark.newPageCreate")}
          </Button>
        </>
      }
      title={label("app.cowmark.newPage")}
    >
      <form
        className="grid gap-4"
        id={id}
        ref={formRef}
        onSubmit={(event) => {
          event.preventDefault();
          if (!blocked) onClose({ collection, slug, title: title.trim(), values });
        }}
      >
        <fieldset className="m-0 grid min-w-0 gap-1.5 border-0 p-0">
          <legend className="mb-1.5 p-0 text-[12px] leading-5 font-[650] text-(--text-heading)">
            {label("app.cowmark.newPageType")}
          </legend>
          <div className="grid max-h-56 gap-1 overflow-y-auto rounded-md border border-(--border-default) p-1">
            {collections.map((candidate) => (
              <label
                className={`flex cursor-pointer items-start gap-2 rounded px-2 py-1.5 ${candidate.name === collectionName ? "bg-(--bg-secondary)" : "hover:bg-(--bg-hover)"}`}
                key={candidate.name}
              >
                <input
                  checked={candidate.name === collectionName}
                  className="mt-1 accent-(--accent)"
                  name={`${id}-type`}
                  type="radio"
                  value={candidate.name}
                  onChange={() => chooseCollection(candidate.name)}
                />
                <span className="grid min-w-0">
                  <span className="text-[13px] leading-5 font-[600] text-(--text-heading)">{candidate.label}</span>
                  {candidate.description ? (
                    <span className="text-[11px] leading-4 text-(--text-secondary)">{candidate.description}</span>
                  ) : null}
                </span>
              </label>
            ))}
          </div>
        </fieldset>

        <PageFieldInput
          documentPath={null}
          field={collection.fields.find((field) => field.name === "title") ?? titleField}
          label={label}
          pages={null}
          suggestions={[]}
          value={title}
          onChange={(next) => setTitle(typeof next === "string" ? next : "")}
        />

        <div className="grid min-w-0 gap-1.5">
          <label className="text-[12px] leading-5 font-[650] text-(--text-heading)" htmlFor={slugId}>
            {label("app.cowmark.newPageSlug")}
          </label>
          <TextInput
            aria-describedby={`${slugId}-hint ${slugId}-url`}
            aria-invalid={slugError ? true : undefined}
            className="font-mono"
            id={slugId}
            placeholder={collection.slugExample}
            spellCheck={false}
            value={shownSlug}
            onBlur={() => {
              if (slugInput !== null) setSlugInput(slugify(slugInput));
            }}
            onChange={(event) => setSlugInput(slugDraft(event.currentTarget.value))}
          />
          <p className="m-0 text-[11px] leading-4 text-(--text-secondary)" id={`${slugId}-hint`}>{hint}</p>
          <p className="m-0 truncate font-mono text-[11px] leading-4 text-(--text-secondary)" id={`${slugId}-url`}>
            {label("app.cowmark.newPageUrl")} {pageUrl(collection, slug || "…")}
          </p>
          {slugError ? (
            <p className="m-0 text-[11px] leading-4 text-(--danger)" role="alert">{slugError}</p>
          ) : null}
        </div>

        {asked.map((field) => (
          <PageFieldInput
            documentPath={null}
            field={field}
            key={`${collection.name}/${field.name}`}
            label={label}
            pages={null}
            suggestions={fieldValues[field.name] ?? []}
            value={values[field.name]}
            onChange={(next) => setValues((current) => ({ ...current, [field.name]: next }))}
          />
        ))}

        {later.length > 0 ? (
          <p className="m-0 text-[11px] leading-4 text-(--text-secondary)">
            {label("app.cowmark.newPageLater").replace("{fields}", later.map((field) => humanName(field.name)).join(", "))}
          </p>
        ) : null}
      </form>
    </Modal>
  );
}
