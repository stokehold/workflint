#!/usr/bin/env bash
# Usage: install-scripts.sh <version> <directory>
#
# Writes the copies of scripts/install.sh and scripts/install.ps1 that a
# release attaches into <directory>. The copies install <version> when no
# version is given. Each copy differs from its source in the one line that
# sets the default version, and fails the script if that line is missing.
#
# The checksums job of .github/workflows/release.yml runs it before it writes
# checksums.txt, so the files that are listed, attested, and uploaded are the
# same bytes.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: install-scripts.sh <version> <directory>" >&2
  exit 2
fi
version="$1"
out="$2"
scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a version such as 0.1.0" >&2
  exit 1
fi

# Usage: substitute <file name> <line in the source> <line in the copy>
substitute() {
  local name="$1" from="$2" to="$3" count
  count="$(grep -cxF -- "$from" "$scripts/$name" || true)"
  if [ "$count" -ne 1 ]; then
    echo "error: scripts/$name has $count lines '$from', not 1" >&2
    exit 1
  fi
  awk -v from="$from" -v to="$to" '$0 == from { print to; next } { print }' \
    "$scripts/$name" > "$out/$name"
}

mkdir -p "$out"
substitute install.sh "default_version=latest" "default_version=$version"
substitute install.ps1 "\$DefaultVersion = 'latest'" "\$DefaultVersion = '$version'"
