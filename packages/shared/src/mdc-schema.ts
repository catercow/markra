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
};

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

  return prop;
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
