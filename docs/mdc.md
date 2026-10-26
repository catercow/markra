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
- **Content lint in CowMark.** For Markdown files inside the managed content folder, CowMark lints with the shared Rust crate `crates/cowmark-lint` instead of `mdc-lint.ts`, so the editor reports exactly what `cowmark check` and CI do: the component rules above plus front matter against `schema/collections.json`, missing images, image descriptions, links to missing pages and leftover conflict markers. The `cowmark_lint` command (`apps/desktop/src-tauri/src/content_lint.rs`) keeps one linter per folder and rebuilds it when either schema file changes. The app passes the folder-relative path and the message templates (`editor.mdcLint.*` and `editor.contentLint.*`); the diagnostic's source is the rule id. Other files, stock Markra and builds without the runtime command (`files.contentLint`) use `mdc-lint.ts`, as does a failed native call.

The file is saved exactly as typed. Cards and forms never change text by themselves.

## Site preview (desktop only)

When the open folder has a `preview.json` (`{ "site": "https://…" }`), the globe button in the title bar opens the site beside the editor. The site shows the folder's changes before they are committed:

- **On load**, the site sends `catercow:draft:ready` with the commit it was built from. `content_preview_pending` (`crates/cowmark-core/src/preview.rs`) diffs the folder against that commit with git (committed, staged, unstaged and untracked files under `content/` and `public/`), and the panel posts every change. Without that commit it falls back to `origin/studio` and says so.
- **While open**, `content_preview_poll` rescans the folder each second, so saves from any app appear. The open document is sent as you type, unsaved edits included.
- Git is the system `git` or the copy inside GitHub Desktop.

The site accepts messages only from origins in its `NUXT_PUBLIC_DRAFT_PREVIEW_ALLOWED_ORIGINS`, which must include `tauri://localhost` (macOS), `http://tauri.localhost` (Windows) and, for the browser preview, `http://localhost:4100` (not `127.0.0.1`). The protocol and the site side are in the CaterCow monorepo, `js/nuxt4/modules/draft-preview`.

## CowMark: sign-in, Save to studio, Get latest

Builds made with `VITE_CATERCOW_PREVIEW_SITE` set (the CaterCow release workflow sets it) revolve around one content folder, so editors never use git or GitHub themselves. Without it, the app behaves exactly like stock Markra.

- **First launch** shows a sign-in screen. Google sign-in runs in the browser against the preview site (`/__content_access/*`, PKCE with a loopback redirect to `127.0.0.1`), which checks the email against its allowlist. The app then downloads the content repository's `studio` branch to `~/Documents/catercow-content`; the location can be changed only during this setup. The session and folder are stored in `content-access.json` in the app data directory.
- **Every later launch** opens that folder straight away, signed in or not, offline included. When signed in and nothing is waiting to be saved, it quietly gets the latest changes; a failure is ignored. Opening other files or folders is turned off.
- **The preview panel footer** lists changed files with a checkbox each (none ticked by default; ticking `content/x/<slug>.md` also ticks the changed files in `content/x/<slug>/`) and has a message box, **Save to studio** (saves open documents, commits only the ticked files, rebases on `origin/studio` carrying the rest along, pushes; unticked changes stay on the computer as drafts) and **Get latest** (fetch and rebase, keeping uncommitted edits). If someone else changed the same lines, see **Conflicts** below.
- **Conflicts.** When the rebase (or re-applying uncommitted edits) can't combine the changes, `catch_up` in `crates/cowmark-core/src/git.rs` moves the folder to the latest `origin/studio` and replays every local change on top as uncommitted edits: the editor's commit, drafts and untracked files. Overlapping lines are written into the file between `<<<<<<< yours`, `=======` and `>>>>>>> theirs` (`merge.rs`). A file deleted on one side and edited on the other keeps the edit; a binary file changed on both sides keeps yours (theirs is in the commit). The whole folder is first recorded under `refs/cowmark/backup` (one reflog entry each time). The error is `conflict: <message>` followed by `\nconflict-files: <JSON list>`; the footer opens those files and says to resolve the highlighted conflicts, then save again. Save to studio refuses ticked files that still contain markers. When only a draft overlaps, the ticked commit is kept but not sent until the next save.
- **Changes from outside the app.** A clean tab reloads when its file changes on disk. A tab with unsaved edits is merged three ways against the content it last read or wrote (`content_merge_text`): changes to different lines are applied as one undoable edit and a notice says so; overlapping ones get the markers above. Either way the tab stays unsaved.
- **Conflict bars** (`packages/editor/src/codemirror/conflicts.ts`): each marker block shows your lines and their lines, highlighted, with **Keep mine**, **Use theirs** and **Keep both** above it; a choice rewrites only that block. A bar above the document counts the conflicts left and jumps to the next. Marker lines are parsed as their own blocks (`conflict-syntax.ts`), so `=======` doesn't turn your last line into a heading. Stock Markra builds show markers as plain text and ignore disk changes to unsaved tabs, as before.
- **Expired session (401):** the session is cleared and the footer asks to sign in again; edits are kept. **Removed from the allowlist (403):** "This email can no longer save CaterCow content. Ask a developer."
- **Tokens** are requested only to download, save or get latest. They are short-lived GitHub tokens from the preview site, held in memory, and reach git only through `GIT_CONFIG_COUNT` environment variables as an `http.https://github.com/.extraheader` (with `credential.helper` emptied). They never appear in command arguments, URLs, `.git/config` or logs. This needs git 2.31 or later.
- **Code:** `crates/cowmark-core` (`access.rs`: sign-in, session, tokens; `git.rs`: clone, status, save, update, conflicts; `merge.rs`: three-way merge; `preview_server.rs` and `control.rs`: see CowMark as a hub), wrapped as Tauri commands in `apps/desktop/src-tauri/src/content_access.rs`; `packages/app/src/components/CowMarkGate.tsx` and `ContentSaveBar.tsx`. The server side is in the CaterCow monorepo.

