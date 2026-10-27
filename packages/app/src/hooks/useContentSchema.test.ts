import { act, renderHook, waitFor } from "@testing-library/react";
import { configureAppRuntime, getAppRuntime, resetAppRuntimeForTests } from "../runtime";
import { contentSchemaRefreshMs, type ContentSchemaLoader, type ContentSchemaResult } from "../lib/content-schema";
import { useContentSchema } from "./useContentSchema";

const url = "https://content-preview.catercow.com/_cowmark/schema.json";

function served(name: string, partial: Partial<ContentSchemaResult> = {}): ContentSchemaResult {
  const components = JSON.stringify({ components: [{ name, props: [] }] });
  return {
    collections: JSON.stringify({ collections: [] }),
    components,
    hash: components,
    origin: { kind: "site", url },
    stale: false,
    url,
    warnings: [],
    ...partial
  };
}

describe("useContentSchema", () => {
  const load = vi.fn<ContentSchemaLoader>();

  beforeEach(() => {
    load.mockReset();
    const runtime = getAppRuntime();
    configureAppRuntime({ ...runtime, files: { ...runtime.files, contentSchema: load } });
  });

  afterEach(() => {
    vi.useRealTimers();
    resetAppRuntimeForTests();
  });

  it("loads when the folder opens, again on focus, and fetches every ten minutes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    load.mockResolvedValue(served("banner"));
    const { result } = renderHook(() => useContentSchema("/content", "https://content-preview.catercow.com"));

    await waitFor(() => expect(result.current?.components).toContain("banner"));
    expect(load).toHaveBeenCalledWith("/content", "https://content-preview.catercow.com", false);
    expect(result.current?.notice).toBeNull();
    const first = result.current;

    // The same schema again keeps the same state.
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    expect(result.current).toBe(first);

    load.mockResolvedValue(served("hero"));
    await act(async () => {
      vi.advanceTimersByTime(contentSchemaRefreshMs);
    });
    expect(load).toHaveBeenLastCalledWith("/content", "https://content-preview.catercow.com", true);
    await waitFor(() => expect(result.current?.components).toContain("hero"));
  });

  it("carries a notice when the site couldn't be reached and keeps the last schema on errors", async () => {
    load.mockResolvedValueOnce(
      served("old", { origin: { ageSecs: 30 * 86400, kind: "cache", url }, stale: true, warnings: ["Couldn't get it."] })
    );
    const { result } = renderHook(() => useContentSchema("/content", null));
    await waitFor(() => expect(result.current?.notice).toEqual({ kind: "outOfDate", site: "content-preview.catercow.com" }));

    load.mockRejectedValueOnce(new Error("backend gone"));
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    expect(result.current?.components).toContain("old");
  });

  it("does nothing without a folder or the desktop loader", () => {
    const { result } = renderHook(() => useContentSchema(null, "https://x.example.com"));
    expect(result.current).toBeNull();
    expect(load).not.toHaveBeenCalled();

    const runtime = getAppRuntime();
    configureAppRuntime({ ...runtime, files: { ...runtime.files, contentSchema: undefined } });
    const { result: stock } = renderHook(() => useContentSchema("/content", null));
    expect(stock.current).toBeNull();
  });
});
