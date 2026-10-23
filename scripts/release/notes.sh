#!/usr/bin/env bash
# Usage: notes.sh <version> <changelog>
#
# Prints the section of <changelog> for <version>, without its heading, as the
# notes of the GitHub release. The heading is `## [<version>]`, optionally
# followed by a link and a date as release-plz writes it, or `## <version>`.
# The section ends at the next `## ` heading. Prints nothing when the file or
# the section does not exist, and the release job then asks GitHub to
# generate the notes.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: notes.sh <version> <changelog>" >&2
  exit 2
fi
version="$1"
changelog="$2"

[ -f "$changelog" ] || exit 0

awk -v version="$version" '
  # Compares strings, not patterns, because a version contains dots.
  function heading_version(line,    rest) {
    rest = substr(line, 4)
    sub(/[ \t].*$/, "", rest)
    if (substr(rest, 1, 1) == "[") {
      rest = substr(rest, 2)
      sub(/\].*$/, "", rest)
    }
    sub(/^v/, "", rest)
    return rest
  }
  /^## / {
    if (inside) exit
    if (heading_version($0) == version) inside = 1
    next
  }
  inside {
    lines[++count] = $0
  }
  END {
    first = 1
    while (first <= count && lines[first] ~ /^[ \t]*$/) first++
    last = count
    while (last >= first && lines[last] ~ /^[ \t]*$/) last--
    for (i = first; i <= last; i++) print lines[i]
  }
' "$changelog"