## CowMark as a hub

CowMark can stay in front while people edit in another app (VS Code, Obsidian, Claude Desktop). Everything runs inside the app: there is no background process, and quitting CowMark stops the browser preview and the control connection.

- **Companion view** (the title bar button next to the globe, remembered per user) hides the editor, file tree and AI panel. The preview and the save bar fill the window, sized for a narrow window beside another editor. The preview follows the most recently saved file: when one poll finds several saved pages, only the last navigates. Click the button again for the normal layout.
- **Browser preview.** While the managed folder is open, CowMark serves `http://localhost:4100` on 127.0.0.1 only (`crates/cowmark-core/src/preview_server.rs`). The page frames the site at the same path and relays the same draft messages as the panel: pending changes once the site is ready, then saved files over server-sent events, and images. Unsaved typing in CowMark's editor is not sent; saved files are. Every request needs the per-launch token in `?token=`, and the Host header must be `localhost:4100` or `127.0.0.1:4100`. **Open in browser** in the preview panel opens the current page there. The port is fixed because the site's allowlist names it exactly: if another program holds it, the panel says so and CowMark doesn't try another port.
- **The `cowmark` command** (to come: a CLI and MCP server) talks to the running app over the control connection below, and opens the app first if it isn't running. Publishing from it is a proposal: it ticks files and fills in the message, and a person clicks Save to studio.

### Control connection

`crates/cowmark-core/src/control.rs`; the app side is `apps/desktop/src-tauri/src/cowmark_companion.rs`.

- **Where.** In the app data dir (macOS: `~/Library/Application Support/com.catercow.cowmark`), the `control/` directory (mode 0700) holds `cowmark.sock`, a Unix domain socket (mode 0600), and `token`. On Windows the endpoint is the named pipe `\\.\pipe\cowmark-<id>`, where `<id>` is the first 16 hex digits of the SHA-256 of the app data dir path; its DACL allows only the pipe's owner and SYSTEM. `ControlPaths::for_data_dir` computes both.
- **Token.** `control/token` holds 64 hex characters, is rewritten (mode 0600) at every launch and deleted on quit. Every request carries it.
- **One owner.** The app starts listening when the managed folder opens. If another live CowMark answers on the endpoint, the second one doesn't listen. A socket file left by a crash is replaced.
- **Framing.** Newline-delimited JSON-RPC 2.0: one UTF-8 JSON object per line, at most 1 MiB. A connection can carry many requests; replies come back in order, one line each.

```json
{"jsonrpc":"2.0","id":1,"method":"propose_publish","params":{"paths":["content/blog/new-post.md"],"message":"New post"},"token":"<contents of control/token>"}
{"jsonrpc":"2.0","id":1,"result":{"paths":["content/blog/new-post.md"],"message":"New post"}}
```

Paths are relative to the content folder (or absolute inside it). Anything outside the folder, through `..` or a symlink, or inside `.git`, is rejected.

