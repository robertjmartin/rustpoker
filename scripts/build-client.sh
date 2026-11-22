#!/usr/bin/env bash
# Build the Solid client into one self-contained HTML file and copy it into the
# server crate, where include_str! embeds it. Run from anywhere:
#
#   ./scripts/build-client.sh
#
# The generated file is checked in so `cargo build` works without Node.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
client="$root/client"
target="$root/server/pokerserver/static/client.html"

cd "$client"
if [ ! -d node_modules ]; then
  npm ci --no-audit --no-fund
fi
npm run build

built="$client/dist/index.html"
if [ ! -f "$built" ]; then
  echo "error: $built was not produced" >&2
  exit 1
fi

# A single-file build must carry its JS and CSS inline.
if grep -Eq '<script[^>]+src=|<link[^>]+rel="stylesheet"' "$built"; then
  echo "error: $built still references external assets; is vite-plugin-singlefile active?" >&2
  exit 1
fi

cp "$built" "$target"
printf 'embedded client: %s (%s bytes)\n' "${target#"$root"/}" "$(wc -c <"$target")"
