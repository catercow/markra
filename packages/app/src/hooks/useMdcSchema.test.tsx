import { act, renderHook, waitFor } from "@testing-library/react";
import { configureAppRuntime, getAppRuntime, resetAppRuntimeForTests } from "../runtime";
import { mdcSchemaPathForRoot, useMdcSchema } from "./useMdcSchema";

const mockedReadNativeWorkspaceTextFile = vi.fn<(path: string) => Promise<string | null>>();

function schemaText(names: string[]) {
  return JSON.stringify({ components: names.map((name) => ({ name, props: [] })) });
}

describe("useMdcSchema", () => {
  beforeEach(() => {
    mockedReadNativeWorkspaceTextFile.mockReset();
    const runtime = getAppRuntime();
    configureAppRuntime({
      ...runtime,
      files: { ...runtime.files, readWorkspaceTextFile: mockedReadNativeWorkspaceTextFile }
    });
  });

  afterEach(() => {
    resetAppRuntimeForTests();
  });

  it("joins the schema path onto the workspace root", () => {
    expect(mdcSchemaPathForRoot("/vault")).toBe("/vault/schema/components.json");
    expect(mdcSchemaPathForRoot("/vault/")).toBe("/vault/schema/components.json");
    expect(mdcSchemaPathForRoot("web-folder://abc")).toBe("web-folder://abc/schema/components.json");
  });

  it("loads and parses the schema from the workspace root", async () => {
    mockedReadNativeWorkspaceTextFile.mockResolvedValue(schemaText(["callout"]));

    const { result } = renderHook(() => useMdcSchema("/vault"));

    await waitFor(() => expect(result.current?.components.map((component) => component.name)).toEqual(["callout"]));
    expect(mockedReadNativeWorkspaceTextFile).toHaveBeenCalledWith("/vault/schema/components.json");
  });

  it("returns null without a root, a schema file, or valid JSON", async () => {
    const { result, rerender } = renderHook(({ root }) => useMdcSchema(root), {
      initialProps: { root: null as string | null }
    });

    expect(result.current).toBeNull();
    expect(mockedReadNativeWorkspaceTextFile).not.toHaveBeenCalled();

    mockedReadNativeWorkspaceTextFile.mockResolvedValueOnce(null);
    rerender({ root: "/missing" });
    await waitFor(() => expect(mockedReadNativeWorkspaceTextFile).toHaveBeenCalledTimes(1));
    expect(result.current).toBeNull();

    mockedReadNativeWorkspaceTextFile.mockResolvedValueOnce("{ not json");
    rerender({ root: "/broken" });
    await waitFor(() => expect(mockedReadNativeWorkspaceTextFile).toHaveBeenCalledTimes(2));
    expect(result.current).toBeNull();
  });

  it("clears the previous workspace schema and reloads on window focus", async () => {
    mockedReadNativeWorkspaceTextFile.mockResolvedValueOnce(schemaText(["callout"]));
    const { result, rerender } = renderHook(({ root }) => useMdcSchema(root), {
      initialProps: { root: "/vault" as string | null }
    });
    await waitFor(() => expect(result.current?.components).toHaveLength(1));

    mockedReadNativeWorkspaceTextFile.mockResolvedValueOnce(schemaText(["callout", "hero"]));
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(result.current?.components).toHaveLength(2));

    rerender({ root: null });
    expect(result.current).toBeNull();
  });
});
