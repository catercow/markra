// Trimmed copy of the CaterCow marketing-content schema/components.json: a subset of
// components, prop descriptions dropped; examples, groups and purposes as generated.
import type { MdcCatalogComponent } from "./components.ts";

export const catalog: MdcCatalogComponent[] = [
  {
    "name": "address-cta",
    "component": "AddressCta",
    "group": "Calls to action",
    "purpose": "Address box that sends the reader to catering search for their address; use near the top and end of a page.",
    "props": [
      {
        "name": "headline",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "subhead",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "note",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "forward-to",
        "type": "string",
        "required": false,
        "default": "\"/search\""
      },
      {
        "name": "cta",
        "type": "string",
        "required": false,
        "default": "\"Find catering\""
      },
      {
        "name": "background",
        "type": "\"none\" | \"warm\" | \"blue\"",
        "required": false,
        "default": "\"none\"",
        "options": [
          "none",
          "warm",
          "blue"
        ]
      }
    ],
    "content": true,
    "example": {
      "markdown": "::address-cta\n---\nheadline: Find baby shower catering near you\nsubhead: Enter the shower's delivery address to see brunch spreads, platters and finger foods from vetted local restaurants.\nnote: Brunch spreads · Platters & trays · On-time delivery guarantee\nforward-to: /search?search_type=packages&h1=Baby+Shower+Catering+Near+You\ncta: Search\nbackground: warm\n---\n::"
    }
  },
  {
    "name": "bullet-list",
    "component": "BulletList",
    "group": "Text and lists",
    "purpose": "Bulleted list from a list of strings; the warning variant flags cautions.",
    "props": [
      {
        "name": "items",
        "type": "string[]",
        "required": false,
        "default": "[]"
      },
      {
        "name": "variant",
        "type": "\"default\" | \"warning\"",
        "required": false,
        "default": "\"default\"",
        "options": [
          "default",
          "warning"
        ]
      }
    ],
    "content": true,
    "example": {
      "markdown": "::bullet-list{variant=\"warning\"}\n---\nitems:\n  - Ordering a single-protein bar (all chicken or all ground beef) for 25+ people — you'll have disappointed guests.\n  - Assuming guacamole comes free — it's a hard $2+ upcharge that gets listed separately on the receipt.\n  - Ordering delivery without confirming the fee and time window — delivery varies wildly by location and distance.\n  - Not labeling dietary restrictions — Qdoba labels items by name, not by allergen or dietary flags (vegan, gluten-free).\n---\n::"
    }
  },
  {
    "name": "callout",
    "component": "Callout",
    "group": "Text and lists",
    "purpose": "Highlighted note or tip set apart from the body text.",
    "props": [
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"\""
      }
    ],
    "content": true,
    "example": {
      "markdown": "::callout{title=\"Need every meal labeled?\"}\nCaterCow delivers Italian catering from local restaurants, including [individually boxed pasta meals](https://www.catercow.com/dishes/individually-packaged-italian-and-pasta) labeled with each guest's name and dietary needs. Your first order of $100 or more also earns a [$100 Olive Garden gift card](https://www.catercow.com/blog/8-olive-garden-catering-promo-codes).\n::"
    }
  },
  {
    "name": "card",
    "component": "Card",
    "group": "Layout",
    "purpose": "Boxed block with a title and an optional step number; use inside a grid.",
    "props": [
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "number",
        "type": "boolean",
        "required": false,
        "default": "false"
      }
    ],
    "content": true,
    "example": {
      "markdown": "::card{title=\"Use the 2-Hour Lead Time as Your Edge\" number}\nApplebee's own catering page confirms orders can be placed with \"as little as 2 hours notice.\" That's the shortest published lead time we've seen across casual dining chains and the strongest reason to keep Applebee's in your back pocket for last-minute office needs.\n::"
    }
  },
  {
    "name": "chipotle-cost-calculator",
    "component": "ChipotleCostCalculator",
    "group": "Calculators",
    "purpose": "Interactive calculator estimating Chipotle catering cost by metro and headcount. No props.",
    "props": [],
    "content": false,
    "example": {
      "markdown": "::chipotle-cost-calculator\n::"
    }
  },
  {
    "name": "cta-banner",
    "component": "CtaBanner",
    "group": "Calls to action",
    "purpose": "Banner with text and one button linking to a CaterCow page; the main call to action.",
    "props": [
      {
        "name": "to",
        "type": "string",
        "required": true
      },
      {
        "name": "cta",
        "type": "string",
        "required": true
      }
    ],
    "content": true,
    "example": {
      "markdown": "::cta-banner{to=\"https://www.catercow.com/brands/9109-chickfila\" cta=\"Order Chick-fil-A catering →\"}\n**Last checked: October 7, 2026.** Chick-fil-A rarely offers promo codes for catering. On CaterCow, you can get **$100 off any Chick-fil-A catering order of $200 or more** in Chicago, Washington, DC, and Atlanta. Enter your city's code at checkout. Codes are valid through March 31, 2027.\n::"
    }
  },
  {
    "name": "faqs",
    "component": "Faqs",
    "group": "Text and lists",
    "purpose": "Frequently asked questions with plain-text answers; can emit FAQ structured data for search engines.",
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
            "required": true
          },
          {
            "name": "answer",
            "type": "string",
            "required": true
          }
        ]
      },
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"Frequently Asked Questions\""
      },
      {
        "name": "structured-data",
        "type": "boolean",
        "required": false,
        "default": "false"
      }
    ],
    "content": true,
    "example": {
      "markdown": "::faqs\n---\ntitle: \"Baby Shower Catering FAQ\"\nstructured-data: true\nitems:\n  - question: 'What food is best for a baby shower?'\n    answer: 'The best baby shower catering foods are light, brunch-friendly, and easy to eat while standing or socializing. Popular choices include mini quiches, tea sandwiches, charcuterie boards, fruit platters, salad bars, mini croissant sandwiches, and bite-sized desserts. These menu options photograph beautifully and fit the relaxed, celebratory atmosphere of most baby showers.'\n  - question: 'How much does baby shower catering cost?'\n    answer: 'Baby shower catering usually costs between $12 and $25 per person for drop-off or buffet-style service. Sandwich platters and fruit trays are often the most affordable options, while brunch spreads, charcuterie boards, and full-service catering tend to cost more. Total pricing depends on guest count, menu choices, delivery fees, and whether staff or rental items are included. For a 25 to 35 person shower, expect a total of $300 to $800.'\n  - question: 'How far in advance should I order baby shower catering?'\n    answer: 'It is best to order baby shower catering 2 to 3 weeks in advance, especially for showers in the spring and early summer. Shower season overlaps with bridal showers and graduation parties, so booking early improves your chances of getting your preferred menu, delivery window, and restaurant.'\n  - question: 'What is the easiest food for a baby shower?'\n    answer: 'Drop-off platters are usually the easiest option for a baby shower. Charcuterie boards, mini sandwich trays, fruit displays, and brunch boxes are simple to serve and let guests graze when they arrive. These catering formats work especially well for at-home showers where the host is also managing dec'\n---\n::"
    }
  },
  {
    "name": "grid",
    "component": "Grid",
    "group": "Layout",
    "purpose": "Lays out the blocks inside it in 1 to 4 columns.",
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
        ]
      }
    ],
    "content": true,
    "example": {
      "markdown": ":::grid{columns=2}\n::card{title=\"Stack Dunkin' Rewards\" number}\nThe Dunkin' Rewards program earns points on most in-store and app orders, including large dozen-boxes and Box O' Joe. If you're the one regularly ordering for the team, route the orders through your own Rewards account — a 20-person breakfast run can earn you a free drink, and the points roll over.\n::\n\n::card{title=\"Call ahead for 5+ dozen\" number}\nThe online ordering flow caps at 3 dozen donuts. For anything bigger, the recommended approach is to phone the store the day before, ask to speak with the morning manager, and confirm a pickup time. Walking in cold with a 5-dozen request will get you waited on, but you'll wait — and you might not get your first-choice flavors.\n::\n:::"
    }
  },
  {
    "name": "hero",
    "component": "Hero",
    "group": "Layout",
    "purpose": "Large image banner with text at the top of a page.",
    "props": [
      {
        "name": "image",
        "type": "string",
        "required": false,
        "default": "\"\""
      }
    ],
    "content": true,
    "example": {
      "markdown": "::hero{image=\"/images/catering-guides/catering-guides-default.jpg\"}\n**2026 Mediterranean Catering Guide**\n\n# CAVA Catering Guide (2026): Group Bowl Bar, Pita Packs, Real Prices & How It Compares\n::"
    }
  },
  {
    "name": "instagram-post",
    "component": "InstagramPost",
    "group": "Layout",
    "purpose": "Embedded Instagram post.",
    "props": [
      {
        "name": "url",
        "type": "string",
        "required": true
      },
      {
        "name": "caption",
        "type": "string",
        "required": false
      }
    ],
    "content": false,
    "example": {
      "markdown": "::instagram-post\n---\nurl: https://www.instagram.com/p/DMd8x-TRioJ/\ncaption: A post shared by The Hapeville Dwarf House (@thedwarfhouse)\n---\n::"
    }
  },
  {
    "name": "page-section",
    "component": "PageSection",
    "group": "Layout",
    "purpose": "Full-width section with an optional heading and background that groups the blocks inside it.",
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
        ]
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
        ]
      },
      {
        "name": "heading",
        "type": "string",
        "required": false,
        "default": "\"\""
      }
    ],
    "content": true,
    "example": null
  },
  {
    "name": "pricing-grid",
    "component": "PricingGrid",
    "group": "Pricing and numbers",
    "purpose": "Grid of menu items with counts, prices, per-person prices and descriptions.",
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
            "required": true
          },
          {
            "name": "count",
            "type": "string",
            "required": false
          },
          {
            "name": "price",
            "type": "number",
            "required": false
          },
          {
            "name": "per-person",
            "type": "boolean",
            "required": false
          },
          {
            "name": "upcharge",
            "type": "number",
            "required": false
          },
          {
            "name": "description",
            "type": "string",
            "required": false
          },
          {
            "name": "secondary_description",
            "type": "string",
            "required": false
          },
          {
            "name": "tag",
            "type": "string",
            "required": false
          }
        ]
      }
    ],
    "content": true,
    "example": {
      "markdown": "::pricing-grid\n---\nitems:\n  - name: The Classic Breakfast Egg Burrito Pack\n    price: 11.50\n    upcharge: 0\n    description: 10 Classic Breakfast Burritos with scrambled eggs, Jack cheese, and fillings. Feeds 10. ($115.00 total.)\n    tag: Insider Pick\n---\n\n::"
    }
  },
  {
    "name": "promo-codes",
    "component": "PromoCodes",
    "group": "Pricing and numbers",
    "purpose": "Table of promo codes, one row per location, with copy buttons and fine print.",
    "props": [
      {
        "name": "items",
        "type": "PromoCode[]",
        "required": false,
        "default": "[]",
        "items": [
          {
            "name": "label",
            "type": "string",
            "required": true
          },
          {
            "name": "url",
            "type": "string",
            "required": false
          },
          {
            "name": "code",
            "type": "string",
            "required": true
          },
          {
            "name": "note",
            "type": "string",
            "required": false
          }
        ]
      },
      {
        "name": "label-heading",
        "type": "string",
        "required": false,
        "default": "\"Location\""
      },
      {
        "name": "code-heading",
        "type": "string",
        "required": false,
        "default": "\"Code\""
      },
      {
        "name": "note-heading",
        "type": "string",
        "required": false,
        "default": "\"Details\""
      }
    ],
    "content": true,
    "example": {
      "markdown": "::promo-codes\n---\nlabel-heading: Area\ncode-heading: Code for a $100 Wawa Gift Card\nitems:\n  - label: New York City & North Jersey\n    url: https://www.catercow.com/explore/new-york-city/cuisines/sandwich-catering\n    code: 100OFFWAWANYC\n  - label: Washington, DC Area\n    url: https://www.catercow.com/explore/washington-dc/cuisines/sandwich-catering\n    code: 100OFFWAWADC\n---\nValid on your first CaterCow order of $100+ through March 31, 2027. Gift card arrives by email in about two weeks.\n::"
    }
  },
  {
    "name": "quote",
    "component": "Quote",
    "group": "Text and lists",
    "purpose": "Pull quote or testimonial with the speaker's name, title and company.",
    "props": [
      {
        "name": "name",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "title",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "company",
        "type": "string",
        "required": false,
        "default": "\"\""
      },
      {
        "name": "avatar",
        "type": "string",
        "required": false,
        "default": "\"\""
      }
    ],
    "content": true,
    "example": {
      "markdown": "::quote{name=\"Quang Nguyen\" title=\"Owner\" company=\"Umami Tsunami Taqueria\"}\nOrder a mix for inclusivity & variety: **60% barbacoa / 40% carnitas** (covers pork-free needs while delighting pork fans).\n::"
    }
  },
  {
    "name": "steps",
    "component": "Steps",
    "group": "Text and lists",
    "purpose": "Numbered how-to steps.",
    "props": [
      {
        "name": "items",
        "type": "Step[]",
        "required": false,
        "default": "[]",
        "items": [
          {
            "name": "title",
            "type": "string",
            "required": true
          },
          {
            "name": "body",
            "type": "string",
            "required": false
          }
        ]
      }
    ],
    "content": true,
    "example": null
  }
]
;

export function component(name: string) {
  return catalog.find((candidate) => candidate.name === name)!;
}
