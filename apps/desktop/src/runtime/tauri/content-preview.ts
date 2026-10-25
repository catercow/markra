import { invoke } from "@tauri-apps/api/core";
import type { AppRuntime } from "@markra/app/runtime";
import { invokeNative } from "./invoke";

type ContentPreviewRuntime = NonNullable<AppRuntime["files"]["contentPreview"]>;

export const nativeContentPreview: ContentPreviewRuntime = {
  pending: (root, previewCommit) => invokeNative("content_preview_pending", { previewCommit, root }),
  // Polled about once a second, so it bypasses invokeNative's per-call logging.
  poll: (root) => invoke("content_preview_poll", { root }),
  readMedia: (root, path) => invoke<ArrayBuffer>("content_preview_read_media", { path, root }),
  stop: (root) => invoke("content_preview_stop", { root })
};
