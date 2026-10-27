import { useEffect, useMemo, useState } from "react";
import { parseMdcSchema, type MdcSchema } from "@markra/shared";
import { getAppRuntime } from "../runtime";

export const mdcSchemaRelativePath = "schema/components.json";

export function mdcSchemaPathForRoot(rootPath: string) {
  const separator = /[\\/]$/u.test(rootPath) ? "" : "/";

  return `${rootPath}${separator}${mdcSchemaRelativePath}`;
}

/**
 * The components schema: `served` (CowMark's schema from the site, see useContentSchema) when
 * given, else `schema/components.json` from the workspace root, reloaded when the root changes
 * and whenever the window regains focus, so regenerating the schema outside Markra is picked up.
 */
export function useMdcSchema(rootPath: string | null, served?: string | null): MdcSchema | null {
  const fromFile = useWorkspaceMdcSchema(served === undefined ? rootPath : null);
  const fromServed = useMemo(() => (served ? parseMdcSchema(served) : null), [served]);

  return served === undefined ? fromFile : fromServed;
}

function useWorkspaceMdcSchema(rootPath: string | null): MdcSchema | null {
  const [schema, setSchema] = useState<MdcSchema | null>(null);

  useEffect(() => {
    setSchema(null);
    if (!rootPath) return;

    let active = true;
    let lastText: string | null = null;
    const schemaPath = mdcSchemaPathForRoot(rootPath);
    const load = () => {
      // Read through the runtime directly so runtimes without the reader simply yield no schema.
      const readWorkspaceTextFile = getAppRuntime().files.readWorkspaceTextFile;
      if (!readWorkspaceTextFile) return;

      readWorkspaceTextFile(schemaPath)
        .catch(() => null)
        .then((text) => {
          if (!active || text === lastText) return;
          lastText = text;
          setSchema(text === null ? null : parseMdcSchema(text));
        });
    };

    load();
    window.addEventListener("focus", load);

    return () => {
      active = false;
      window.removeEventListener("focus", load);
    };
  }, [rootPath]);

  return schema;
}
