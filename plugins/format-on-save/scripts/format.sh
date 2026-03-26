#!/bin/sh
# Auto-format based on file extension.
# Called after fs_write/fs_edit with the modified file path in $1.

FILE="$1"
[ -z "$FILE" ] && exit 0

case "$FILE" in
    *.rs)     rustfmt "$FILE" 2>/dev/null ;;
    *.ts|*.tsx|*.js|*.jsx) npx biome format --write "$FILE" 2>/dev/null ;;
    *.py)     python3 -m black "$FILE" 2>/dev/null ;;
    *.go)     gofmt -w "$FILE" 2>/dev/null ;;
    *.json)   python3 -m json.tool "$FILE" > "$FILE.tmp" && mv "$FILE.tmp" "$FILE" 2>/dev/null ;;
esac
