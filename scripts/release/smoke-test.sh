#!/usr/bin/env bash
# Usage: smoke-test.sh <binary> <version>
#
# Checks that a workflint binary from a release channel starts: `-version`
# exits with 0 and prints <version> on its first line. Every channel runs this
# script on the binary it installs (docs/releasing.md).
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: smoke-test.sh <binary> <version>" >&2
  exit 2
fi
binary="$1"
version="$2"

if ! output="$("$binary" -version)"; then
  echo "error: $binary -version failed" >&2
  exit 1
fi
printf '%s\n' "$output"

first="$(printf '%s\n' "$output" | head -n 1 | tr -d '\r')"
if [ "$first" != "$version" ]; then
  echo "error: the first line of $binary -version is '$first', not '$version'" >&2
  exit 1
fi
echo "smoke test passed: $binary is version $version"
