#!/usr/bin/env bash
# Usage: check-wheels.sh <directory> <recorded>
#
# Checks the PyPI wheels in <directory> before anything publishes them:
#
# - The directory holds one wheel for each of the six targets of ADR 0014,
#   with the platform tags of docs/releasing.md, and nothing else.
# - <recorded> is in the format of sha256sum and lists each wheel once, with
#   the SHA-256 that its build job recorded, and every checksum matches.
#
# Needs sha256sum, as on the Ubuntu runners.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: check-wheels.sh <directory> <recorded>" >&2
  exit 2
fi
dir="$1"
recorded="$2"
tags=(
  manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_1_x86_64
  manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64
  macosx_10_12_x86_64
  macosx_11_0_arm64
  win_amd64
  win_arm64
)
failed=0
fail() {
  echo "error: $*" >&2
  failed=1
}

files="$(cd "$dir" && find . -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort)"
for tag in "${tags[@]}"; do
  matches="$(grep -c -e "^workflint-[^-]*-py3-none-${tag//./\\.}\.whl\$" <<< "$files" || true)"
  [ "$matches" -eq 1 ] || fail "the directory has $matches wheels with the tag $tag, not 1"
done
count="$(grep -c . <<< "$files" || true)"
[ "$count" -eq "${#tags[@]}" ] || fail "the directory has $count files, not the ${#tags[@]} wheels"

listed="$(awk '{ print $2 }' "$recorded" | LC_ALL=C sort)"
if [ "$listed" != "$files" ]; then
  fail "$recorded does not list every wheel once"
  diff <(echo "$files") <(echo "$listed") >&2 || true
fi
(cd "$dir" && sha256sum -c -) < "$recorded" || fail "a wheel does not match the checksum its build job recorded"

if [ "$failed" -ne 0 ]; then
  exit 1
fi
echo "The wheels are complete."
