// CowMark: the content schema (components and collections) the site serves at
// `/_cowmark/schema.json`, loaded by the desktop backend (`cowmark_schema`, see
// crates/cowmark-core/src/schema_source.rs) with its cached copy and the folder's
// `schema/` files as fallbacks. See docs/mdc.md.

export type ContentSchemaOrigin =
  | { kind: "site"; url: string }
  | { kind: "cache"; url: string; ageSecs: number }
  | { kind: "dir"; path: string }
  | { kind: "repo" }
  | { kind: "none" };

export type ContentSchemaResult = {
  components: string | null;
  collections: string | null;
  origin: ContentSchemaOrigin;
  /** Why a better source wasn't used, e.g. a failed fetch. */
  warnings: string[];
  hash: string;
  /** The endpoint tried, if any. */
  url: string | null;
  /** A cached copy older than a week. */
  stale: boolean;
};

/** Loads the schema for the folder `root`; `site` is used when the folder has no `preview.json`. */
export type ContentSchemaLoader = (root: string, site: string | null, force: boolean) => Promise<ContentSchemaResult>;

export type ContentSchemaNotice = { kind: "outOfDate" | "missing"; site: string };

/** How often the schema is fetched again while the folder is open. */
export const contentSchemaRefreshMs = 10 * 60 * 1000;

/** The site's host for messages: `https://www.catercow.com/_cowmark/schema.json` → `www.catercow.com`. */
export function contentSchemaSiteLabel(url: string | null) {
  if (!url) return "the site";
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}

/**
 * Nothing while the site's schema (or a recent cached copy) is in use. Otherwise a small notice:
 * "out of date" for a week-old cached copy, or the folder's own files after a failed fetch;
 * "missing" when there is no schema at all.
 */
export function contentSchemaNotice(result: ContentSchemaResult | null): ContentSchemaNotice | null {
  if (!result) return null;
  const site = contentSchemaSiteLabel(result.url ?? ("url" in result.origin ? result.origin.url : null));
  switch (result.origin.kind) {
    case "none":
      return { kind: "missing", site };
    case "cache":
      return result.stale ? { kind: "outOfDate", site } : null;
    case "repo":
      return result.warnings.length ? { kind: "outOfDate", site } : null;
    default:
      return null;
  }
}
