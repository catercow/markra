#!/usr/bin/env bash
# Runs CowMark from source against a local marketing site, so schema, component and preview
# changes on either side show up without a deploy. See docs/mdc.md, "Local development".
#
#   scripts/cowmark-dev.sh <content checkout> [site]
#
# <content checkout>: a marketing-content clone, e.g. the monorepo's js/nuxt4/marketing-content,
# opened directly without signing in. Save to studio and Get latest need a sign-in and fail.
# [site]: the local site, default http://localhost:4012. Start it from js/nuxt4 in the monorepo:
#
#   CONTENT_PREVIEW=true NUXT_PUBLIC_DRAFT_PREVIEW_ALLOWED_ORIGINS=http://127.0.0.1:1420 \
#     NODE_ENV=development NUXT_API_URL=http://localhost:3000 nuxt dev --port 4012
set -euo pipefail

folder="${1:?usage: scripts/cowmark-dev.sh <content checkout> [site]}"
folder="$(cd "$folder" && pwd)"
[ -d "$folder/content" ] || { echo "$folder has no content/ folder" >&2; exit 1; }

export VITE_COWMARK_DEV_FOLDER="$folder"
export VITE_COWMARK_DEV_SITE="${2:-http://localhost:4012}"
export VITE_CATERCOW_PREVIEW_SITE="$VITE_COWMARK_DEV_SITE"

# Tauri's beforeDevCommand runs a bare `pnpm`; without one installed, use corepack's.
if ! command -v pnpm >/dev/null; then
  shim="$(mktemp -d)"
  printf '#!/bin/sh\nexec corepack pnpm "$@"\n' > "$shim/pnpm"
  chmod +x "$shim/pnpm"
  export PATH="$shim:$PATH"
fi

cd "$(dirname "$0")/../apps/desktop"
exec pnpm tauri dev --config src-tauri/tauri.cowmark-dev.conf.json
