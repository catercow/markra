import { describe, expect, it } from "vitest";
import { formatFrontmatterValue, readFrontmatterFields, writeFrontmatterField } from "./frontmatter-fields.ts";

const post = `title: 'Papa John''s Catering: Get a $100 Gift Card'
author:
  name: CaterCow Staff
  slug: catercow # the house account
description: Can you order Papa John's catering? Yes.
excerpt: ''
feature_image: /blog/10-papa-johns-catering-coupons/papa-john.jpg
featured: false
tags:
  - slug: catering-promotions
    name: Catering Promotions

# Dates are ISO strings.
published_at: '2026-10-06T12:06:00.000+00:00'
`;

describe("readFrontmatterFields", () => {
  it("reads values and key order", () => {
    const read = readFrontmatterFields(post);
    expect(read.error).toBeUndefined();
    expect(read.keys).toEqual(["title", "author", "description", "excerpt", "feature_image", "featured", "tags", "published_at"]);
    expect(read.values.title).toBe("Papa John's Catering: Get a $100 Gift Card");
    expect(read.values.author).toEqual({ name: "CaterCow Staff", slug: "catercow" });
  });

  it("reports invalid YAML", () => {
    expect(readFrontmatterFields("title: [oops\n").error).toMatch(/Invalid YAML/u);
  });
});

describe("writeFrontmatterField", () => {
  it("rewrites only the changed scalar and keeps its quoting", () => {
    const next = writeFrontmatterField(post, "title", "Papa John's: $50 off").text;
    expect(next).toBe(post.replace("'Papa John''s Catering: Get a $100 Gift Card'", "'Papa John''s: $50 off'"));
  });

  it("keeps plain scalars plain and quotes only when required", () => {
    expect(writeFrontmatterField(post, "description", "Short.").text).toBe(
      post.replace("description: Can you order Papa John's catering? Yes.", "description: Short.")
    );
    const quoted = writeFrontmatterField(post, "description", "Note: colons need quotes").text;
    expect(readFrontmatterFields(quoted).values.description).toBe("Note: colons need quotes");
    expect(quoted.replace(/^description: .*$/mu, "")).toBe(post.replace(/^description: .*$/mu, ""));
  });

  it("toggles booleans in place", () => {
    expect(writeFrontmatterField(post, "featured", true).text).toBe(post.replace("featured: false", "featured: true"));
  });

  it("replaces a whole block collection and leaves the comment after it", () => {
    const next = writeFrontmatterField(post, "tags", [
      { slug: "catering-promotions", name: "Catering Promotions" },
      { slug: "office-life", name: "Office Life" }
    ]).text;
    expect(next).toBe(
      post.replace(
        "    name: Catering Promotions\n",
        "    name: Catering Promotions\n  - slug: office-life\n    name: Office Life\n"
      )
    );
  });

  it("removes a key with its line", () => {
    expect(writeFrontmatterField(post, "excerpt", undefined).text).toBe(post.replace("excerpt: ''\n", ""));
    expect(writeFrontmatterField(post, "tags", undefined).text).toBe(
      post.replace("tags:\n  - slug: catering-promotions\n    name: Catering Promotions\n", "")
    );
  });

  it("adds a key in alphabetical position after the title", () => {
    const next = writeFrontmatterField(post, "meta_title", "Papa John's Promo Codes").text;
    expect(next).toBe(post.replace("tags:\n", "meta_title: Papa John's Promo Codes\ntags:\n"));
    expect(writeFrontmatterField("title: A\n", "author", { name: "Ada" }).text).toBe("title: A\nauthor:\n  name: Ada\n");
    expect(writeFrontmatterField("", "title", "A").text).toBe("title: A\n");
    expect(writeFrontmatterField("title: A", "zeta", 1).text).toBe("title: A\nzeta: 1\n");
  });

  it("quotes new ISO dates so YAML 1.1 readers keep them strings", () => {
    const next = writeFrontmatterField("title: A\n", "updated_at", "2026-10-26T12:00:00.000+00:00").text;
    expect(next).toBe("title: A\nupdated_at: '2026-10-26T12:00:00.000+00:00'\n");
  });

  it("fills an empty value", () => {
    expect(writeFrontmatterField("title: A\nfeature_image:\nfeatured: false\n", "feature_image", "a/b.png").text).toBe(
      "title: A\nfeature_image: a/b.png\nfeatured: false\n"
    );
  });

  it("refuses invalid YAML", () => {
    const write = writeFrontmatterField("title: [oops\n", "title", "B");
    expect(write.error).toMatch(/Invalid YAML/u);
    expect(write.text).toBe("title: [oops\n");
  });
});

describe("formatFrontmatterValue", () => {
  it("formats nested values as YAML", () => {
    expect(formatFrontmatterValue([{ a: 1 }])).toBe("- a: 1");
    expect(formatFrontmatterValue(undefined)).toBe("");
  });
});
