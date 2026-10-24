#!/usr/bin/env bash
# Usage: npm-pack.sh <version> <archives> <out>
#
# Writes the seven npm packages of ADR 0014 for <version> from the release
# archives in <archives>, and packs each into <out> with `npm pack`:
#
# - One platform package per archive, @stokehold/workflint-<os>-<arch>, with
#   the os and cpu of the table below, the binary taken unchanged from the
#   archive, and npm/platform/package.json and README.md as templates.
# - workflint, from npm/workflint/, with the launcher and one optional
#   dependency per platform package, pinned at <version>.
#
# The packages in git have no version. This script writes <version> into all
# seven package.json files. LICENSE and NOTICE come from the repository root.
# Needs jq, tar, npm, and Python 3, as on the Ubuntu runners.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: npm-pack.sh <version> <archives> <out>" >&2
  exit 2
fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
version="$1"
archives="$(cd "$2" && pwd)"
mkdir -p "$3"
out="$(cd "$3" && pwd)"

# A version that npm accepts, and nothing that could escape a path.
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a version" >&2
  exit 1
fi

# <archive suffix> <package> <os> <cpu>, as fixed by M6.1.
platforms=(
  "linux_amd64.tar.gz @stokehold/workflint-linux-x64 linux x64"
  "linux_arm64.tar.gz @stokehold/workflint-linux-arm64 linux arm64"
  "darwin_amd64.tar.gz @stokehold/workflint-darwin-x64 darwin x64"
  "darwin_arm64.tar.gz @stokehold/workflint-darwin-arm64 darwin arm64"
  "windows_amd64.zip @stokehold/workflint-win32-x64 win32 x64"
  "windows_arm64.zip @stokehold/workflint-win32-arm64 win32 arm64"
)

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

pack() {
  # --ignore-scripts: the packages have no scripts, and none may run here.
  (cd "$1" && npm pack --ignore-scripts --silent --pack-destination "$out") > /dev/null
}

optional='{}'
for row in "${platforms[@]}"; do
  read -r suffix name os cpu <<< "$row"
  archive="$archives/workflint_${version}_${suffix}"
  dir="$stage/${name#@stokehold/}"
  mkdir -p "$dir"
  case "$suffix" in
    *.zip)
      binary=workflint.exe
      python3 - "$archive" "$dir" << 'EOF'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    archive.extract("workflint.exe", sys.argv[2])
EOF
      ;;
    *)
      binary=workflint
      tar -xzf "$archive" -C "$dir" workflint
      ;;
  esac
  chmod 755 "$dir/$binary"
  jq --arg name "$name" --arg version "$version" --arg os "$os" --arg cpu "$cpu" --arg binary "$binary" \
    '{name: $name, version: $version} + . + {os: [$os], cpu: [$cpu], files: [$binary, "NOTICE"]}' \
    "$root/npm/platform/package.json" > "$dir/package.json"
  sed -e "s|{{name}}|$name|g" -e "s|{{os}}|$os|g" -e "s|{{cpu}}|$cpu|g" \
    "$root/npm/platform/README.md" > "$dir/README.md"
  cp "$root/LICENSE" "$root/NOTICE" "$dir/"
  pack "$dir"
  optional="$(jq -c --arg name "$name" --arg version "$version" '. + {($name): $version}' <<< "$optional")"
done

dir="$stage/workflint"
mkdir -p "$dir/bin"
cp "$root/npm/workflint/bin/workflint.js" "$dir/bin/"
cp "$root/npm/workflint/README.md" "$root/LICENSE" "$root/NOTICE" "$dir/"
jq --arg version "$version" --argjson optional "$optional" \
  '{name, version: $version} + . + {optionalDependencies: $optional}' \
  "$root/npm/workflint/package.json" > "$dir/package.json"
pack "$dir"

(cd "$out" && ls -- *.tgz)
