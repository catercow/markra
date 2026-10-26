# MDC components (CaterCow fork)

This fork edits [MDC](https://content.nuxt.com/docs/files/markdown) (Nuxt Content's Markdown Components) for the CaterCow marketing content repository. It is not proposed upstream.

## When it is on

- Always, in a folder that contains `schema/components.json`. The content repository generates that file from the site's components.
- Anywhere else, when **Settings → Editor → MDC components** is on. It is off by default.

## What it does

- **Parsing** (`packages/editor/src/codemirror/mdc-syntax.ts`): `::name{attrs}` blocks, a `---` YAML props block directly after the opener, `#slot` lines, `::` closers, `:name[label]{attrs}` inline components, and `{attrs}` after spans, emphasis, code and links. Openers and closers are separate nodes and the content between them is parsed as ordinary Markdown. `readMdcBlocks` pairs them by colon count.
- **Cards** (`mdc-preview.ts`): each block is drawn as a card, with nested cards indented. The opener shows as a chip with the component's name and an Edit button; the props are YAML-highlighted. Moving the caret onto the opener or the closer shows its source.
- **Props form** (`mdc-edit.ts`, `packages/editor/src/mdc/`): Edit opens a form generated from `schema/components.json`. Each change rewrites only that prop, in the braces or in the YAML block, and leaves the rest of the text byte for byte.
- **Insert** (`mdc-insert.ts`, `packages/editor/src/mdc/components.ts`): with a schema, the slash menu has a Components section, grouped by `group` and searchable by name, component id and purpose. The block "+" button opens the same menu with the components first. Choosing one inserts the schema `example` (or a bare block) as its own block, adds colons when it goes inside another component, and opens the props form.
- **Lint** (`mdc-lint.ts`): diagnostics for unknown components, missing, unknown and mistyped props (list item fields included), and unclosed blocks and props blocks. A card hides its opener line, so problems found there underline the card's label instead.

The file is saved exactly as typed. Cards and forms never change text by themselves.

## Site preview (desktop only)

When the open folder has a `preview.json` (`{ "site": "https://…" }`), the globe button in the title bar opens the site beside the editor. The site shows the folder's changes before they are committed:

- **On load**, the site sends `catercow:draft:ready` with the commit it was built from. `content_preview_pending` (`apps/desktop/src-tauri/src/content_preview.rs`) diffs the folder against that commit with git (committed, staged, unstaged and untracked files under `content/` and `public/`), and the panel posts every change. Without that commit it falls back to `origin/studio` and says so.
- **While open**, `content_preview_poll` rescans the folder each second, so saves from any app appear. The open document is sent as you type, unsaved edits included.
- Git is the system `git` or the copy inside GitHub Desktop.

The site accepts messages only from origins in its `NUXT_PUBLIC_DRAFT_PREVIEW_ALLOWED_ORIGINS`, which must include `tauri://localhost` (macOS) and `http://tauri.localhost` (Windows). The protocol and the site side are in the CaterCow monorepo, `js/nuxt4/modules/draft-preview`.

## CowMark: sign-in, Save to studio, Get latest

Builds made with `VITE_CATERCOW_PREVIEW_SITE` set (the CaterCow release workflow sets it) revolve around one content folder, so editors never use git or GitHub themselves. Without it, the app behaves exactly like stock Markra.

- **First launch** shows a sign-in screen. Google sign-in runs in the browser against the preview site (`/__content_access/*`, PKCE with a loopback redirect to `127.0.0.1`), which checks the email against its allowlist. The app then downloads the content repository's `studio` branch to `~/Documents/catercow-content`; the location can be changed only during this setup. The session and folder are stored in `content-access.json` in the app data directory.
- **Every later launch** opens that folder straight away, signed in or not, offline included. When signed in and nothing is waiting to be saved, it quietly gets the latest changes; a failure is ignored. Opening other files or folders is turned off.
- **The preview panel footer** lists changed files and has a message box, **Save to studio** (saves open documents, commits, rebases on `origin/studio`, pushes) and **Get latest** (fetch and rebase, keeping uncommitted edits). If someone else changed the same lines, the rebase is undone and the editor's commit stays on this computer; the footer says to ask a developer.
- **Expired session (401):** the session is cleared and the footer asks to sign in again; edits are kept. **Removed from the allowlist (403):** "This email can no longer save CaterCow content. Ask a developer."
- **Tokens** are requested only to download, save or get latest. They are short-lived GitHub tokens from the preview site, held in memory, and reach git only through `GIT_CONFIG_COUNT` environment variables as an `http.https://github.com/.extraheader` (with `credential.helper` emptied). They never appear in command arguments, URLs, `.git/config` or logs. This needs git 2.31 or later.
- **Code:** `apps/desktop/src-tauri/src/content_access.rs` (sign-in, session, tokens) and `content_git.rs` (clone, status, save, update); `packages/app/src/components/CowMarkGate.tsx` and `ContentSaveBar.tsx`. The server side is in the CaterCow monorepo.

## Differences from plain Markdown while MDC is on

- **Indented code blocks are off.** Nested components are often indented four spaces or more, and MDC renderers dedent them, so such lines are content, not code. Fenced code blocks still work, including indented ones.
- **A line that is only `#name`** (no space) is read as a slot, not a paragraph.

## Tests

- `mdc-syntax.test.ts` checks the grammar against the input of every [comark](https://github.com/comarkdown/comark) spec fixture that uses components. The inputs are vendored in `mdc-syntax.fixtures.ts`.
- The grammar was also checked against all 349 marketing pages, which have 1,045 components; every block and every props block closes.

## Building and installing CowMark

The fork ships as its own app, **CowMark** (`com.catercow.cowmark`), CaterCow's edition of Markra,, so it can sit next to a stock Markra install without sharing settings. `apps/desktop/src-tauri/tauri.catercow.conf.json` sets the name and ID. It also leaves out the Quick Look extension, whose bundle ID is tied to upstream's app ID.

- **Release:** push a `catercow-v*` tag (for example `catercow-v2.14.1-mdc.2`). `.github/workflows/catercow-desktop.yml` builds a macOS `.dmg` for Apple Silicon and Intel and a Windows `setup.exe`, then attaches them to a GitHub release on this fork.
- **Not notarized or code-signed.** The first launch needs one confirmation:
  - **Mac:** right-click the app and choose Open, or allow it under System Settings → Privacy & Security.
  - **Windows:** SmartScreen shows "Windows protected your PC"; choose More info → Run anyway.
- **No auto-update.** The updater is only configured in upstream's release workflow, so these builds never replace themselves with stock Markra. Install a new release to update.
- **Local build** (needs Rust 1.85+): `pnpm --filter @markra/desktop tauri build --config src-tauri/tauri.catercow.conf.json --bundles dmg`. Without Xcode 26 this uses the classic `.icns` icon. CI adds `tauri.catercow-ci.conf.json` to build the adaptive icon.
