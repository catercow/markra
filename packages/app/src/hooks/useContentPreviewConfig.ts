import { useEffect, useState } from "react";
import {
  contentPreviewConfigPathForRoot,
  parseContentPreviewConfig,
  type ContentPreviewConfig
} from "../lib/content-preview";
import { getAppRuntime } from "../runtime";

/**
 * Loads `preview.json` from the workspace root. Null unless the runtime can list git changes,
 * so the preview is only offered where it can show local edits. Reloads on window focus.
 */
export function useContentPreviewConfig(rootPath: string | null): ContentPreviewConfig | null {
  const [config, setConfig] = useState<ContentPreviewConfig | null>(null);

  useEffect(() => {
    setConfig(null);
    const { contentPreview, readWorkspaceTextFile } = getAppRuntime().files;
    if (!rootPath || !contentPreview || !readWorkspaceTextFile) return;

    let active = true;
    let lastText: string | null | undefined;
    const load = () => {
      readWorkspaceTextFile(contentPreviewConfigPathForRoot(rootPath))
        .catch(() => null)
        .then((text) => {
          if (!active || text === lastText) return;
          lastText = text;
          setConfig(parseContentPreviewConfig(text));
        });
    };

    load();
    window.addEventListener("focus", load);

    return () => {
      active = false;
      window.removeEventListener("focus", load);
    };
  }, [rootPath]);

  return config;
}
