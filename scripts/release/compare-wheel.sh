#!/usr/bin/env bash
# Usage: compare-wheel.sh <wheel> <archive>
#
# Checks a PyPI wheel against the release archive of the same target, so that
# PyPI ships the binary of the release (ADR 0014):
#
# - The wheel holds the binary in <name>-<version>.data/scripts/, the
#   METADATA, WHEEL, and RECORD files, LICENSE and NOTICE, and the SBOM that
#   maturin writes, and nothing else.
# - The binary in the wheel is byte-identical to the binary in <archive>.
#
# Unpacks .whl and .zip files with Python's zipfile module, so it runs on
# every runner of the build job.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: compare-wheel.sh <wheel> <archive>" >&2
  exit 2
fi
wheel="$1"
archive="$2"

# The python3 of a Windows runner can be a stub that opens the Microsoft
# Store.
python=python3
if ! python3 -c '' 2> /dev/null; then
  python=python
fi
sha256() {
  if command -v sha256sum > /dev/null; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir "$work/wheel" "$work/archive"

case "$archive" in
  *.zip)
    binary=workflint.exe
    "$python" -m zipfile -e "$archive" "$work/archive"
    ;;
  *.tar.gz)
    binary=workflint
    tar -xzf "$archive" -C "$work/archive"
    ;;
  *)
    echo "error: $archive is neither a .tar.gz nor a .zip file" >&2
    exit 2
    ;;
esac
"$python" -m zipfile -e "$wheel" "$work/wheel"

# Replaces <name>-<version> with a placeholder, so the list does not depend
# on the version.
entries="$(cd "$work/wheel" && find . -type f | sed -e 's|^\./||' -e 's|^workflint-[^/]*\.data/|DATA/|' \
  -e 's|^workflint-[^/]*\.dist-info/|DIST-INFO/|' | LC_ALL=C sort)"
want="$(printf '%s\n' "DATA/scripts/$binary" DIST-INFO/METADATA DIST-INFO/RECORD DIST-INFO/WHEEL \
  DIST-INFO/licenses/LICENSE DIST-INFO/licenses/NOTICE DIST-INFO/sboms/workflint.cyclonedx.json | LC_ALL=C sort)"
if [ "$entries" != "$want" ]; then
  echo "error: $wheel does not hold the expected files" >&2
  diff <(echo "$want") <(echo "$entries") >&2 || true
  exit 1
fi

in_wheel="$(sha256 "$work"/wheel/workflint-*.data/scripts/"$binary")"
in_archive="$(sha256 "$work/archive/$binary")"
echo "$binary in $(basename "$wheel"): $in_wheel"
echo "$binary in $(basename "$archive"): $in_archive"
if [ "$in_wheel" != "$in_archive" ]; then
  echo "error: the binary in $wheel differs from the binary in $archive" >&2
  exit 1
fi
echo "The wheel holds the binary of the archive."
