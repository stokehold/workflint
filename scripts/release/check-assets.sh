#!/usr/bin/env bash
# Usage: check-assets.sh <directory> <version>
#
# Checks the release assets in <directory> before anything attests or
# publishes them:
#
# - The directory holds exactly the six archives of ADR 0014, checksums.txt,
#   the SBOM, install.sh, and install.ps1.
# - Each archive holds the binary, README.md, LICENSE, and NOTICE at its root
#   and nothing else, and the binary in each .tar.gz archive is executable.
# - The Linux binaries are statically linked.
# - checksums.txt lists every other asset, sorted by file name, and every
#   checksum matches.
# - The SBOM is a CycloneDX JSON document.
# - install.sh and install.ps1 install <version> by default.
#
# Needs tar, unzip, file, jq, and sha256sum, as on the Ubuntu runners.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: check-assets.sh <directory> <version>" >&2
  exit 2
fi
dir="$1"
version="$2"
sbom="workflint_${version}.cdx.json"
archives=(
  "workflint_${version}_darwin_amd64.tar.gz"
  "workflint_${version}_darwin_arm64.tar.gz"
  "workflint_${version}_linux_amd64.tar.gz"
  "workflint_${version}_linux_arm64.tar.gz"
  "workflint_${version}_windows_amd64.zip"
  "workflint_${version}_windows_arm64.zip"
)
failed=0
fail() {
  echo "error: $*" >&2
  failed=1
}

scripts=(install.ps1 install.sh)
expected="$(printf '%s\n' "${archives[@]}" "$sbom" "${scripts[@]}" checksums.txt | LC_ALL=C sort)"
actual="$(cd "$dir" && find . -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort)"
if [ "$actual" != "$expected" ]; then
  fail "the assets are not the expected ones"
  diff <(echo "$expected") <(echo "$actual") >&2 || true
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for archive in "${archives[@]}"; do
  path="$dir/$archive"
  [ -f "$path" ] || continue
  case "$archive" in
    *.zip)
      binary=workflint.exe
      entries="$(unzip -Z1 "$path" | LC_ALL=C sort)"
      ;;
    *)
      binary=workflint
      entries="$(tar -tzf "$path" | LC_ALL=C sort)"
      mode="$(tar -tvzf "$path" "$binary" | cut -c1-10)"
      case "$mode" in
        -rwx*) ;;
        *) fail "$binary in $archive is not executable: $mode" ;;
      esac
      ;;
  esac
  want="$(printf '%s\n' "$binary" LICENSE NOTICE README.md | LC_ALL=C sort)"
  if [ "$entries" != "$want" ]; then
    fail "$archive does not hold exactly $binary, README.md, LICENSE, and NOTICE at its root"
    diff <(echo "$want") <(echo "$entries") >&2 || true
  fi
  case "$archive" in
    *_linux_*)
      mkdir -p "$work/$archive"
      tar -xzf "$path" -C "$work/$archive" "$binary"
      description="$(file -b "$work/$archive/$binary")"
      echo "$archive: $description"
      case "$description" in
        *"statically linked"*) ;;
        *) fail "$binary in $archive is not statically linked" ;;
      esac
      ;;
  esac
done

if [ -f "$dir/checksums.txt" ]; then
  listed="$(awk '{ print $2 }' "$dir/checksums.txt")"
  sorted="$(printf '%s\n' "${archives[@]}" "$sbom" "${scripts[@]}" | LC_ALL=C sort)"
  if [ "$listed" != "$sorted" ]; then
    fail "checksums.txt does not list every asset once, sorted by file name"
    diff <(echo "$sorted") <(echo "$listed") >&2 || true
  fi
  (cd "$dir" && sha256sum -c checksums.txt) || fail "a checksum in checksums.txt does not match"
fi

if [ -f "$dir/$sbom" ]; then
  format="$(jq -r '.bomFormat' "$dir/$sbom" 2> /dev/null || true)"
  [ "$format" = CycloneDX ] || fail "$sbom is not a CycloneDX JSON document"
fi

if [ -f "$dir/install.sh" ] && ! grep -qxF "default_version=$version" "$dir/install.sh"; then
  fail "install.sh does not install $version by default"
fi
if [ -f "$dir/install.ps1" ] && ! grep -qxF "\$DefaultVersion = '$version'" "$dir/install.ps1"; then
  fail "install.ps1 does not install $version by default"
fi

if [ "$failed" -ne 0 ]; then
  exit 1
fi
echo "The assets for $version are complete."
