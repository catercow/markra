// Trimmed copy of the CaterCow marketing-content schema/components.json (descriptions shortened).
import type { MdcComponentSchema } from "./form-model.ts";

export const schemas: Record<string, MdcComponentSchema> = {
  "bullet-list": {
    "name": "bullet-list",
    "props": [
      {
        "name": "items",
        "type": "string[]",
        "required": false,
        "default": "[]",
        "description": "One line per bullet."
      },
      {
        "name": "variant",
        "type": "\"default\" | \"warning\"",
        "required": false,
        "default": "\"default\"",
        "options": [
          "default",
          "warning"
        ],
        "description": "`warning` styles the bullets as cautions."
      }
    ]
  },
  "callout": {
    "name": "callout",
    "props": [
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"\"",
        "description": "Small uppercase label above the text, e.g. \"Pro tip\"."
      }
    ]
  },
  "card": {
    "name": "card",
    "props": [
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"\"",
        "description": "Heading shown above the card's text."
      },
      {
        "name": "number",
        "type": "boolean",
        "required": false,
        "default": "false",
        "description": "Show a running number (#1, #2, ...) beside the card. Counts within the surroundi"
      }
    ]
  },
  "comparison-table": {
    "name": "comparison-table",
    "props": [
      {
        "name": "columns",
        "type": "string[]",
        "required": false,
        "default": "[]",
        "description": "Column headings, e.g. the options being compared."
      },
      {
        "name": "rows",
        "type": "ComparisonRow[]",
        "required": false,
        "default": "[]",
        "items": [
          {
            "name": "category",
            "type": "string",
            "required": true,
            "description": "Row heading, e.g. \"Price per person\"."
          },
          {
            "name": "values",
            "type": "string[]",
            "required": true,
            "description": "One value per column, in column order."
          }
        ],
        "description": "One row per thing being compared."
      }
    ]
  },
  "cta-banner": {
    "name": "cta-banner",
    "props": [
      {
        "name": "to",
        "type": "string",
        "required": true,
        "description": "Where the button goes: a site path like /get-started, or a full URL."
      },
      {
        "name": "cta",
        "type": "string",
        "required": true,
        "description": "Button label."
      }
    ]
  },
  "faqs": {
    "name": "faqs",
    "props": [
      {
        "name": "items",
        "type": "Faq[]",
        "required": false,
        "default": "[]",
        "items": [
          {
            "name": "question",
            "type": "string",
            "required": true,
            "description": "The question, as the reader would ask it."
          },
          {
            "name": "answer",
            "type": "string",
            "required": true,
            "description": "Plain-text answer."
          }
        ],
        "description": "Questions and answers, in display order."
      },
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"Frequently Asked Questions\"",
        "description": "Heading above the questions."
      },
      {
        "name": "structured-data",
        "type": "boolean",
        "required": false,
        "default": "false",
        "description": "Also tell search engines these are FAQs (FAQPage structured data). Turn on for o"
      }
    ]
  },
  "feature-grid": {
    "name": "feature-grid",
    "props": [
      {
        "name": "items",
        "type": "FeatureItem[]",
        "required": false,
        "default": "[]",
        "items": [
          {
            "name": "title",
            "type": "string",
            "required": true,
            "description": "Short heading for the feature."
          },
          {
            "name": "body",
            "type": "string",
            "required": false,
            "description": "One or two sentences explaining it."
          },
          {
            "name": "image",
            "type": "string",
            "required": false,
            "description": "Optional image or icon URL shown above the title."
          },
          {
            "name": "image-alt",
            "type": "string",
            "required": false,
            "description": "Alt text for the image. Leave empty if it is decorative."
          }
        ],
        "description": "The features to show, in order."
      },
      {
        "name": "columns",
        "type": "2 | 3 | 4",
        "required": false,
        "default": "3",
        "description": "How many columns on a wide screen."
      }
    ]
  },
  "grid": {
    "name": "grid",
    "props": [
      {
        "name": "columns",
        "type": "\"2\" | \"1\" | \"3\" | \"4\"",
        "required": false,
        "default": "\"2\"",
        "options": [
          "2",
          "1",
          "3",
          "4"
        ],
        "description": "Columns on wide screens (1 to 4). Narrow screens stack to one column."
      }
    ]
  },
  "hero": {
    "name": "hero",
    "props": [
      {
        "name": "image",
        "type": "string",
        "required": false,
        "default": "\"\"",
        "description": "Background image. Falls back to the default catering photo."
      }
    ]
  },
  "page-section": {
    "name": "page-section",
    "props": [
      {
        "name": "background",
        "type": "\"none\" | \"off-white\" | \"cream\" | \"pale-blue\"",
        "required": false,
        "default": "\"none\"",
        "options": [
          "none",
          "off-white",
          "cream",
          "pale-blue"
        ],
        "description": "Background color, to set a group of blocks apart from the page."
      },
      {
        "name": "width",
        "type": "\"content\" | \"feature\" | \"full\"",
        "required": false,
        "default": "\"feature\"",
        "options": [
          "content",
          "feature",
          "full"
        ],
        "description": "How wide the section runs."
      },
      {
        "name": "heading",
        "type": "string",
        "required": false,
        "default": "\"\"",
        "description": "Optional centered heading at the top of the section."
      }
    ]
  },
  "pricing-grid": {
    "name": "pricing-grid",
    "props": [
      {
        "name": "items",
        "type": "PricedItem[]",
        "required": false,
        "default": "[]",
        "items": [
          {
            "name": "name",
            "type": "string",
            "required": true,
            "description": "Item or package name."
          },
          {
            "name": "count",
            "type": "string",
            "required": false,
            "description": "Second line under the name, e.g. \"Serves 10\"."
          },
          {
            "name": "price",
            "type": "number",
            "required": false,
            "description": "Price in dollars, e.g. 12.5."
          },
          {
            "name": "per-person",
            "type": "boolean",
            "required": false,
            "description": "Add \"/ person\" after the price."
          },
          {
            "name": "upcharge",
            "type": "number",
            "required": false,
            "description": "Extra cost over the base option, in dollars. Shown only when above 0."
          },
          {
            "name": "description",
            "type": "string",
            "required": false
          },
          {
            "name": "secondary_description",
            "type": "string",
            "required": false,
            "description": "Smaller text under the description."
          },
          {
            "name": "tag",
            "type": "string",
            "required": false,
            "description": "Badge above the name, e.g. \"Most popular\"."
          }
        ],
        "description": "One card per item."
      }
    ]
  }
}
;
