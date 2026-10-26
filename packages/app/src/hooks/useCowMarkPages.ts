import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { createMarkdownImageSrcResolver } from "@markra/markdown";
import { getAppRuntime } from "../runtime";
import {
  collectFieldValues,
  collectionsSchemaRelativePath,
  parseCollectionsSchema,
  type FieldValues,
  type PageCollection,
} from "../lib/cowmark-pages";
import { saveContentImage } from "../lib/cowmark-images";
import { openNativeLocalImages, saveNativeClipboardImage } from "../lib/tauri";

export type CowMarkPages = {
  collections: PageCollection[];
  rootPath: string;
  /** Values the collection's files already use (see `collectFieldValues`); rescanned after window focus. */
  loadFieldValues: (collection: PageCollection) => Promise<FieldValues>;
  /** Asks for an image file and stores it next to the page; resolves the relative reference. */
  pickImage: (documentPath: string) => Promise<string | null>;
  /** A displayable URL for an image reference written in the page at `documentPath`. */
  resolveImage: (documentPath: string | null | undefined, src: string) => string;
};

export const CowMarkPagesContext = createContext<CowMarkPages | null>(null);

export function useCowMarkPagesContext() {
  return useContext(CowMarkPagesContext);
}

function joinPath(root: string, relative: string) {
  return `${root.replace(/[\\/]+$/u, "")}/${relative}`;
}

/**
 * Resolves image references the way the site does: relative to the page, and site paths
 * ("/blog/x.jpg") from the content repository's public/ folder.
 */
export function resolveContentImageSrc(rootPath: string, documentPath: string | null | undefined, src: string) {
  if (/^\/(?!\/)/u.test(src)) {
    return createMarkdownImageSrcResolver(joinPath(rootPath, "public/index.md"))(`.${src}`);
  }
  return createMarkdownImageSrcResolver(documentPath)(src);
}

type Options = {
  /** Shown when an image can't be stored. */
  onImageError: (error: unknown) => unknown;
  refreshTree: (documentPath: string) => Promise<unknown>;
  rootPath: string | null;
};

/** CowMark's page tools for the managed content folder; null outside CowMark. */
export function useCowMarkPages({ onImageError, refreshTree, rootPath }: Options): CowMarkPages | null {
  const [collections, setCollections] = useState<PageCollection[]>([]);
  const valuesRef = useRef(new Map<string, Promise<FieldValues>>());
  const callbacksRef = useRef({ onImageError, refreshTree });
  callbacksRef.current = { onImageError, refreshTree };

  useEffect(() => {
    setCollections([]);
    valuesRef.current.clear();
    if (!rootPath) return;

    let active = true;
    let lastText: string | null | undefined;
    const load = () => {
      valuesRef.current.clear();
      const read = getAppRuntime().files.readWorkspaceTextFile;
      if (!read) return;
      read(joinPath(rootPath, collectionsSchemaRelativePath))
        .catch(() => null)
        .then((text) => {
          if (!active || text === lastText) return;
          lastText = text;
          setCollections(parseCollectionsSchema(text));
        });
    };
    load();
    window.addEventListener("focus", load);
    return () => {
      active = false;
      window.removeEventListener("focus", load);
    };
  }, [rootPath]);

  const loadFieldValues = useCallback((collection: PageCollection) => {
    if (!rootPath) return Promise.resolve({});
    const cached = valuesRef.current.get(collection.folder);
    if (cached) return cached;
    const loading = (async () => {
      const files = getAppRuntime().files;
      const read = files.readWorkspaceTextFile;
      if (!read) return {};
      const listed = await files.listMarkdownFilesForPath(joinPath(rootPath, collection.folder)).catch(() => []);
      const texts = await Promise.all(
        listed.filter((file) => /\.md$/iu.test(file.path)).map((file) => read(file.path).catch(() => null))
      );
      return collectFieldValues(texts.filter((text): text is string => typeof text === "string"));
    })();
    valuesRef.current.set(collection.folder, loading);
    return loading;
  }, [rootPath]);

  const pickImage = useCallback(async (documentPath: string) => {
    try {
      const [image] = await openNativeLocalImages();
      if (!image) return null;
      const saved = await saveContentImage({ documentPath, image, saveLocalImage: saveNativeClipboardImage });
      await callbacksRef.current.refreshTree(documentPath).catch(() => {});
      return saved.src;
    } catch (error) {
      callbacksRef.current.onImageError(error);
      return null;
    }
  }, []);

  const resolveImage = useCallback(
    (documentPath: string | null | undefined, src: string) => (rootPath ? resolveContentImageSrc(rootPath, documentPath, src) : src),
    [rootPath]
  );

  return useMemo(
    () => (rootPath ? { collections, loadFieldValues, pickImage, resolveImage, rootPath } : null),
    [collections, loadFieldValues, pickImage, resolveImage, rootPath]
  );
}
