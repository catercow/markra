import { afterEach, describe, expect, it } from "vitest";
import { setCatercowPreviewSiteForTests } from "./content-access";
import { editionAppName, editionFeatures, editionHidden, editionLabel, withoutHiddenCommands } from "./cowmark-edition";
import type { AppFeatureRuntime } from "../runtime";

const allFeatures = {
  ai: true,
  export: true,
  markdownBundle: true,
  networkProxy: true,
  pandoc: true,
  s3ImageUpload: true,
  updater: true
} as AppFeatureRuntime;

describe("cowmark edition", () => {
  afterEach(() => {
    setCatercowPreviewSiteForTests(undefined);
  });

  it("changes nothing in stock Markra", () => {
    setCatercowPreviewSiteForTests(null);
    const hidden = editionHidden();

    expect(editionAppName()).toBe("Markra");
    expect(editionLabel("Quit Markra")).toBe("Quit Markra");
    expect(editionFeatures(allFeatures)).toBe(allFeatures);
    expect(hidden.menuCommands.size).toBe(0);
    expect(hidden.settingsCategories).toEqual([]);
    expect(hidden.slashCommands).toEqual([]);
    expect(withoutHiddenCommands({ openFolder: 1 }, hidden.menuCommands)).toEqual({ openFolder: 1 });
  });

  it("turns off what CowMark doesn't offer", () => {
    setCatercowPreviewSiteForTests("https://preview.example.test");
    const hidden = editionHidden();

    expect(editionAppName()).toBe("CowMark");
    expect(editionLabel("Quit Markra")).toBe("Quit CowMark");
    expect(editionFeatures(allFeatures)).toMatchObject({ ai: false, export: false, updater: false, pandoc: false });
    expect(hidden.menuCommands.has("openFolder")).toBe(true);
    expect(withoutHiddenCommands({ openFolder: 1, saveDocument: 2 }, hidden.menuCommands)).toEqual({ saveDocument: 2 });
  });
});
