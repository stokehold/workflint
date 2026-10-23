#!/usr/bin/env bash
# Usage: package.sh <binary> <archive>
#
# Packs <binary>, README.md, LICENSE, and NOTICE from the current directory
# into <archive>, a .tar.gz or a .zip file, with every file at the root of the
# archive and no directory around them, as in actionlint's archives
# (ADR 0014). The .zip format needs 7-Zip, as on the Windows runners.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: package.sh <binary> <archive>" >&2
  exit 2
fi
binary="$1"
archive="$2"
name="$(basename "$binary")"
files=("$name" README.md LICENSE NOTICE)

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp "$binary" "$stage/$name"
chmod 755 "$stage/$name"
cp README.md LICENSE NOTICE "$stage/"

file_name="$(basename "$archive")"
case "$file_name" in
  *.tar.gz)
    # Keeps the tar of macOS from adding AppleDouble files for extended
    # attributes.
    COPYFILE_DISABLE=1 tar -czf "$stage/$file_name" -C "$stage" "${files[@]}"
    ;;
  *.zip)
    # The paths stay relative, so 7-Zip, a Windows program, need not read the
    # Unix-style paths of the shell.
    (cd "$stage" && 7z a -tzip -bd -bso0 "$file_name" "${files[@]}")
    ;;
  *)
    echo "error: $archive is neither a .tar.gz nor a .zip file" >&2
    exit 2
    ;;
esac
mv "$stage/$file_name" "$archive"
echo "packed $archive"
