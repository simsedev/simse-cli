#!/usr/bin/env bash
# Typecheck the plugin SDK and every TypeScript plugin.
set -euo pipefail

cd "$(dirname "$0")/.."

PLUGINS="plugin-sdk plugins/claude plugins/copilot plugins/ollama plugins/github plugins/perplexity"
fail=0

for dir in $PLUGINS; do
	echo "typecheck: $dir"
	if ! (cd "$dir" && bunx tsc -p tsconfig.json); then
		echo "typecheck FAILED: $dir" >&2
		fail=1
	fi
done

exit "$fail"
