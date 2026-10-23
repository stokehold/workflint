#!/usr/bin/env bash
# Usage: summary.sh <directory> <mode>
#
# Prints a Markdown table of the files in <directory>, sorted by name, with
# the size and SHA-256 of each, for the job summary of the release workflow.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: summary.sh <directory> <mode>" >&2
  exit 2
fi
dir="$1"
mode="$2"

echo "## Release assets ($mode)"
echo
echo "| File | Size in bytes | SHA-256 |"
echo "| --- | ---: | --- |"
while IFS= read -r name; do
  size="$(wc -c < "$dir/$name" | tr -d ' ')"
  hash="$(sha256sum "$dir/$name" | cut -d ' ' -f 1)"
  echo "| \`$name\` | $size | \`$hash\` |"
done < <(cd "$dir" && find . -mindepth 1 -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort)
