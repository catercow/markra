import { useEffect, useState } from "react";
import { getAppRuntime } from "../runtime";
import {
  contentSchemaNotice,
  contentSchemaRefreshMs,
  type ContentSchemaNotice
} from "../lib/content-schema";

export type ContentSchemaState = {
  components: string | null;
  collections: string | null;
  notice: ContentSchemaNotice | null;
};

/**
 * CowMark: the content schema for the managed folder, from the site (see lib/content-schema.ts).
 * Loads when the folder opens and whenever the window regains focus (the backend reuses a fetch
 * for ten minutes), and fetches again every ten minutes. Null until the first load, and always
 * without a folder or outside the desktop app; callers then read the folder's `schema/` files.
 */
export function useContentSchema(rootPath: string | null, site: string | null): ContentSchemaState | null {
  const [state, setState] = useState<ContentSchemaState | null>(null);

  useEffect(() => {
    setState(null);
    const load = getAppRuntime().files.contentSchema;
    if (!rootPath || !load) return;

    let active = true;
    let lastKey: string | null = null;
    const refresh = (force: boolean) => {
      load(rootPath, site, force)
        .then((result) => {
          if (!active) return;
          const notice = contentSchemaNotice(result);
          const key = `${result.hash}|${notice?.kind ?? ""}|${notice?.site ?? ""}`;
          if (key === lastKey) return;
          lastKey = key;
          setState({ collections: result.collections, components: result.components, notice });
        })
        // A failed call keeps what was loaded; the next focus or tick tries again.
        .catch(() => {});
    };

    refresh(false);
    const onFocus = () => refresh(false);
    window.addEventListener("focus", onFocus);
    const timer = window.setInterval(() => refresh(true), contentSchemaRefreshMs);

    return () => {
      active = false;
      window.removeEventListener("focus", onFocus);
      window.clearInterval(timer);
    };
  }, [rootPath, site]);

  return state;
}
