import {
  fieldKind,
  formatMdcValue,
  kebabCase,
  parseMdcValue,
  readMdcProps,
  schemaDefault,
  selectOptions,
  writeMdcProp,
  type MdcPropSchema as EditorMdcPropSchema,
} from "@markra/editor";
import { useMarkraEditorView, useMarkraMdcPropsEditor } from "@markra/editor-react";
import { t, type AppLanguage, type MdcComponentSchema, type MdcSchema } from "@markra/shared";
import { Badge, Button, IconButton, Select, Switch, TextInput } from "@markra/ui";
import { ArrowDown, ArrowUp, ChevronRight, Plus, Trash2, TriangleAlert, X } from "lucide-react";
import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";

export interface MdcPropsPanelProps {
  language: AppLanguage;
  schema: MdcSchema | null | undefined;
}

type Label = (key: string) => string;
type ChangeValue = (value: unknown) => unknown;

/** "structured-data" → "Structured data", matching the editor's component cards. */
function humanName(name: string) {
  const text = name.replace(/([a-z])([A-Z])/gu, "$1 $2").split(/[-_\s]+/u).filter(Boolean).join(" ").toLowerCase();
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * The props form for the component whose card's Edit button was pressed. Docked to the right of
 * the window rather than anchored to the card: the form can be taller than the viewport (FAQ and
 * pricing lists), and it must not move while the card it edits reflows under every keystroke.
 */
export function MdcPropsPanel({ language, schema }: MdcPropsPanelProps) {
  const editor = useMarkraMdcPropsEditor();
  if (!editor.block || !editor.text) return null;

  return <MdcPropsPanelBody key={editor.block.open.from} language={language} schema={schema} />;
}

function MdcPropsPanelBody({ language, schema }: MdcPropsPanelProps) {
  const view = useMarkraEditorView();
  const editor = useMarkraMdcPropsEditor();
  const label: Label = (key) => t(language, key);
  const panelRef = useRef<HTMLElement | null>(null);
  const block = editor.block!;
  const text = editor.text!;
  const component = schema?.components.find((candidate) => candidate.name === block.name);
  const read = useMemo(() => readMdcProps({ ...text, schema: component }), [component, text]);
  const yamlInvalid = read.errors.some((error) => error.startsWith("Invalid YAML"));
  const disabled = editor.readOnly || yamlInvalid;

  // Writes read the newest text, not the text of the render that created the handler.
  const latest = useRef({ apply: editor.apply, component, text });
  latest.current = { apply: editor.apply, component, text };

  const writeProp = (name: string, value: unknown) => {
    const { apply, component: current, text: currentText } = latest.current;
    const next = writeMdcProp({ ...currentText, schema: current }, name, value);
    if (!next.error) apply({ attributes: next.attributes, yaml: next.yaml });
  };

  const close = () => {
    editor.close();
    view?.focus();
  };

  useEffect(() => {
    const panel = panelRef.current;
    const first = panel?.querySelector<HTMLElement>(
      ["input", "textarea", "select", "button"]
        .map((tag) => `[data-mdc-props-body] ${tag}:not(:disabled)`)
        .join(", "),
    );
    (first ?? panel)?.focus();
  }, []);

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    event.stopPropagation();
    close();
  };

  const otherProps = component ? read.unknown : Object.keys(read.values);
  const props = component?.props ?? [];

  return (
    <aside
      aria-label={label("editor.mdcProps")}
      className="markra-mdc-props-panel fixed top-12 right-[calc(var(--markra-ai-agent-inset,0px)_+_12px)] bottom-3 z-40 flex w-[min(380px,calc(100vw-24px))] flex-col overflow-hidden rounded-lg border border-(--border-default) bg-(--bg-primary) text-(--text-primary) shadow-(--ai-command-popover-shadow) outline-none"
      ref={panelRef}
      role="complementary"
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      <header className="flex shrink-0 items-start gap-2 border-b border-(--border-default) px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="m-0 text-[14px] leading-5 font-[650] text-(--text-heading)">{humanName(block.name)}</h2>
            {component?.group ? (
              <Badge className="bg-(--bg-secondary) font-[560] text-(--text-secondary)" size="sm">
                {component.group}
              </Badge>
            ) : null}
          </div>
          {component?.purpose ? (
            <p className="m-0 mt-1 text-[12px] leading-4.5 text-(--text-secondary)">{component.purpose}</p>
          ) : null}
        </div>
        <IconButton label={label("editor.mdcPropsClose")} size="icon-sm" onClick={close}>
          <X aria-hidden="true" size={14} />
        </IconButton>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3" data-mdc-props-body>
        {yamlInvalid ? (
          <Notice>
            <p className="m-0">{label("editor.mdcPropsInvalidYaml")}</p>
            {read.errors.map((error) => (
              <p className="m-0 mt-1 font-mono text-[11px]" key={error}>{error}</p>
            ))}
          </Notice>
        ) : null}
        {!component ? <Notice>{label("editor.mdcPropsUnknownComponent")}</Notice> : null}
        {component && props.length === 0 ? (
          <p className="m-0 text-[12px] text-(--text-secondary)">{label("editor.mdcPropsNoProps")}</p>
        ) : null}

        <fieldset className="m-0 grid min-w-0 gap-4 border-0 p-0" disabled={disabled}>
          {props.map((prop) => {
            const name = kebabCase(prop.name);
            return (
              <MdcField
                key={name}
                label={label}
                prop={prop}
                topLevel
                value={read.source[name] === "default" ? undefined : read.values[name]}
                onChange={(value) => writeProp(name, value)}
              />
            );
          })}
        </fieldset>

        {otherProps.length > 0 ? (
          <section className="mt-5 border-t border-(--border-default) pt-3">
            <h3 className="m-0 text-[12px] leading-5 font-[650] text-(--text-heading)">{label("editor.mdcPropsOther")}</h3>
            <p className="m-0 mb-2 text-[11px] leading-4 text-(--text-secondary)">{label("editor.mdcPropsOtherDescription")}</p>
            <dl className="m-0 grid gap-2">
              {otherProps.map((name) => (
                <div className="min-w-0" key={name}>
                  <dt className="font-mono text-[11px] font-[650] text-(--text-secondary)">{name}</dt>
                  <dd className="m-0">
                    <pre className="m-0 overflow-x-auto rounded-md bg-(--bg-secondary) px-2 py-1 font-mono text-[11px] leading-4 whitespace-pre-wrap text-(--text-primary)">
                      {formatMdcValue(read.values[name])}
                    </pre>
                  </dd>
                </div>
              ))}
            </dl>
          </section>
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

/**
 * Local text for an input that commits on every keystroke. The document is the source of truth,
 * but its value can differ from what was typed (a typed default is removed, "1." reads back as
 * 1), so a field keeps its own text after its own edits and adopts every other change (undo,
 * typing in the editor). Blurring settles it on the document's value.
 */
function useDraft(external: string) {
  const [draft, setDraft] = useState(external);
  const ownEdit = useRef(false);

  useEffect(() => {
    if (!ownEdit.current) setDraft(external);
  }, [external]);
  // Runs after the effect above in the same commit, so only the render an edit caused skips it.
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

interface MdcFieldProps {
  label: Label;
  onChange: ChangeValue;
  prop: EditorMdcPropSchema;
  /** Top-level props fall back to the schema default when emptied; item fields are kept. */
  topLevel?: boolean;
  value: unknown;
}

function MdcField(props: MdcFieldProps) {
  const { label, prop } = props;
  const id = useId();
  const descriptionId = prop.description ? `${id}-description` : undefined;
  const kind = effectiveKind(prop, props.value);
  const title = humanName(prop.name);
  const control = renderControl(kind, { ...props, descriptionId, id, title });
  const labelled = kind !== "boolean" && kind !== "string-list" && kind !== "object-list";

  return (
    <div className="grid min-w-0 gap-1.5" data-mdc-prop={prop.name}>
      <div className="flex min-w-0 items-baseline gap-1.5">
        {labelled ? (
          <label className="text-[12px] leading-5 font-[650] text-(--text-heading)" htmlFor={id}>{title}</label>
        ) : (
          <span className="text-[12px] leading-5 font-[650] text-(--text-heading)" id={`${id}-label`}>{title}</span>
        )}
        {prop.required ? (
          <span className="text-[12px] leading-5 text-(--danger)" title={label("editor.mdcPropsRequired")}>*</span>
        ) : null}
        <code className="ml-auto truncate font-mono text-[10.5px] text-(--text-secondary)">{prop.name}</code>
      </div>
      {control}
      {prop.description ? (
        <p className="m-0 text-[11px] leading-4 text-(--text-secondary)" id={descriptionId}>{prop.description}</p>
      ) : null}
    </div>
  );
}

/** A list or option value that the form cannot show falls back to the free-form editor. */
function effectiveKind(prop: EditorMdcPropSchema, value: unknown) {
  const kind = fieldKind(prop);
  if (value === undefined) return kind;
  if (kind === "string-list" && !(Array.isArray(value) && value.every((item) => typeof item === "string"))) return "json";
  if (kind === "object-list" && !(Array.isArray(value) && value.every(isRecord))) return "json";
  if ((kind === "text" || kind === "textarea") && typeof value !== "string" && typeof value !== "number") return "json";
  return kind;
}

interface ControlProps extends MdcFieldProps {
  descriptionId: string | undefined;
  id: string;
  title: string;
}

function renderControl(kind: ReturnType<typeof fieldKind>, props: ControlProps) {
  switch (kind) {
    case "text":
    case "textarea":
      return <TextControl {...props} multiline={kind === "textarea"} />;
    case "number":
      return <NumberControl {...props} />;
    case "boolean":
      return <BooleanControl {...props} />;
    case "select":
      return <SelectControl {...props} />;
    case "string-list":
      return <StringListControl {...props} />;
    case "object-list":
      return <ObjectListControl {...props} />;
    default:
      return <JsonControl {...props} />;
  }
}

function placeholderFor(prop: EditorMdcPropSchema, label: Label) {
  // Link props take a pasted address; CowMark stores it in canonical form on save.
  if (prop.format === "search-link") return label("editor.mdcPropsSearchLinkHint");
  if (prop.format === "review-link") return label("editor.mdcPropsReviewLinkHint");
  const fallback = schemaDefault(prop);
  return fallback === undefined || fallback === "" || typeof fallback === "object" ? undefined : String(fallback);
}

// The @markra/ui Textarea look, on a native element so the auto-grow ref reaches it.
const textareaClassName =
  "min-h-16 w-full resize-none overflow-hidden rounded-md border border-(--border-default) bg-(--bg-primary) px-3 py-2 leading-5 font-[520] text-(--text-heading) outline-none transition-[border-color,box-shadow] duration-150 ease-out placeholder:text-(--text-secondary) focus:border-(--accent) focus:ring-2 focus:ring-(--accent)/20 focus-visible:outline-none";

function useAutoGrow(value: string) {
  const ref = useRef<HTMLTextAreaElement | null>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight + 2}px`;
  }, [value]);
  return ref;
}

function TextControl({ descriptionId, id, label, multiline, onChange, prop, topLevel, value }: ControlProps & { multiline: boolean }) {
  const { draft, edit, onBlur } = useDraft(value === undefined ? "" : String(value));
  const textareaRef = useAutoGrow(multiline ? draft : "");
  const commit = (next: string) => {
    edit(next);
    // An emptied optional field reverts to its default; a required item field keeps "".
    onChange(next === "" && (topLevel || !prop.required) ? undefined : next);
  };
  const shared = {
    "aria-describedby": descriptionId,
    id,
    placeholder: placeholderFor(prop, label),
    value: draft,
    onBlur,
  };

  return multiline ? (
    <textarea
      {...shared}
      className={`${textareaClassName} text-[13px]`}
      ref={textareaRef}
      rows={2}
      onChange={(event) => commit(event.currentTarget.value)}
    />
  ) : (
    <TextInput {...shared} onChange={(event) => commit(event.currentTarget.value)} />
  );
}

function NumberControl({ descriptionId, id, label, onChange, prop, value }: ControlProps) {
  const { draft, edit, onBlur } = useDraft(value === undefined ? "" : String(value));
  return (
    <TextInput
      aria-describedby={descriptionId}
      id={id}
      inputMode="decimal"
      placeholder={placeholderFor(prop, label)}
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

function BooleanControl({ descriptionId, onChange, prop, title, value }: ControlProps) {
  const checked = typeof value === "boolean" ? value : schemaDefault(prop) === true;
  return (
    <Switch
      aria-describedby={descriptionId}
      checked={checked}
      label={title}
      onCheckedChange={(next) => onChange(next)}
    />
  );
}

function optionLabel(option: unknown) {
  return option === "" ? '""' : String(option);
}

function SelectControl({ descriptionId, id, label, onChange, prop, value }: ControlProps) {
  const options = selectOptions(prop) ?? [];
  const encoded = value === undefined ? "" : JSON.stringify(value);
  const known = options.some((option) => JSON.stringify(option) === encoded);
  const fallback = schemaDefault(prop);
  const defaultLabel = fallback === undefined
    ? label("editor.mdcPropsDefault")
    : `${label("editor.mdcPropsDefault")} (${optionLabel(fallback)})`;

  return (
    <Select
      aria-describedby={descriptionId}
      id={id}
      value={encoded}
      onChange={(event) => {
        const next = event.currentTarget.value;
        onChange(next === "" ? undefined : JSON.parse(next));
      }}
    >
      <option value="">{defaultLabel}</option>
      {options.map((option) => (
        <option key={JSON.stringify(option)} value={JSON.stringify(option)}>{optionLabel(option)}</option>
      ))}
      {!known && value !== undefined ? <option value={encoded}>{optionLabel(value)}</option> : null}
    </Select>
  );
}

function move<T>(items: readonly T[], from: number, to: number) {
  const next = [...items];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item!);
  return next;
}

function ListRowActions({
  count,
  index,
  label,
  name,
  onMove,
  onRemove,
}: {
  count: number;
  index: number;
  label: Label;
  name: string;
  onMove: (to: number) => unknown;
  onRemove: () => unknown;
}) {
  return (
    <div className="flex shrink-0 items-center">
      <IconButton
        disabled={index === 0}
        label={`${label("editor.mdcPropsMoveUp")}: ${name}`}
        size="icon-xs"
        onClick={() => onMove(index - 1)}
      >
        <ArrowUp aria-hidden="true" size={12} />
      </IconButton>
      <IconButton
        disabled={index === count - 1}
        label={`${label("editor.mdcPropsMoveDown")}: ${name}`}
        size="icon-xs"
        onClick={() => onMove(index + 1)}
      >
        <ArrowDown aria-hidden="true" size={12} />
      </IconButton>
      <IconButton label={`${label("editor.mdcPropsRemoveItem")}: ${name}`} size="icon-xs" onClick={onRemove}>
        <Trash2 aria-hidden="true" size={12} />
      </IconButton>
    </div>
  );
}

function AddItemButton({ label, onClick }: { label: Label; onClick: () => unknown }) {
  return (
    <Button className="justify-self-start" size="sm" onClick={onClick}>
      <Plus aria-hidden="true" size={13} />
      {label("editor.mdcPropsAddItem")}
    </Button>
  );
}

/** Writes a list, removing the prop when the list is emptied so it falls back to its default. */
function listChange(onChange: ChangeValue, topLevel: boolean | undefined) {
  return (next: unknown[]) => onChange(next.length === 0 && topLevel ? undefined : next);
}

function useFocusAfterAdd(id: string) {
  const [pending, setPending] = useState<number | null>(null);
  useEffect(() => {
    if (pending === null) return;
    document.getElementById(`${id}-${pending}`)?.focus();
    setPending(null);
  }, [id, pending]);
  return setPending;
}

function StringListRow({
  id,
  onChange,
  value,
  ...actions
}: Parameters<typeof ListRowActions>[0] & { id: string; onChange: (value: string) => unknown; value: string }) {
  const { draft, edit, onBlur } = useDraft(value);
  return (
    <li className="flex items-center gap-1">
      <TextInput
        aria-label={actions.name}
        className="h-8"
        id={id}
        value={draft}
        onBlur={onBlur}
        onChange={(event) => {
          edit(event.currentTarget.value);
          onChange(event.currentTarget.value);
        }}
      />
      <ListRowActions {...actions} />
    </li>
  );
}

function StringListControl({ id, label, onChange, title, topLevel, value }: ControlProps) {
  const items = Array.isArray(value) ? (value as string[]) : [];
  const write = listChange(onChange, topLevel);
  const focusRow = useFocusAfterAdd(id);

  return (
    <div aria-labelledby={`${id}-label`} className="grid gap-1.5" role="group">
      {items.length > 0 ? (
        <ol className="m-0 grid list-none gap-1 p-0">
          {items.map((item, index) => (
            <StringListRow
              count={items.length}
              id={`${id}-${index}`}
              index={index}
              key={index}
              label={label}
              name={`${title} ${index + 1}`}
              value={item}
              onChange={(next) => write(items.map((current, at) => (at === index ? next : current)))}
              onMove={(to) => write(move(items, index, to))}
              onRemove={() => write(items.filter((_, at) => at !== index))}
            />
          ))}
        </ol>
      ) : null}
      <AddItemButton
        label={label}
        onClick={() => {
          write([...items, ""]);
          focusRow(items.length);
        }}
      />
    </div>
  );
}

function newItem(fields: readonly EditorMdcPropSchema[]) {
  const item: Record<string, unknown> = {};
  for (const field of fields) {
    if (field.required && field.type === "string") item[field.name] = "";
  }
  return item;
}

function itemTitle(item: Record<string, unknown>, fields: readonly EditorMdcPropSchema[]) {
  for (const field of fields) {
    const value = item[field.name];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}

function ObjectListControl({ id, label, onChange, prop, topLevel, value }: ControlProps) {
  const items = Array.isArray(value) ? (value as Record<string, unknown>[]) : [];
  const fields = prop.items ?? [];
  const write = listChange(onChange, topLevel);
  const [open, setOpen] = useState<ReadonlySet<number>>(() => new Set());
  const focusItem = useFocusAfterAdd(id);

  const toggle = (index: number) => setOpen((current) => {
    const next = new Set(current);
    if (!next.delete(index)) next.add(index);
    return next;
  });
  const moveOpen = (from: number, to: number) => setOpen((current) => {
    const flags = move(items.map((_, index) => current.has(index)), from, to);
    return new Set(flags.flatMap((flag, index) => (flag ? [index] : [])));
  });

  return (
    <div aria-labelledby={`${id}-label`} className="grid gap-1.5" role="group">
      {items.map((item, index) => {
        const expanded = open.has(index);
        const name = itemTitle(item, fields) ?? `${label("editor.mdcPropsItem")} ${index + 1}`;
        const bodyId = `${id}-${index}-body`;
        return (
          <div className="rounded-md border border-(--border-default) bg-(--bg-primary)" key={index}>
            <div className="flex items-center gap-1 py-1 pr-1 pl-1.5">
              <button
                aria-controls={bodyId}
                aria-expanded={expanded}
                className="flex min-w-0 flex-1 cursor-pointer items-center gap-1 rounded border-0 bg-transparent p-1 text-left text-[12px] leading-5 font-[560] text-(--text-heading) hover:bg-(--bg-hover) focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-(--accent)"
                id={`${id}-${index}`}
                type="button"
                onClick={() => toggle(index)}
              >
                <ChevronRight
                  aria-hidden="true"
                  className={`shrink-0 text-(--text-secondary) transition-transform duration-150 ${expanded ? "rotate-90" : ""}`}
                  size={13}
                />
                <span className="truncate">{name}</span>
              </button>
              <ListRowActions
                count={items.length}
                index={index}
                label={label}
                name={name}
                onMove={(to) => {
                  moveOpen(index, to);
                  write(move(items, index, to));
                }}
                onRemove={() => {
                  setOpen((current) => new Set([...current].filter((at) => at !== index).map((at) => (at > index ? at - 1 : at))));
                  write(items.filter((_, at) => at !== index));
                }}
              />
            </div>
            {expanded ? (
              <div className="grid gap-3 border-t border-(--border-default) px-3 py-3" id={bodyId}>
                {fields.map((field) => (
                  <MdcField
                    key={field.name}
                    label={label}
                    prop={field}
                    value={item[field.name]}
                    onChange={(next) => {
                      const updated = { ...item };
                      if (next === undefined) delete updated[field.name];
                      else updated[field.name] = next;
                      write(items.map((current, at) => (at === index ? updated : current)));
                    }}
                  />
                ))}
              </div>
            ) : null}
          </div>
        );
      })}
      <AddItemButton
        label={label}
        onClick={() => {
          setOpen((current) => new Set([...current, items.length]));
          write([...items, newItem(fields)]);
          focusItem(items.length);
        }}
      />
    </div>
  );
}

function JsonControl({ descriptionId, id, label, onChange, value }: ControlProps) {
  const { draft, edit, onBlur } = useDraft(formatMdcValue(value));
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useAutoGrow(draft);
  const errorId = `${id}-error`;

  return (
    <>
      <textarea
        aria-describedby={[descriptionId, error ? errorId : null].filter(Boolean).join(" ") || undefined}
        aria-invalid={error ? true : undefined}
        className={`${textareaClassName} font-mono text-[12px]`}
        id={id}
        ref={textareaRef}
        spellCheck={false}
        value={draft}
        onBlur={() => {
          // Invalid text stays so it can be fixed; valid text re-reads the document.
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
        <p className="m-0 text-[11px] leading-4 text-(--danger)" id={errorId} role="alert">
          {label("editor.mdcPropsInvalidValue")} {error}
        </p>
      ) : null}
    </>
  );
}
