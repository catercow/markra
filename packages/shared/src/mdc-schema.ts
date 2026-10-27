import { isRecord } from "./record.ts";

// Mirrors the `schema/components.json` file generated in an MDC content workspace.
// Every field beyond `name` is optional so older or partial schema files still load.
export type MdcPropSchema = {
  name: string;
  type?: string;
  required?: boolean;
  description?: string;
  /** Source-level default, e.g. `"\"none\""` or `"3"`; not parsed. */
  default?: string;
  options?: string[];
  /** Fields of each element when the prop is an array of objects. */
  items?: MdcPropSchema[];
  /** How the value is written: `search-link` (a CaterCow search URL) or `review-link`. */
  format?: string;
  /** For a `search-link`: the search filters the site knows, with their allowed values. */
  searchParams?: Record<string, { values: string[] | null }>;
};

/** A column of a table component's slot. */
export type MdcInputColumn = {
  /** The canonical header text. */
  name: string;
  aliases?: string[];
  required?: boolean;
  /** `text`, `price`, `code` or `link`. */
  format?: string;
  /** Inline Markdown is allowed in the cells. */
  markdown?: boolean;
};

/** What goes in a component's slot: one Markdown table, or Markdown images. */
export type MdcComponentInput = { kind: "table"; columns: MdcInputColumn[] } | { kind: "images" };

export type MdcComponentExample = {
  from?: string;
  markdown: string;
};

export type MdcComponentSchema = {
  /** Kebab-case name used in `::name` blocks. */
  name: string;
  /** Vue component name, e.g. `AddressCta`. */
  component?: string;
  group?: string;
  purpose?: string;
  props: MdcPropSchema[];
  /** Whether the component renders a default slot body. */
  content?: boolean;
  example?: MdcComponentExample;
  input?: MdcComponentInput;
};

export type MdcSchema = {
  generatedBy?: string;
  components: MdcComponentSchema[];
};

function optionalString(value: unknown) {
  return typeof value === "string" ? value : undefined;
}

function nonEmptyString(value: unknown) {
  return typeof value === "string" && value.trim() ? value : null;
}

function parseProp(value: unknown): MdcPropSchema | null {
  if (!isRecord(value) || Array.isArray(value)) return null;
  const name = nonEmptyString(value.name);
  if (!name) return null;

  const prop: MdcPropSchema = { name };
  const type = optionalString(value.type);
  const description = optionalString(value.description);
  if (type !== undefined) prop.type = type;
  if (typeof value.required === "boolean") prop.required = value.required;
  if (description !== undefined) prop.description = description;
  if (value.default !== undefined && value.default !== null) {
    prop.default = typeof value.default === "string" ? value.default : JSON.stringify(value.default);
  }
  if (Array.isArray(value.options)) {
    prop.options = value.options.filter((option): option is string => typeof option === "string");
  }
  if (Array.isArray(value.items)) {
    prop.items = parseProps(value.items);
  }
  const format = optionalString(value.format);
  if (format !== undefined) prop.format = format;
  const searchParams = parseSearchParams(value.searchParams);
  if (searchParams) prop.searchParams = searchParams;

  return prop;
}

function stringList(value: unknown) {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : undefined;
}

function parseSearchParams(value: unknown): MdcPropSchema["searchParams"] {
  if (!isRecord(value) || Array.isArray(value)) return undefined;
  const params: NonNullable<MdcPropSchema["searchParams"]> = {};
  for (const [name, param] of Object.entries(value)) {
    params[name] = { values: isRecord(param) ? stringList(param.values) ?? null : null };
  }
  return params;
}

function parseInput(value: unknown): MdcComponentInput | undefined {
  if (!isRecord(value)) return undefined;
  if (value.kind === "images") return { kind: "images" };
  if (value.kind !== "table" || !Array.isArray(value.columns)) return undefined;
  const columns: MdcInputColumn[] = [];
  for (const entry of value.columns) {
    if (!isRecord(entry)) continue;
    const name = nonEmptyString(entry.name);
    if (!name) continue;
    const column: MdcInputColumn = { name };
    const aliases = stringList(entry.aliases);
    if (aliases) column.aliases = aliases;
    if (typeof entry.required === "boolean") column.required = entry.required;
    const format = optionalString(entry.format);
    if (format !== undefined) column.format = format;
    if (typeof entry.markdown === "boolean") column.markdown = entry.markdown;
    columns.push(column);
  }
  return columns.length > 0 ? { columns, kind: "table" } : undefined;
}

function parseProps(values: unknown[]) {
  return values.map(parseProp).filter((prop): prop is MdcPropSchema => prop !== null);
}

function parseComponent(value: unknown): MdcComponentSchema | null {
  if (!isRecord(value) || Array.isArray(value)) return null;
  const name = nonEmptyString(value.name);
  if (!name) return null;

  const component: MdcComponentSchema = {
    name,
    props: Array.isArray(value.props) ? parseProps(value.props) : []
  };
  const componentName = optionalString(value.component);
  const group = optionalString(value.group);
  const purpose = optionalString(value.purpose);
  if (componentName !== undefined) component.component = componentName;
  if (group !== undefined) component.group = group;
  if (purpose !== undefined) component.purpose = purpose;
  if (typeof value.content === "boolean") component.content = value.content;
  if (isRecord(value.example) && typeof value.example.markdown === "string") {
    const from = optionalString(value.example.from);
    component.example = from === undefined
      ? { markdown: value.example.markdown }
      : { from, markdown: value.example.markdown };
  }
  const input = parseInput(value.input);
  if (input) component.input = input;

  return component;
}

/**
 * Parses the text of an MDC `components.json` schema. Returns null when the text is not
 * JSON or has no `components` array; malformed component and prop entries are dropped.
 */
export function parseMdcSchema(text: string): MdcSchema | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || !Array.isArray(parsed.components)) return null;

  const schema: MdcSchema = {
    components: parsed.components
      .map(parseComponent)
      .filter((component): component is MdcComponentSchema => component !== null)
  };
  const generatedBy = optionalString(parsed.generatedBy);
  if (generatedBy !== undefined) schema.generatedBy = generatedBy;

  return schema;
}
