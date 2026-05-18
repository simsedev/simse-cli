#!/usr/bin/env bash
# Typecheck the plugin SDK and every TypeScript plugin.
set -euo pipefail
shopt -s nullglob

cd "$(dirname "$0")/.."

# The SDK plus every plugin that ships a tsconfig.json (discovered, so a new
# TypeScript plugin is covered without editing this list).
DIRS=("plugin-sdk")
for tsconfig in plugins/*/tsconfig.json; do
	DIRS+=("$(dirname "$tsconfig")")
done

fail=0

for dir in "${DIRS[@]}"; do
	echo "typecheck: $dir"
	if ! (cd "$dir" && bunx tsc -p tsconfig.json); then
		echo "typecheck FAILED: $dir" >&2
		fail=1
	fi
done

exit "$fail"
