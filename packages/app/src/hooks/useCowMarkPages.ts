import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { createMarkdownImageSrcResolver } from "@markra/markdown";
import { getAppRuntime } from "../runtime";
import {
  collectFieldValues,
  collectionsSchemaRelativePath,
  pageImageFolder,
  parseCollectionsSchema,
  rootRelativePath,
  type FieldValues,
  type PageCollection,
} from "../lib/cowmark-pages";
import { saveContentImage } from "../lib/cowmark-images";
import { openNativeLocalImages, saveNativeContentSiteImage } from "../lib/tauri";

export type CowMarkPages = {
  collections: PageCollection[];
  rootPath: string;
  /** Values the collection's files already use (see `collectFieldValues`); rescanned after window focus. */
  loadFieldValues: (collection: PageCollection) => Promise<FieldValues>;
  /** The page's image folder as a site path (`/blog/my-post`), or null outside `content/`. */
  imageFolder: (documentPath: string | null | undefined) => string | null;
  /** Asks for an image file and stores it in the page's image folder; resolves its site path. */
  pickImage: (documentPath: string) => Promise<string | null>;
  /** Stores an image in the page's image folder under `public/`; resolves its site path. */
  saveImage: (documentPath: string, image: File) => Promise<string>;
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
 * Resolves image references the way the site does: site paths ("/blog/x.jpg") from the content
 * repository's public/ folder. Other references, and documents outside the folder, resolve as
 * in stock Markra.
 */
export function resolveContentImageSrc(rootPath: string, documentPath: string | null | undefined, src: string) {
  if (/^\/(?!\/)/u.test(src) && rootRelativePath(rootPath, documentPath) !== null) {
    return createMarkdownImageSrcResolver(joinPath(rootPath, "public/index.md"))(`.${src}`);
  }
  return createMarkdownImageSrcResolver(documentPath)(src);
}

type Options = {
  /** Shown when an image can't be stored. */
  onImageError: (error: unknown) => unknown;
  refreshTree: (documentPath: string) => Promise<unknown>;
  rootPath: string | null;
  /** The collections schema the site serves (see useContentSchema); else `schema/collections.json` is read. */
  collectionsText?: string | null;
};

/** CowMark's page tools for the managed content folder; null outside CowMark. */
export function useCowMarkPages({ collectionsText, onImageError, refreshTree, rootPath }: Options): CowMarkPages | null {
  const [collections, setCollections] = useState<PageCollection[]>([]);
  const valuesRef = useRef(new Map<string, Promise<FieldValues>>());
  const callbacksRef = useRef({ onImageError, refreshTree });
  callbacksRef.current = { onImageError, refreshTree };

  useEffect(() => {
    setCollections([]);
    valuesRef.current.clear();
    if (!rootPath) return;
    if (collectionsText !== undefined) {
      setCollections(parseCollectionsSchema(collectionsText));
      return;
    }

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
  }, [collectionsText, rootPath]);

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

  const imageFolder = useCallback(
    (documentPath: string | null | undefined) => {
      const relative = rootPath ? rootRelativePath(rootPath, documentPath) : null;
      return relative ? pageImageFolder(collections, relative) : null;
    },
    [collections, rootPath]
  );

  const saveImage = useCallback(
    async (documentPath: string, image: File) => {
      const folder = imageFolder(documentPath);
      if (!rootPath || !folder) throw new Error("Images can only be added to pages under content/.");
      const saved = await saveContentImage({
        documentPath,
        image,
        imageFolder: folder,
        rootPath,
        saveSiteImage: saveNativeContentSiteImage,
      });
      await callbacksRef.current.refreshTree(documentPath).catch(() => {});
      return saved.src;
    },
    [imageFolder, rootPath]
  );

  const pickImage = useCallback(async (documentPath: string) => {
    try {
      const [image] = await openNativeLocalImages();
      if (!image) return null;
      return await saveImage(documentPath, image);
    } catch (error) {
      callbacksRef.current.onImageError(error);
      return null;
    }
  }, [saveImage]);

  const resolveImage = useCallback(
    (documentPath: string | null | undefined, src: string) => (rootPath ? resolveContentImageSrc(rootPath, documentPath, src) : src),
    [rootPath]
  );

  return useMemo(
    () => (rootPath ? { collections, imageFolder, loadFieldValues, pickImage, resolveImage, rootPath, saveImage } : null),
    [collections, imageFolder, loadFieldValues, pickImage, resolveImage, rootPath, saveImage]
  );
}
