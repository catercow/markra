import { useCallback } from "react";
import { isMarkdownPath, pathNameFromPath, type I18nKey } from "@markra/shared";
import { showAppToast } from "../lib/app-toast";
import { catercowPreviewSite, managedFilePath } from "../lib/content-access";
import type { NativeMarkdownFolderFile } from "../lib/tauri";
import { getAppRuntime } from "../runtime";
import type { DiskFileMerge } from "./useMarkdownDocument";

const mergeToastId = "cowmark-disk-merge";

/**
 * CowMark only: merging disk changes into unsaved edits, and its notice.
 * Stock Markra keeps ignoring such changes.
 */
export function useCowMarkDiskMerge(translate: (key: I18nKey) => string) {
  const mergeDiskText = catercowPreviewSite() ? getAppRuntime().files.mergeText : undefined;
  const onDiskFileMerged = useCallback(({ conflicts }: DiskFileMerge) => {
    showAppToast({
      duration: conflicts > 0 ? 10000 : undefined,
      id: mergeToastId,
      message: translate(conflicts > 0 ? "app.cowmark.externalConflicts" : "app.cowmark.externalMerged"),
      status: conflicts > 0 ? "warning" : "success",
      surface: "notice"
    });
  }, [translate]);

  return { mergeDiskText, onDiskFileMerged };
}

/**
 * Opens the Markdown files a save or Get latest left with conflict markers:
 * the rest in background tabs, the first in the editor. Already open clean
 * tabs reload from disk when shown, so their conflict bars appear.
 */
export async function openConflictFiles(
  root: string,
  paths: readonly string[],
  open: (file: NativeMarkdownFolderFile) => Promise<unknown>,
  openInBackground?: (file: NativeMarkdownFolderFile) => Promise<unknown>
) {
  const files = paths
    .filter((path) => isMarkdownPath(path))
    .map((relativePath) => ({ name: pathNameFromPath(relativePath), path: managedFilePath(root, relativePath), relativePath }));
  const [first, ...rest] = files;
  if (openInBackground) {
    for (const file of rest) await openInBackground(file);
  }
  if (first) await open(first);
}
