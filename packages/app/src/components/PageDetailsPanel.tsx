import {
  formatFrontmatterValue,
  parseMdcValue,
  readFrontmatterFields,
  writeFrontmatterField,
} from "@markra/editor";
import { useMarkraEditorView, useMarkraFrontmatterDetails } from "@markra/editor-react";
import { t, type AppLanguage, type I18nKey } from "@markra/shared";
import { Button, IconButton, Select, Switch, TextInput } from "@markra/ui";
import { ImagePlus, TriangleAlert, X } from "lucide-react";
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

type Label = (key: I18nKey) => string;
type Change = (value: unknown) => unknown;

export type PageDetailsPanelProps = {
  documentPath: string | null | undefined;
  language: AppLanguage;
};

/** "feature_image" → "Feature image". */
function humanName(name: string) {
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
 * Opened from the front matter card. Each change rewrites only its own key.
 */
export function PageDetailsPanel({ documentPath, language }: PageDetailsPanelProps) {
  const pages = useCowMarkPagesContext();
  const details = useMarkraFrontmatterDetails();
  const collection = pages ? collectionForDocument(pages.collections, pages.rootPath, documentPath) : null;
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
            <PageField
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

type FieldProps = {
  documentPath: string;
  field: PageField;
  label: Label;
  onChange: Change;
  pages: CowMarkPages;
  /** Values other pages in the collection use for this field. */
  suggestions: string[];
  value: unknown;
};

function PageField(props: FieldProps) {
  const { field, label } = props;
  const id = useId();
  const kind = pageFieldKind(field);
  const title = humanName(field.name);
  const labelled = kind !== "boolean";

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
      {renderControl(kind, { ...props, id, title })}
    </div>
  );
}

type ControlProps = FieldProps & { id: string; title: string };

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
    default:
      return <TextControl {...props} multiline={kind === "textarea"} />;
  }
}

/** An emptied optional field is removed from the file; a required one keeps "". */
function emptied(field: PageField, text: string) {
  return text === "" && !field.required ? undefined : text;
}

function TextControl({ field, id, label, multiline, onChange, suggestions, value }: ControlProps & { multiline: boolean }) {
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
          aria-describedby={counterId}
          className={textareaClassName}
          id={id}
          rows={3}
          value={draft}
          onBlur={onBlur}
          onChange={(event) => commit(event.currentTarget.value)}
        />
      ) : (
        <TextInput
          aria-describedby={counterId}
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

function BooleanControl({ field, onChange, title, value }: ControlProps) {
  const checked = typeof value === "boolean" ? value : field.default === true;
  return <Switch checked={checked} label={title} onCheckedChange={(next) => onChange(next)} />;
}

function NumberControl({ id, onChange, value }: ControlProps) {
  const { draft, edit, onBlur } = useDraft(value === undefined || value === null ? "" : String(value));
  return (
    <TextInput
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

function DateControl({ field, id, onChange, value }: ControlProps) {
  return (
    <TextInput
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

function ImageControl({ documentPath, field, id, label, onChange, pages, value }: ControlProps) {
  const src = typeof value === "string" ? value : "";
  const { draft, edit, onBlur } = useDraft(src);
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [src]);

  return (
    <div className="grid gap-1.5">
      {src && !broken ? (
        <img
          alt=""
          className="max-h-40 w-full rounded-md border border-(--border-default) bg-(--bg-secondary) object-contain"
          src={pages.resolveImage(documentPath, src)}
          onError={() => setBroken(true)}
        />
      ) : null}
      <div className="flex items-center gap-1">
        <TextInput
          className="min-w-0 flex-1"
          id={id}
          spellCheck={false}
          value={draft}
          onBlur={onBlur}
          onChange={(event) => {
            edit(event.currentTarget.value);
            onChange(emptied(field, event.currentTarget.value));
          }}
        />
        {src ? (
          <IconButton
            label={label("app.cowmark.pageDetailsRemoveImage")}
            size="icon-sm"
            onClick={() => onChange(field.required ? "" : undefined)}
          >
            <X aria-hidden="true" size={13} />
          </IconButton>
        ) : null}
      </div>
      <Button
        className="justify-self-start"
        size="sm"
        onClick={async () => {
          const picked = await pages.pickImage(documentPath);
          if (picked) onChange(picked);
        }}
      >
        <ImagePlus aria-hidden="true" size={13} />
        {label("app.cowmark.pageDetailsChooseImage")}
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
