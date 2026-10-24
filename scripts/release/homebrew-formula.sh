#!/usr/bin/env bash
# Usage: homebrew-formula.sh <version> <checksums.txt> <base URL>
#
# Prints the Homebrew formula of <version>, written from
# packaging/homebrew/workflint.rb.tmpl. Each archive URL is
# <base URL>/<archive>, and each SHA-256 comes from the archive's line in
# <checksums.txt>. A release passes the download URL of its tag, and a dry
# run passes a file:// URL of the directory that holds this run's archives.
#
# Fails unless <checksums.txt> has exactly one line for each of the four
# macOS and Linux archives, with a SHA-256 of 64 lowercase hex digits.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: homebrew-formula.sh <version> <checksums.txt> <base URL>" >&2
  exit 2
fi
version="$1"
checksums="$2"
base_url="$3"
template="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/packaging/homebrew/workflint.rb.tmpl"

# The values go into sed replacements and Ruby strings, so each one is
# limited to characters that mean nothing to either.
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "error: '$version' is not a version" >&2
  exit 1
fi
if ! [[ "$base_url" =~ ^(https://|file:///)[A-Za-z0-9._~/:%+-]*[A-Za-z0-9._~:%+-]$ ]]; then
  echo "error: '$base_url' is not an https:// or file:// URL without a trailing slash" >&2
  exit 1
fi

script=(-e "s|@VERSION@|$version|g" -e "s|@BASE_URL@|$base_url|g")
for platform in darwin_arm64 darwin_amd64 linux_arm64 linux_amd64; do
  archive="workflint_${version}_$platform.tar.gz"
  lines="$(grep -c -F "  $archive" "$checksums" || true)"
  if [ "$lines" -ne 1 ]; then
    echo "error: $checksums has $lines lines for $archive, not 1" >&2
    exit 1
  fi
  read -r sha256 name < <(grep -F "  $archive" "$checksums")
  if [ "$name" != "$archive" ] || ! [[ "$sha256" =~ ^[0-9a-f]{64}$ ]]; then
    echo "error: the line of $archive in $checksums is not '<SHA-256>  $archive'" >&2
    exit 1
  fi
  placeholder="$(printf '%s' "$platform" | tr '[:lower:]' '[:upper:]')"
  script+=(-e "s|@SHA256_$placeholder@|$sha256|g")
done

sed "${script[@]}" "$template"
