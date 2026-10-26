import { invoke } from "@tauri-apps/api/core";
import type { AppRuntime } from "@markra/app/runtime";
import { invokeNative } from "./invoke";

type ContentPreviewRuntime = NonNullable<AppRuntime["files"]["contentPreview"]>;
type ContentAccessRuntime = NonNullable<AppRuntime["files"]["contentAccess"]>;

export const nativeContentPreview: ContentPreviewRuntime = {
  pending: (root, previewCommit) => invokeNative("content_preview_pending", { previewCommit, root }),
  // Polled about once a second, so it bypasses invokeNative's per-call logging.
  poll: (root) => invoke("content_preview_poll", { root }),
  readMedia: (root, path) => invoke<ArrayBuffer>("content_preview_read_media", { path, root }),
  stop: (root) => invoke("content_preview_stop", { root })
};

export const nativeContentAccess: ContentAccessRuntime = {
  clone: (dest) => invokeNative("content_access_clone", { dest }),
  isCheckout: (path) => invokeNative("content_access_is_checkout", { path }),
  save: (root, message, paths) => invokeNative("content_access_save", { message, paths, root }),
  setFolder: (path) => invokeNative("content_access_set_folder", { path }),
  signIn: (site) => invokeNative("content_access_sign_in", { site }),
  signOut: () => invokeNative("content_access_sign_out"),
  state: () => invokeNative("content_access_state"),
  // Polled while the preview panel is open, so it skips invokeNative's logging.
  status: (root) => invoke("content_access_status", { root }),
  update: (root) => invokeNative("content_access_update", { root })
};
