# MDC components (CaterCow fork)

This fork edits [MDC](https://content.nuxt.com/docs/files/markdown) (Nuxt Content's Markdown Components) for the CaterCow marketing content repository. It is not proposed upstream.

## When it is on

- Always, in a folder that contains `schema/components.json`. The content repository generates that file from the site's components.
- Anywhere else, when **Settings → Editor → MDC components** is on. It is off by default.

## What it does

- **Parsing** (`packages/editor/src/codemirror/mdc-syntax.ts`): `::name{attrs}` blocks, a `---` YAML props block directly after the opener, `#slot` lines, `::` closers, `:name[label]{attrs}` inline components, and `{attrs}` after spans, emphasis, code and links. Openers and closers are separate nodes and the content between them is parsed as ordinary Markdown. `readMdcBlocks` pairs them by colon count.
- **Cards** (`mdc-preview.ts`): each block is drawn as a card, with nested cards indented. The opener shows as a chip with the component's name and an Edit button; the props are YAML-highlighted. Moving the caret onto the opener or the closer shows its source.
- **Props form** (`mdc-edit.ts`, `packages/editor/src/mdc/`): Edit opens a form generated from `schema/components.json`. Each change rewrites only that prop, in the braces or in the YAML block, and leaves the rest of the text byte for byte.

The file is saved exactly as typed. Cards and forms never change text by themselves.

## Differences from plain Markdown while MDC is on

- **Indented code blocks are off.** Nested components are often indented four spaces or more, and MDC renderers dedent them, so such lines are content, not code. Fenced code blocks still work, including indented ones.
- **A line that is only `#name`** (no space) is read as a slot, not a paragraph.

## Tests

- `mdc-syntax.test.ts` checks the grammar against the input of every [comark](https://github.com/comarkdown/comark) spec fixture that uses components. The inputs are vendored in `mdc-syntax.fixtures.ts`.
- The grammar was also checked against all 349 marketing pages, which have 1,045 components; every block and every props block closes.
