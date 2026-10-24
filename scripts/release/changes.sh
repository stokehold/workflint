#!/usr/bin/env bash
# Usage: changes.sh <paths file> < <changed files>
#
# Reads one changed file per line from standard input and prints
# `packaging=true` when one of them matches a line of <paths file>, and
# `packaging=false` otherwise, in the format of $GITHUB_OUTPUT. Lines of
# <paths file> that are empty or start with # are skipped. Each other line is
# a bash pattern, in which * also matches /. Prints the matching files to
# standard error.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: changes.sh <paths file> < <changed files>" >&2
  exit 2
fi

patterns=()
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in
    '' | '#'*) ;;
    *) patterns+=("$line") ;;
  esac
done < "$1"
if [ "${#patterns[@]}" -eq 0 ]; then
  echo "error: $1 lists no paths" >&2
  exit 1
fi

packaging=false
while IFS= read -r file || [ -n "$file" ]; do
  for pattern in "${patterns[@]}"; do
    # shellcheck disable=SC2053 # The pattern is a glob on purpose.
    if [[ "$file" == $pattern ]]; then
      echo "$file matches $pattern" >&2
      packaging=true
      break
    fi
  done
done

echo "packaging=$packaging"
