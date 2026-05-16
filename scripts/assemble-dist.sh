#!/usr/bin/env bash
# Assemble the release dist tree and archive for one target.
#
# Usage: assemble-dist.sh <cargo-target-triple> <platform-label>
#   platform-label is one of: linux-x86_64 linux-aarch64
#                             darwin-x86_64 darwin-aarch64 windows-x86_64
#
# Expects the core checkout at ./core and a finished build at
# core/target/<triple>/release/simse[.exe].
# Produces ./simse-<platform-label>.{tar.gz,zip} at the repo root.
set -euo pipefail

TARGET="$1"
PLATFORM="$2"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

EXT=""
case "$PLATFORM" in
	windows-*) EXT=".exe" ;;
esac

BIN="core/target/$TARGET/release/simse$EXT"
if [ ! -f "$BIN" ]; then
	echo "assemble-dist: binary not found at $BIN" >&2
	exit 1
fi

DIST="dist"
rm -rf "$DIST" "simse-$PLATFORM.tar.gz" "simse-$PLATFORM.zip"
mkdir -p "$DIST/bin" "$DIST/share/simse"

cp "$BIN" "$DIST/bin/simse$EXT"
cp -R plugins "$DIST/share/simse/plugins"
cp LICENSE "$DIST/LICENSE"
cp README.md "$DIST/README.md"

# Drop dev-only artifacts from the bundled plugins.
find "$DIST/share/simse/plugins" -name node_modules -type d -prune -exec rm -rf {} +
find "$DIST/share/simse/plugins" \
	\( -name 'tsconfig.json' -o -name 'package.json' \) -delete

case "$PLATFORM" in
	windows-*)
		( cd "$DIST" && 7z a -tzip "../simse-$PLATFORM.zip" ./* >/dev/null )
		echo "created simse-$PLATFORM.zip"
		;;
	*)
		tar -czf "simse-$PLATFORM.tar.gz" -C "$DIST" .
		echo "created simse-$PLATFORM.tar.gz"
		;;
esac
