#!/bin/sh
# Compile all TypeScript plugins to JavaScript using esbuild.
# Usage: ./scripts/build-plugins.sh
#
# Requires: npx (Node.js) with esbuild available.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGINS_DIR="$(cd "$SCRIPT_DIR/../plugins" && pwd)"

count=0

for dir in "$PLUGINS_DIR"/*/; do
    ts="$dir/index.ts"
    js="$dir/index.js"

    if [ ! -f "$ts" ]; then
        continue
    fi

    name="$(basename "$dir")"
    echo "compiling $name/index.ts -> index.js"
    npx --yes esbuild "$ts" --outfile="$js" --format=iife --platform=neutral --target=esnext
    count=$((count + 1))
done

echo "compiled $count plugin(s)"