| Method | Params | Result |
| --- | --- | --- |
| `ping` | none | `{ app: "CowMark", version, protocol: 1, email, folder, previewUrl }`; `email` and `folder` are null when signed out or not set up, `previewUrl` when the browser preview isn't running |
| `status` | none | `{ branch, changes: [{ path, status }], ahead }`, as the save bar shows them |
| `focus` | none | `{}`; brings the window forward |
| `open` | `{ path }`, a Markdown file that exists | `{ path }`; opens it in the editor and shows it in the preview. Doesn't focus the window |
| `propose_publish` | `{ paths: [...], message }` | `{ paths, message }`; ticks exactly these files in the checklist (no folder expansion), fills in the trimmed message, shows and flashes the save bar and focuses the window. Every path must have changes. Never saves |

| Error code | Meaning |
| --- | --- |
| -32700 / -32600 / -32601 / -32602 | Not JSON / not a request / unknown method / bad params |
| -32001 | Missing or wrong token |
| -32002 | No content folder yet |
| -32003 | Path outside the content folder |
| -32004 | A proposed path has no changes |
| -32005 | The app couldn't do it (git failed, no window) |

`crates/cowmark-core/tests/control_client.rs` is a minimal client that calls every method; `tests/preview_server.rs` covers the browser preview.

## CowMark: images and page details

- **Images live next to the page.** For `content/blog/my-post.md`, pasted, dropped or chosen images are saved in `content/blog/my-post/` and referenced as `my-post/photo.jpg`. The site serves them at `/blog/my-post/photo.jpg`, the same URL space as `public/`, and rewrites relative references in Markdown, component props and image front matter fields at build time (`js/nuxt4/modules/content-media` in the monorepo). Older `/blog/…` references to `public/` keep working.
- **On import** the name becomes lowercase and hyphenated (a generic name like `image.png` becomes `<page>-image`), a clash gets `-2`, `-3`… Photos larger than 2400 px on the longest side are scaled down and re-encoded as JPEG or WebP; images with transparency stay PNG; SVG and GIF are copied as they are. The editor then asks for alt text; **Skip** inserts the image with empty alt text. Code: `packages/app/src/lib/cowmark-images.ts`.
- **Site preview** sends new images under `content/` as media, like those under `public/`.
- **Page details.** In a folder listed in `schema/collections.json`, the front matter header has a **Page details** button. The form is generated from the collection's fields in schema order: text with character counts for titles and descriptions, a list of values other pages in the folder already use, switches, numbers, a date picker that writes `YYYY-MM-DD`, and an image picker that stores the image next to the page. Fields the schema doesn't describe, and lists or objects, are edited as YAML under **Advanced**. Each change rewrites only that key and leaves the rest of the file byte for byte (`packages/editor/src/mdc/frontmatter-fields.ts`, `packages/app/src/components/PageDetailsPanel.tsx`).
- **New post** (the + button in the preview panel header) asks for a title and creates `content/blog/<slug>.md` with the title, today's date and the collection's other required fields, then opens it.

## Differences from plain Markdown while MDC is on

- **Indented code blocks are off.** Nested components are often indented four spaces or more, and MDC renderers dedent them, so such lines are content, not code. Fenced code blocks still work, including indented ones.
- **A line that is only `#name`** (no space) is read as a slot, not a paragraph.

## Tests

- `mdc-syntax.test.ts` checks the grammar against the input of every [comark](https://github.com/comarkdown/comark) spec fixture that uses components. The inputs are vendored in `mdc-syntax.fixtures.ts`.
- The grammar was also checked against all 349 marketing pages, which have 1,045 components; every block and every props block closes.

## Building and installing CowMark

The fork ships as its own app, **CowMark** (`com.catercow.cowmark`), CaterCow's edition of Markra,, so it can sit next to a stock Markra install without sharing settings. `apps/desktop/src-tauri/tauri.catercow.conf.json` sets the name and ID. It also leaves out the Quick Look extension, whose bundle ID is tied to upstream's app ID.

- **Release:** push a `catercow-v*` tag (for example `catercow-v2.14.1-mdc.2`). `.github/workflows/catercow-desktop.yml` builds a macOS `.dmg` for Apple Silicon and a Windows `setup.exe`, then attaches them to a GitHub release on this fork.
- **Not notarized or code-signed.** The first launch needs one confirmation:
  - **Mac:** right-click the app and choose Open, or allow it under System Settings → Privacy & Security.
  - **Windows:** SmartScreen shows "Windows protected your PC"; choose More info → Run anyway.
- **No auto-update.** The updater is only configured in upstream's release workflow, so these builds never replace themselves with stock Markra. Install a new release to update.
- **Local build** (needs Rust 1.85+): `pnpm --filter @markra/desktop tauri build --config src-tauri/tauri.catercow.conf.json --bundles dmg`. Without Xcode 26 this uses the classic `.icns` icon. CI adds `tauri.catercow-ci.conf.json` to build the adaptive icon.
