import { describe, expect, it } from "vitest";
import { isEquivalentEditorMarkdown } from "./document-model.ts";

describe("isEquivalentEditorMarkdown with MDC components", () => {
  const component = "::hero\n---\ntitle: A\n---\n::";

  it("does not read a component's YAML props fences as setext headings", () => {
    expect(isEquivalentEditorMarkdown(component, "## ::hero\ntitle: A\n---\n::")).toBe(false);
    expect(isEquivalentEditorMarkdown(component, "::hero\n---\n## title: A\n::")).toBe(false);
  });

  it("still treats identical components as equivalent", () => {
    expect(isEquivalentEditorMarkdown(component, `${component}\n`)).toBe(true);
  });
});
