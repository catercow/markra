import {
  formatFrontmatterValue,
  parseMdcValue,
  readFrontmatterFields,
  writeFrontmatterField,
} from "@markra/editor";
import { openFrontmatterDetails } from "@markra/editor/codemirror";
import { useMarkraEditorView, useMarkraFrontmatterDetails } from "@markra/editor-react";
import { t, type AppLanguage, type I18nKey } from "@markra/shared";
import { Button, IconButton, Switch, TextInput } from "@markra/ui";
import { Plus, TriangleAlert, X } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useCowMarkPagesContext, type CowMarkPages } from "../hooks/useCowMarkPages";
import {
  collectionForDocument,
  datePart,
  pageFieldCharLimit,
  pageFieldKind,
  withDatePart,
  type FieldValues,
  type PageCollection,
  type PageField,
} from "../lib/cowmark-pages";
import { ImageField } from "./ImageField";

type Label = (key: I18nKey) => string;
type Change = (value: unknown) => unknown;

export type PageDetailsPanelProps = {
  documentPath: string | null | undefined;
  language: AppLanguage;
};

/** "feature_image" → "Feature image". */
export function humanName(name: string) {
  const text = name.split(/[_\-\s]+/u).filter(Boolean).join(" ").toLowerCase();
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** True when the page details form applies to this document: CowMark, a known collection. */
export function usePageDetailsAvailable(documentPath: string | null | undefined) {
  const pages = useCowMarkPagesContext();
  return Boolean(pages && collectionForDocument(pages.collections, pages.rootPath, documentPath));
}

/**
 * CowMark's form over a page's YAML front matter, generated from `schema/collections.json`.
 * Opened from the front matter card, or by New page (`requestPageDetails`). Each change
 * rewrites only its own key.
 */
export function PageDetailsPanel({ documentPath, language }: PageDetailsPanelProps) {
  const pages = useCowMarkPagesContext();
  const view = useMarkraEditorView();
  const details = useMarkraFrontmatterDetails();
  const collection = pages ? collectionForDocument(pages.collections, pages.rootPath, documentPath) : null;

  useEffect(() => {
    if (!pages || !view || !documentPath || !collection || details.yaml === null || details.open) return;
    if (pages.takePageDetailsRequest(documentPath, view.state.doc.toString())) {
      view.dispatch({ effects: openFrontmatterDetails.of(true) });
    }
  }, [collection, details.open, details.yaml, documentPath, pages, view]);

  if (!pages || !collection || !details.open || details.yaml === null || !documentPath) return null;

  return (
    <PageDetailsBody
      collection={collection}
      documentPath={documentPath}
      key={documentPath}
      language={language}
      pages={pages}
    />
  );
}

function PageDetailsBody({
  collection,
  documentPath,
  language,
  pages,
}: {
  collection: PageCollection;
  documentPath: string;
  language: AppLanguage;
  pages: CowMarkPages;
}) {
  const fields = collection.fields;
  const view = useMarkraEditorView();
  const details = useMarkraFrontmatterDetails();
  const label: Label = (key) => t(language, key);
  const panelRef = useRef<HTMLElement | null>(null);
  const yaml = details.yaml ?? "";
  const read = useMemo(() => readFrontmatterFields(yaml), [yaml]);
  const [fieldValues, setFieldValues] = useState<FieldValues>({});
  const [writeError, setWriteError] = useState<string | null>(null);
  const latest = useRef({ apply: details.apply, yaml });
  latest.current = { apply: details.apply, yaml };

  useEffect(() => {
    let active = true;
    pages.loadFieldValues(collection).then((loaded) => {
      if (active) setFieldValues(loaded);
    }).catch(() => {});
    return () => {
      active = false;
    };
  }, [collection, pages]);

  useEffect(() => {
    panelRef.current?.querySelector<HTMLElement>("[data-page-details-body] input:not(:disabled)")?.focus();
  }, []);

  const write = (key: string, value: unknown) => {
    const next = writeFrontmatterField(latest.current.yaml, key, value);
    setWriteError(next.error ?? null);
    if (!next.error) latest.current.apply(next.text);
  };

  const close = () => {
    details.close();
    view?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    event.stopPropagation();
    close();
  };

  const known = new Set(fields.map((field) => field.name));
  const formFields = fields.filter((field) => pageFieldKind(field) !== "yaml");
  const advanced = [
    ...fields.filter((field) => pageFieldKind(field) === "yaml").map((field) => field.name),
    ...read.keys.filter((key) => !known.has(key)),
  ];
  const disabled = details.readOnly || Boolean(read.error);

  return (
    <aside
      aria-label={label("app.cowmark.pageDetails")}
      className="markra-page-details-panel fixed top-12 right-[calc(var(--markra-ai-agent-inset,0px)_+_12px)] bottom-3 z-40 flex w-[min(400px,calc(100vw-24px))] flex-col overflow-hidden rounded-lg border border-(--border-default) bg-(--bg-primary) text-(--text-primary) shadow-(--ai-command-popover-shadow) outline-none"
      ref={panelRef}
      role="complementary"
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      <header className="flex shrink-0 items-center gap-2 border-b border-(--border-default) px-4 py-3">
        <h2 className="m-0 min-w-0 flex-1 text-[14px] leading-5 font-[650] text-(--text-heading)">
          {label("app.cowmark.pageDetails")}
        </h2>
        <IconButton label={label("app.cowmark.pageDetailsClose")} size="icon-sm" onClick={close}>
          <X aria-hidden="true" size={14} />
        </IconButton>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3" data-page-details-body>
        {read.error || writeError ? (
          <Notice>
            <p className="m-0">{label("app.cowmark.pageDetailsInvalidYaml")}</p>
            <p className="m-0 mt-1 font-mono text-[11px]">{read.error ?? writeError}</p>
          </Notice>
        ) : null}

        <fieldset className="m-0 grid min-w-0 gap-4 border-0 p-0" disabled={disabled}>
          {formFields.map((field) => (
            <PageFieldInput
              documentPath={documentPath}
              field={field}
              key={field.name}
              label={label}
              pages={pages}
              suggestions={fieldValues[field.name] ?? []}
              value={read.values[field.name]}
              onChange={(value) => write(field.name, value)}
            />
          ))}
        </fieldset>

        {advanced.length > 0 ? (
          <details className="mt-5 border-t border-(--border-default) pt-3">
            <summary className="cursor-pointer text-[12px] leading-5 font-[650] text-(--text-heading)">
              {label("app.cowmark.pageDetailsAdvanced")}
            </summary>
            <p className="m-0 mt-1 mb-3 text-[11px] leading-4 text-(--text-secondary)">
              {label("app.cowmark.pageDetailsAdvancedDescription")}
            </p>
            <fieldset className="m-0 grid min-w-0 gap-4 border-0 p-0" disabled={disabled}>
              {advanced.map((key) => (
                <YamlField
                  key={key}
                  label={label}
                  name={key}
                  value={read.values[key]}
                  onChange={(value) => write(key, value)}
                />
              ))}
            </fieldset>
          </details>
        ) : null}
      </div>
    </aside>
  );
}

function Notice({ children }: { children: ReactNode }) {
  return (
    <div
      className="mb-3 flex items-start gap-2 rounded-md bg-(--bg-secondary) px-3 py-2 text-[12px] leading-4.5 text-(--text-heading)"
      role="alert"
    >
      <TriangleAlert aria-hidden="true" className="mt-0.5 shrink-0 text-(--danger)" size={13} />
      <div className="min-w-0">{children}</div>
    </div>
  );
}

/** Keeps typed text while the document's value catches up; adopts outside changes (undo). */
function useDraft(external: string) {
  const [draft, setDraft] = useState(external);
  const ownEdit = useRef(false);
  useEffect(() => {
    if (!ownEdit.current) setDraft(external);
  }, [external]);
  useEffect(() => {
    ownEdit.current = false;
  });
  return {
    draft,
    edit: (next: string) => {
      ownEdit.current = true;
      setDraft(next);
    },
    onBlur: () => setDraft(external),
  };
}

const textareaClassName =
  "min-h-16 w-full resize-y rounded-md border border-(--border-default) bg-(--bg-primary) px-3 py-2 text-[13px] leading-5 font-[520] text-(--text-heading) outline-none transition-[border-color,box-shadow] duration-150 ease-out placeholder:text-(--text-secondary) focus:border-(--accent) focus:ring-2 focus:ring-(--accent)/20";

export type PageFieldInputProps = {
  /** The page the field belongs to; null before it exists (New page), when images can't be stored. */
  documentPath: string | null;
  field: PageField;
  label: Label;
  onChange: Change;
  pages: CowMarkPages | null;
  /** Values other pages in the collection use for this field. */
  suggestions: string[];
  value: unknown;
};

/** One front matter field: its label, control and help text. Used by Page details and New page. */
export function PageFieldInput(props: PageFieldInputProps) {
  const { field, label } = props;
  const id = useId();
  const kind = pageFieldKind(field);
  const title = humanName(field.name);
  const labelled = kind !== "boolean" && kind !== "images";
  const descriptionId = field.description ? `${id}-description` : undefined;

  return (
    <div className="grid min-w-0 gap-1.5" data-page-field={field.name}>
      <div className="flex min-w-0 items-baseline gap-1.5">
        {labelled ? (
          <label className="text-[12px] leading-5 font-[650] text-(--text-heading)" htmlFor={id}>{title}</label>
        ) : (
          <span className="text-[12px] leading-5 font-[650] text-(--text-heading)" id={`${id}-label`}>{title}</span>
        )}
        {field.required ? (
          <span className="text-[12px] leading-5 text-(--danger)" title={label("app.cowmark.pageDetailsRequired")}>*</span>
        ) : null}
        <code className="ml-auto truncate font-mono text-[10.5px] text-(--text-secondary)">{field.name}</code>
      </div>
      {renderControl(kind, { ...props, descriptionId, id, title })}
      {field.description ? (
        <p className="m-0 text-[11px] leading-4 text-(--text-secondary)" id={descriptionId}>{field.description}</p>
      ) : null}
    </div>
  );
}

type ControlProps = PageFieldInputProps & { descriptionId: string | undefined; id: string; title: string };

function renderControl(kind: ReturnType<typeof pageFieldKind>, props: ControlProps) {
  switch (kind) {
    case "boolean":
      return <BooleanControl {...props} />;
    case "number":
      return <NumberControl {...props} />;
    case "date":
      return <DateControl {...props} />;
    case "image":
      return <ImageControl {...props} />;
    case "images":
      return <ImagesControl {...props} />;
    default:
      return <TextControl {...props} multiline={kind === "textarea"} />;
  }
}

/** An emptied optional field is removed from the file; a required one keeps "". */
function emptied(field: PageField, text: string) {
  return text === "" && !field.required ? undefined : text;
}

function describedBy(...ids: Array<string | undefined>) {
  return ids.filter(Boolean).join(" ") || undefined;
}

function TextControl({ descriptionId, field, id, label, multiline, onChange, suggestions, value }: ControlProps & { multiline: boolean }) {
  const { draft, edit, onBlur } = useDraft(typeof value === "string" || typeof value === "number" ? String(value) : "");
  const limit = pageFieldCharLimit(field);
  const counterId = limit ? `${id}-count` : undefined;
  // Free text (titles, descriptions) never repeats enough to suggest; a category does.
  const listId = !multiline && !limit && suggestions.length > 0 ? `${id}-values` : undefined;
  const commit = (next: string) => {
    edit(next);
    onChange(emptied(field, next));
  };

  return (
    <>
      {multiline ? (
        <textarea
          aria-describedby={describedBy(counterId, descriptionId)}
          className={textareaClassName}
          id={id}
          rows={3}
          value={draft}
          onBlur={onBlur}
          onChange={(event) => commit(event.currentTarget.value)}
        />
      ) : (
        <TextInput
          aria-describedby={describedBy(counterId, descriptionId)}
          id={id}
          list={listId}
          value={draft}
          onBlur={onBlur}
          onChange={(event) => commit(event.currentTarget.value)}
        />
      )}
      {listId ? (
        <datalist id={listId}>
          {suggestions.map((suggestion) => <option key={suggestion} value={suggestion} />)}
        </datalist>
      ) : null}
      {limit ? (
        <p
          className={`m-0 text-right text-[11px] leading-4 ${draft.length > limit ? "text-(--danger)" : "text-(--text-secondary)"}`}
          id={counterId}
        >
          {label("app.cowmark.pageDetailsCharCount")
            .replace("{count}", String(draft.length))
            .replace("{limit}", String(limit))}
          {draft.length > limit ? ` · ${label("app.cowmark.pageDetailsTooLong")}` : ""}
        </p>
      ) : null}
    </>
  );
}

function BooleanControl({ descriptionId, field, onChange, title, value }: ControlProps) {
  const checked = typeof value === "boolean" ? value : field.default === true;
  return (
    <Switch aria-describedby={descriptionId} checked={checked} label={title} onCheckedChange={(next) => onChange(next)} />
  );
}

function NumberControl({ descriptionId, id, onChange, value }: ControlProps) {
  const { draft, edit, onBlur } = useDraft(value === undefined || value === null ? "" : String(value));
  return (
    <TextInput
      aria-describedby={descriptionId}
      id={id}
      inputMode="decimal"
      type="number"
      value={draft}
      onBlur={onBlur}
      onChange={(event) => {
        const next = event.currentTarget.value;
        edit(next);
        if (next.trim() === "") onChange(undefined);
        else if (Number.isFinite(Number(next))) onChange(Number(next));
      }}
    />
  );
}

function DateControl({ descriptionId, field, id, onChange, value }: ControlProps) {
  return (
    <TextInput
      aria-describedby={descriptionId}
      id={id}
      type="date"
      value={datePart(value)}
      onChange={(event) => {
        const next = event.currentTarget.value;
        if (!next) {
          if (!field.required) onChange(undefined);
          return;
        }
        onChange(withDatePart(value, next));
      }}
    />
  );
}

/** The picker, drop and paste store images in the page's folder; without a page, only text. */
function imageFieldTools({ documentPath, label, pages }: ControlProps) {
  const tools = {
    chooseLabel: label("app.cowmark.pageDetailsChooseImage"),
    placeholder: label("app.cowmark.imagePlaceholder"),
    removeLabel: label("app.cowmark.pageDetailsRemoveImage"),
  };
  if (!pages || !documentPath) return tools;
  return {
    ...tools,
    pick: () => pages.pickImage(documentPath),
    resolveSrc: (src: string) => pages.resolveImage(documentPath, src),
    save: (file: File) => pages.importImage(documentPath, file),
  };
}

function ImageControl(props: ControlProps) {
  const { descriptionId, field, id, onChange, value } = props;
  return (
    <ImageField
      {...imageFieldTools(props)}
      aria-describedby={descriptionId}
      id={id}
      value={typeof value === "string" ? value : ""}
      onChange={(src) => onChange(emptied(field, src))}
      onClear={() => onChange(field.required ? "" : undefined)}
    />
  );
}

/** A `list of string` of images: one image control per entry, with add and remove. */
function ImagesControl(props: ControlProps) {
  const { descriptionId, id, label, onChange, title, value } = props;
  const items = Array.isArray(value) ? value.map((item) => (typeof item === "string" ? item : "")) : [];
  const tools = imageFieldTools(props);
  const write = (next: string[]) => onChange(next.length > 0 ? next : undefined);
  const [added, setAdded] = useState<number | null>(null);
  useEffect(() => {
    if (added === null) return;
    document.getElementById(`${id}-${added}`)?.focus();
    setAdded(null);
  }, [added, id]);

  return (
    <div aria-describedby={descriptionId} aria-labelledby={`${id}-label`} className="grid gap-2" role="group">
      {items.map((item, index) => (
        <div className="flex items-start gap-1" key={index}>
          <div className="min-w-0 flex-1">
            <ImageField
              {...tools}
              aria-label={`${title} ${index + 1}`}
              id={`${id}-${index}`}
              value={item}
              onChange={(src) => write(items.map((current, at) => (at === index ? src : current)))}
            />
          </div>
          <IconButton
            label={`${label("app.cowmark.pageDetailsRemoveImage")}: ${title} ${index + 1}`}
            size="icon-sm"
            onClick={() => write(items.filter((_, at) => at !== index))}
          >
            <X aria-hidden="true" size={13} />
          </IconButton>
        </div>
      ))}
      <Button
        className="justify-self-start"
        size="sm"
        onClick={() => {
          write([...items, ""]);
          setAdded(items.length);
        }}
      >
        <Plus aria-hidden="true" size={13} />
        {label("app.cowmark.imageAdd")}
      </Button>
    </div>
  );
}

function YamlField({ label, name, onChange, value }: { label: Label; name: string; onChange: Change; value: unknown }) {
  const id = useId();
  const { draft, edit, onBlur } = useDraft(formatFrontmatterValue(value));
  const [error, setError] = useState<string | null>(null);

  return (
    <div className="grid min-w-0 gap-1.5">
      <label className="font-mono text-[11px] font-[650] text-(--text-secondary)" htmlFor={id}>{name}</label>
      <textarea
        aria-invalid={error ? true : undefined}
        className={`${textareaClassName} font-mono text-[12px]`}
        id={id}
        rows={Math.min(12, Math.max(2, draft.split("\n").length))}
        spellCheck={false}
        value={draft}
        onBlur={() => {
          if (!error) onBlur();
        }}
        onChange={(event) => {
          const next = event.currentTarget.value;
          edit(next);
          const parsed = parseMdcValue(next);
          setError(parsed.error ?? null);
          if (parsed.error === undefined) onChange(parsed.value);
        }}
      />
      {error ? (
        <p className="m-0 text-[11px] leading-4 text-(--danger)" role="alert">
          {label("app.cowmark.pageDetailsInvalidValue")} {error}
        </p>
      ) : null}
    </div>
  );
}
