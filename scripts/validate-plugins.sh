#!/usr/bin/env bash
# Validate every plugins/*/plugin.json: required fields, known kind, and
# (for code plugins) an existing entry point.
set -euo pipefail
shopt -s nullglob

cd "$(dirname "$0")/.."

manifests=(plugins/*/plugin.json)
if [ "${#manifests[@]}" -eq 0 ]; then
	echo "INVALID: no plugins/*/plugin.json found" >&2
	exit 1
fi

fail=0

for manifest in "${manifests[@]}"; do
	dir="$(dirname "$manifest")"
	plugin_ok=1

	# Parse name, kind, main in a single pass; path passed via env var so
	# it is never interpolated into the JS source.
	fields="$(MANIFEST="$manifest" bun -e '
		const fs = require("fs");
		const j = JSON.parse(fs.readFileSync(process.env.MANIFEST, "utf8"));
		console.log([j.name ?? "", j.kind ?? "", j.main ?? ""].join("\t"));
	')"
	IFS=$'\t' read -r name kind main <<<"$fields"

	if [ -z "$name" ]; then
		echo "INVALID $manifest: missing 'name'" >&2
		plugin_ok=0
	fi

	case "$kind" in
		acp|mcp|skill|hook) ;;
		*)
			echo "INVALID $manifest: kind '$kind' not in acp|mcp|skill|hook" >&2
			plugin_ok=0
			;;
	esac

	# acp/mcp plugins must have a resolvable entry point.
	case "$kind" in
		acp|mcp)
			if [ -z "$main" ]; then
				echo "INVALID $manifest: kind '$kind' requires 'main'" >&2
				plugin_ok=0
			elif [ ! -f "$dir/$main" ]; then
				echo "INVALID $manifest: main '$main' not found in $dir" >&2
				plugin_ok=0
			fi
			;;
	esac

	if [ "$plugin_ok" -eq 1 ]; then
		echo "ok: $dir ($kind)"
	else
		fail=1
	fi
done

exit "$fail"
