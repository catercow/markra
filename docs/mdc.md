# MDC components (CaterCow fork)

This fork edits [MDC](https://content.nuxt.com/docs/files/markdown) (Nuxt Content's Markdown Components) for the CaterCow marketing content repository. It is not proposed upstream.

## When it is on

- Always, in a folder that contains `schema/components.json`. The content repository generates that file from the site's components.
- Anywhere else, when **Settings → Editor → MDC components** is on. It is off by default.

## What it does

- **Parsing** (`packages/editor/src/codemirror/mdc-syntax.ts`): `::name{attrs}` blocks, a `---` YAML props block directly after the opener, `#slot` lines, `::` closers, `:name[label]{attrs}` inline components, and `{attrs}` after spans, emphasis, code and links. Openers and closers are separate nodes and the content between them is parsed as ordinary Markdown. `readMdcBlocks` pairs them by colon count.
- **Cards** (`mdc-preview.ts`): each block is drawn as a card, with nested cards indented. The opener shows as a chip with the component's name and an Edit button; the props are YAML-highlighted. Moving the caret onto the opener or the closer shows its source.
- **Props form** (`mdc-edit.ts`, `packages/editor/src/mdc/`): Edit opens a form generated from `schema/components.json`. Each change rewrites only that prop, in the braces or in the YAML block, and leaves the rest of the text byte for byte.
- **Insert** (`mdc-insert.ts`, `packages/editor/src/mdc/components.ts`): with a schema, the slash menu has a Components section, grouped by `group` and searchable by name, component id and purpose. The block "+" button opens the same menu with the components first. Choosing one inserts the schema `example` (or a bare block) as its own block, adds colons when it goes inside another component, and opens the props form. A component with a table `input` gets an empty table with its columns instead, with the caret in the first cell; a gallery (`"kind": "images"`) gets a row of two placeholder images. Props whose `format` is `search-link` or `review-link` show "Paste a CaterCow search URL" (or review URL) as their placeholder.
- **Lint** (`mdc-lint.ts`): diagnostics for unknown components, missing, unknown and mistyped props (list item fields included), and unclosed blocks and props blocks. A card hides its opener line, so problems found there underline the card's label instead.
- **Content lint in CowMark.** For Markdown files inside the managed content folder, CowMark lints with the shared Rust crate `crates/cowmark-lint` instead of `mdc-lint.ts`, so the editor reports exactly what `cowmark check` and CI do: the component rules above plus front matter against `schema/collections.json`, missing images, image descriptions, links to missing pages and leftover conflict markers. Three error rules cover what the site refuses to render (`crates/cowmark-lint/src/safety.rs`): `raw-html` flags every HTML block and inline tag, comment, processing instruction, declaration and CDATA section, found by a CommonMark parser (pulldown-cmark) over the prose only, so autolinks, code, escaped `\<`, a `<` that isn't a tag, component lines, props blocks, front matter and `{…}` attributes are left alone; `unsafe-link` flags Markdown links and images, and URL props and attributes (`href`, `src`, `srcset`, `to`, `url`, `link`, `action`, `poster` and names ending in them), whose scheme isn't `http`, `https`, `mailto` or `tel` (relative URLs are fine; the scheme is read as a browser reads it, ignoring case, tabs and line breaks); `unsafe-attribute` flags `on*`, `@*`, `v-on`, `v-bind`, `v-model`, `v-html`, `srcdoc` and `formaction` in component braces, YAML props and inline `{…}` attributes. The `cowmark_lint` command (`apps/desktop/src-tauri/src/content_lint.rs`) keeps one linter per folder and rebuilds it when either schema file changes. The app passes the folder-relative path and the message templates (`editor.mdcLint.*` and `editor.contentLint.*`); the diagnostic's source is the rule id. Other files, stock Markra and builds without the runtime command (`files.contentLint`) use `mdc-lint.ts`, as does a failed native call.

Cards and forms never change text by themselves. Outside CowMark content pages the file is saved exactly as typed.

### Component input: lenient when typed, canonical when stored

Some components take their content as a Markdown table or as images, and some props take a CaterCow URL. The schema says which, and cowmark-lint (`crates/cowmark-lint/src/formats.rs`, `input.rs`) reads what people type leniently and writes it back in one canonical form, so the site can be strict. The schema shape is a contract with the site, which generates it:

```json
"input": { "kind": "table", "columns": [
  { "name": "Item", "aliases": ["Name", "Dish"], "required": true, "format": "text", "markdown": true },
  { "name": "Price", "aliases": ["Cost"], "required": true, "format": "price" } ] }
"input": { "kind": "images" }
"props": [{ "name": "link", "type": "string", "format": "search-link", "searchParams": { "diet": { "values": ["vegan", "…"] }, "q": { "values": null } } }]
```

Unknown keys and odd types are ignored rather than rejected, so an older or newer schema never stops the linter.

| Format | Read leniently | Stored as | Error when |
| --- | --- | --- | --- |
| table headers | Any alias, any case, any order; extra spaces; no outer pipes | Canonical names in schema order; `| a | b |` rows with one space around each cell and a `| --- |` row, no padding | Unknown, duplicate or unnamed column; a required column missing; no `| --- |` row of the header's width; a row with more cells than headers; no rows |
| `price` | `7.5`, `7.5 USD`, `US$ 9`, `1200.5`, `$1,200` | `$7.50`, `$9.00`, `$1,200.50` | Ranges (`7-9`), approximations (`~$13`, `13+`), decimal commas (`7,25`), more than two decimals, other currencies, words (`TBD`) |
| `code` | Lower case, spaces, backticks | `SAVE10` (A–Z, 0–9, `-`, `_`) | Other characters |
| `link` | `https://www.catercow.com/x`, `<…>` | `/x` for CaterCow addresses, else the http(s) address | Other schemes or text that isn't an address |
| `search-link` | Full CaterCow URLs, any parameter order, `name[]=` or repeated names, any value case | `/search?…` with parameters in schema order, values matched to the schema's case, percent-encoded | A path other than `/search`; a parameter the schema doesn't list; a value not in its list (with a did-you-mean suggestion) |
| `review-link` | Full URLs, query strings, fragments, trailing slashes | `/catering/<id>-slug` or `/packages/<id>-slug` | Anything else |
| gallery | Images on separate lines or one line | Each row on one line, separated by a blank line | Text or anything but images in the slot; no images |

Something that can't be read is an error (`component-table`, `cell-format`, `link-format`, `gallery-images`). Something understood but not canonical is also an error, and its diagnostic carries a fix (`fix: { from, to, fromUtf16, toUtf16, insert }`). Layout alone is never reported, so tables edited elsewhere don't have to line up: cell spacing, dash counts, alignment colons, outer pipes, empty rows and empty unnamed columns, and gallery images split over several lines. `cowmark_lint::format(path, text, &schemas)` applies every fix and also tidies that layout, repeatedly until nothing changes, and returns the problems left. Formatted tables aren't padded, so changing one cell changes one line. A table with a structural error (unknown, duplicate or unnamed column, extra cells) is never rewritten, so formatting never drops text.

Where formatting runs:

- **CowMark saves.** Saving a managed content page (`content/**/*.md`) formats it first, through the `cowmark_format` command, as one undoable change (`applyCodeMirrorMarkdownEdit`). A failed or unavailable format saves the text as typed; Save to studio still refuses pages with errors. Pages saved from the source view or side documents aren't formatted.
- **The editor.** Each fixable diagnostic has a **Fix** button (`editor.lintFix`) that applies just that fix.
- **CSV.** Pasting CSV or spreadsheet cells (`mdc-csv.ts`), or dropping a `.csv` file, on a content page converts it to a table with `cowmark_csv_table` (`crates/cowmark-core/src/csv_table.rs`, the `csv` crate). Inside a table component the table takes that component's canonical headers and values, and replaces the empty skeleton table. Text that only looks like CSV is pasted as before.
- **The CLI and MCP**, below.

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
- **The preview panel footer** lists changed files with a checkbox each (none ticked by default; ticking `content/x/<slug>.md` also ticks the changed files in its image folder `public/x/<slug>/`, but never `public/blog/shared/`) and has a message box, **Save to studio** (saves open documents, commits only the ticked files, rebases on `origin/studio` carrying the rest along, pushes; unticked changes stay on the computer as drafts) and **Get latest** (fetch and rebase, keeping uncommitted edits). If someone else changed the same lines, see **Conflicts** below.
- **Conflicts.** When the rebase (or re-applying uncommitted edits) can't combine the changes, `catch_up` in `crates/cowmark-core/src/git.rs` moves the folder to the latest `origin/studio` and replays every local change on top as uncommitted edits: the editor's commit, drafts and untracked files. Overlapping lines are written into the file between `<<<<<<< yours`, `=======` and `>>>>>>> theirs` (`merge.rs`). A file deleted on one side and edited on the other keeps the edit; a binary file changed on both sides keeps yours (theirs is in the commit). The whole folder is first recorded under `refs/cowmark/backup` (one reflog entry each time). The error is `conflict: <message>` followed by `\nconflict-files: <JSON list>`; the footer opens those files and says to resolve the highlighted conflicts, then save again. Save to studio refuses ticked files that still contain markers. It also refuses ticked pages under `content/` with lint errors (warnings don't block): `save` in `git.rs` lints them with the folder's schemas (`content_check.rs`) before committing, so neither the app nor the `cowmark` command nor an agent can publish them. The error is `lint: <message, one line per file>` followed by `\nlint-problems: <JSON list of { file, line, message }>`, the first error in each file; the footer lists them and opens those files. When only a draft overlaps, the ticked commit is kept but not sent until the next save.
- **Changes from outside the app.** A clean tab reloads when its file changes on disk. A tab with unsaved edits is merged three ways against the content it last read or wrote (`content_merge_text`): changes to different lines are applied as one undoable edit and a notice says so; overlapping ones get the markers above. Either way the tab stays unsaved.
- **Conflict bars** (`packages/editor/src/codemirror/conflicts.ts`): each marker block shows your lines and their lines, highlighted, with **Keep mine**, **Use theirs** and **Keep both** above it; a choice rewrites only that block. A bar above the document counts the conflicts left and jumps to the next. Marker lines are parsed as their own blocks (`conflict-syntax.ts`), so `=======` doesn't turn your last line into a heading. Stock Markra builds show markers as plain text and ignore disk changes to unsaved tabs, as before.
- **Expired session (401):** the session is cleared and the footer asks to sign in again; edits are kept. **Removed from the allowlist (403):** "This email can no longer save CaterCow content. Ask a developer."
- **Tokens** are requested only to download, save or get latest. They are short-lived GitHub tokens from the preview site, held in memory, and reach git only through `GIT_CONFIG_COUNT` environment variables as an `http.https://github.com/.extraheader` (with `credential.helper` emptied). They never appear in command arguments, URLs, `.git/config` or logs. This needs git 2.31 or later.
- **Code:** `crates/cowmark-core` (`access.rs`: sign-in, session, tokens; `git.rs`: clone, status, save, update, conflicts; `merge.rs`: three-way merge; `preview_server.rs` and `control.rs`: see CowMark as a hub), wrapped as Tauri commands in `apps/desktop/src-tauri/src/content_access.rs`; `packages/app/src/components/CowMarkGate.tsx` and `ContentSaveBar.tsx`. The server side is in the CaterCow monorepo.

## CowMark as a hub

CowMark can stay in front while people edit in another app (VS Code, Obsidian, Claude Desktop). Everything runs inside the app: there is no background process, and quitting CowMark stops the browser preview and the control connection.

- **Companion view** (the title bar button next to the globe, remembered per user) hides the editor, file tree and AI panel. The preview and the save bar fill the window, sized for a narrow window beside another editor. The preview follows the most recently saved file: when one poll finds several saved pages, only the last navigates. Click the button again for the normal layout.
- **Browser preview.** While the managed folder is open, CowMark serves `http://localhost:4100` on 127.0.0.1 only (`crates/cowmark-core/src/preview_server.rs`). The page frames the site at the same path and relays the same draft messages as the panel: pending changes once the site is ready, then saved files over server-sent events, and images. Unsaved typing in CowMark's editor is not sent; saved files are. Every request needs the per-launch token in `?token=`, and the Host header must be `localhost:4100` or `127.0.0.1:4100`. **Open in browser** in the preview panel opens the current page there. The port is fixed because the site's allowlist names it exactly: if another program holds it, the panel says so and CowMark doesn't try another port.
- **The `cowmark` command** (below) checks content, and talks to the running app over the control connection, opening the app first if it isn't running. `cowmark mcp` offers the same to Claude Desktop and other agents. Publishing from it is a proposal: it ticks files and fills in the message, and a person clicks Save to studio. Neither ever sees git credentials.

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
| `get_latest` | none | `{ started: true }`; opens the preview panel and runs its Get latest, as if clicked (open documents are saved first). Returns once started; the outcome shows in the save bar and in `status` |

| Error code | Meaning |
| --- | --- |
| -32700 / -32600 / -32601 / -32602 | Not JSON / not a request / unknown method / bad params |
| -32001 | Missing or wrong token |
| -32002 | No content folder yet |
| -32003 | Path outside the content folder |
| -32004 | A proposed path has no changes |
| -32005 | The app couldn't do it (git failed, no window) |

`control::Client` is the client the `cowmark` command uses. `crates/cowmark-core/tests/control_client.rs` calls every method; `tests/preview_server.rs` covers the browser preview.

### The `cowmark` command

`crates/cowmark-cli`, a binary named `cowmark`. Release builds ship it inside the app as a Tauri sidecar: `CowMark.app/Contents/MacOS/cowmark` on macOS and `cowmark.exe` beside `CowMark.exe` on Windows, so editors install nothing else.

- **Which folder.** `--root <folder>` (or `COWMARK_ROOT`), else the content checkout around the working directory (a folder with `content/` and `schema/collections.json`), else the running app's folder, else the folder CowMark remembers in `content-access.json`. Every path is checked with `control::resolve_in_folder`: nothing outside the folder, through `..` or a symlink, or in `.git`. Commands that change files touch only Markdown and images under `content/`, never through a symlink.
- **Which app.** The app data dir is `~/Library/Application Support/com.catercow.cowmark` (macOS) or `%APPDATA%\com.catercow.cowmark` (Windows); `COWMARK_DATA_DIR` overrides it, for a CowMark started with `--user-data-dir`. When the app isn't answering, commands that need it start it (the app around the sidecar, else `open -b com.catercow.cowmark`, else `%LOCALAPPDATA%\CowMark\CowMark.exe`) and wait up to 45 seconds for `ping`; the control connection only opens once the managed folder does, so a CowMark still on its sign-in screen times out with a message. With `COWMARK_DATA_DIR` set it never starts the app.

| Command | Needs the app | What it does |
| --- | --- | --- |
| `cowmark check [paths…] [--json] [--strict] [--github]` | no | Lints Markdown files (default: all of `content/`) with cowmark-lint, as the editor does. Prints `file:line:col: severity [rule] message`. Exit 0 when clean or only warnings, 1 on errors (or warnings with `--strict`), 2 when it couldn't run. `--github` adds GitHub Actions annotations for errors |
| `cowmark check --fix [paths…]` | no | Formats the files first (as `fmt`), then checks them |
| `cowmark fmt [paths…] [--check]` | no | Writes component input in its canonical form and lists the files it changed. With `--check` it changes nothing and exits 1 when a file would change |
| `cowmark table [file] [--component <name>]` | no | Prints a CSV file (or stdin; tab-separated when the first line has a tab) as a Markdown table. With `--component` the table is wrapped in that table component and formatted, and problems are printed to stderr |
| `cowmark status [--json]` | no | Changed files, branch and commits waiting to be sent: from the app when it runs on this folder, else from git |
| `cowmark open <path>` | yes | Opens a page in the editor and preview |
| `cowmark propose <paths…> -m <message>` | yes | `propose_publish`: ticks the files and fills in the message. Doesn't publish |
| `cowmark preview-url [path]` | yes | The browser preview's address with its token, at the page's URL (from its collection's `url` pattern) when given |
| `cowmark focus` / `cowmark get-latest` | yes | `focus` / `get_latest` |
| `cowmark mcp` | when a tool needs it | MCP server on stdin/stdout |
| `cowmark mcp-config` | no | Prints the Claude Desktop config entry for this binary |

Paths on the command line are relative to the working directory when they exist there, else to the content folder.

### MCP server (Claude Desktop)

`cowmark mcp` serves MCP over stdio with the official Rust SDK (`rmcp`). It finds the folder again on every tool call, so it follows CowMark being opened, closed or set up. Its instructions tell the agent the workflow: read the schemas, edit, run `check`, then `propose_publish` and ask the person to review and click Save to studio in CowMark.

| Tool | Hints | |
| --- | --- | --- |
| `list_files`, `read_file` | read-only | Any file in the folder but `.git`; text up to 2 MB |
| `write_file`, `rename_file`, `delete_file` | destructive | Markdown under `content/` only; rename and delete also take images under `content/` and `public/`. `write_file` formats component input (as `cowmark fmt`), writes through a temporary file and returns the check result, saying when it reformatted. Writing a file open with unsaved edits in CowMark is fine: the tab merges it |
| `list_components`, `get_component_schema`, `get_collection_schema` | read-only | From `schema/components.json` and `schema/collections.json`, with notes on dates, lengths and images |
| `new_page` | creates only | `<collection folder>/<slug>.md` with the title, the required fields (dates today, the rest empty unless given) and an optional body; lists the fields left to fill |
| `add_image` | creates only | Copies a local file or base64 data (PNG, JPEG, GIF, WebP, AVIF, SVG, up to 25 MB, checked by content) to the page's image folder `public/<collection>/<slug>/<name>`, named like the editor names imports, and returns the site path (`/<collection>/<slug>/<name>`) and a Markdown snippet. Not resized, unlike the editor's import |
| `format_file` | not destructive | Formats one Markdown file under `content/` in place, as `cowmark fmt`, and returns what's left to fix |
| `csv_to_table` | read-only | CSV text to a Markdown table, optionally for a table component, as `cowmark table` |
| `check`, `status`, `preview_link` | read-only | As the commands |
| `get_latest`, `open_in_cowmark` | not destructive | As the commands |
| `propose_publish` | not destructive | Ticks files and fills in the message; says nothing was published and a person must click Save to studio |

To add it to Claude Desktop, run the bundled binary's `mcp-config` and paste the `cowmark` entry under `mcpServers` in Claude Desktop's config (Settings → Developer → Edit Config), then restart Claude Desktop:

```sh
/Applications/CowMark.app/Contents/MacOS/cowmark mcp-config
# Windows: "%LOCALAPPDATA%\CowMark\cowmark.exe" mcp-config
```

```json
{ "mcpServers": { "cowmark": { "command": "/Applications/CowMark.app/Contents/MacOS/cowmark", "args": ["mcp"] } } }
```

The path changes if CowMark is installed elsewhere; run `mcp-config` again after moving it.

### Checking content in CI

Each release also carries `cowmark-linux-x64.tar.gz`, built by the `cli-linux` job in `catercow-desktop.yml`. The content repository's workflow downloads it from the release (this repository is public, so no token is needed) and runs `cowmark check --root . --github` on pushes to `studio` and `main` and on pull requests to `main`. The release's `latest` checker is used unless the workflow pins a tag.

## CowMark: images and page details

- **Images live in `public/`.** For `content/blog/my-post.md`, pasted, dropped or chosen images (including the Page details image picker) are saved in `public/blog/my-post/` and written as the site path `/blog/my-post/photo.jpg`. The folder is the collection's URL prefix from `schema/collections.json` plus the file name without `.md` (`content/cuisines/thai.md` → `public/cuisines/thai/`); `public/blog/shared/` holds images several posts reuse. This replaces the clipboard image folder setting for pages under `content/` only; other files, and stock Markra, keep the setting. The editor, the Page details preview and the side document show `/…` images in a managed page from `<folder>/public/…` (`packages/app/src/hooks/useCowMarkPages.ts`). The site doesn't serve images stored under `content/`, so `missing-image` reports an image written relative to a page and suggests the site path; site paths are checked against `public/`.
- **On import** the name becomes lowercase and hyphenated (a generic name like `image.png` becomes `<page>-image`), a clash gets `-2`, `-3`… Photos larger than 2400 px on the longest side are scaled down and re-encoded as JPEG or WebP; images with transparency stay PNG; SVG and GIF are copied as they are. The editor then asks for alt text; **Skip** inserts the image with empty alt text. Code: `packages/app/src/lib/cowmark-images.ts`.
- **Site preview** sends new and changed images under `public/` as media, so a pasted image shows before it is saved to studio.
- **Page details.** In a folder listed in `schema/collections.json`, the front matter header has a **Page details** button. The form is generated from the collection's fields in schema order: text with character counts for titles and descriptions, a list of values other pages in the folder already use, switches, numbers, a date picker that writes `YYYY-MM-DD`, and an image picker that stores the image in the page's image folder under `public/`. Fields the schema doesn't describe, and lists or objects, are edited as YAML under **Advanced**. Each change rewrites only that key and leaves the rest of the file byte for byte (`packages/editor/src/mdc/frontmatter-fields.ts`, `packages/app/src/components/PageDetailsPanel.tsx`).
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
- **Local build** (needs Rust 1.85+ and Node 22.18+): `pnpm --filter @markra/desktop tauri build --config src-tauri/tauri.catercow.conf.json --bundles dmg`. Its `beforeBuildCommand` also builds the `cowmark` sidecar into `src-tauri/binaries/cowmark-<target triple>` (`apps/desktop/scripts/cowmark-cli.mjs`; `pnpm build:cowmark-cli` runs it alone). Plain `cargo build` and `pnpm tauri dev` use the stock config and don't need it. Without Xcode 26 this uses the classic `.icns` icon. CI adds `tauri.catercow-ci.conf.json` to build the adaptive icon.
