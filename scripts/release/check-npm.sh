#!/usr/bin/env bash
# Usage: check-npm.sh <packages> <archives> <version>
#
# Checks the npm tarballs that npm-pack.sh wrote into <packages> against the
# release archives in <archives>, before anything publishes them:
#
# - <packages> holds the seven tarballs of <version> and nothing else.
# - Each platform tarball holds package.json, README.md, LICENSE, NOTICE, and
#   the binary, which is executable on Linux and macOS. Its package.json has
#   the name, version, os, and cpu of docs/releasing.md and no scripts or
#   libc. Its binary has the SHA-256 of the binary in the matching archive.
# - The workflint tarball holds the launcher, README.md, LICENSE, and NOTICE.
#   Its package.json has no scripts, the repository that trusted publishing
#   needs, and the six platform packages as optional dependencies at
#   <version>.
#
# Needs jq, tar, sha256sum, and Python 3, as on the Ubuntu runners.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: check-npm.sh <packages> <archives> <version>" >&2
  exit 2
fi
packages="$1"
archives="$2"
version="$3"
repository="git+https://github.com/stokehold/workflint.git"
platforms=(
  "linux_amd64.tar.gz linux-x64 linux x64"
  "linux_arm64.tar.gz linux-arm64 linux arm64"
  "darwin_amd64.tar.gz darwin-x64 darwin x64"
  "darwin_arm64.tar.gz darwin-arm64 darwin arm64"
  "windows_amd64.zip win32-x64 win32 x64"
  "windows_arm64.zip win32-arm64 win32 arm64"
)
failed=0
fail() {
  echo "error: $*" >&2
  failed=1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

expected="workflint-$version.tgz"
for row in "${platforms[@]}"; do
  read -r _ suffix _ _ <<< "$row"
  expected+=$'\n'"stokehold-workflint-$suffix-$version.tgz"
done
expected="$(LC_ALL=C sort <<< "$expected")"
actual="$(cd "$packages" && find . -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort)"
if [ "$actual" != "$expected" ]; then
  fail "the tarballs are not the expected ones"
  diff <(echo "$expected") <(echo "$actual") >&2 || true
fi

# The names of the files in tarball <1>, without the package/ prefix.
entries() {
  tar -tzf "$1" | sed 's|^package/||' | LC_ALL=C sort
}
# The package.json of tarball <1>.
manifest() {
  tar -xzOf "$1" package/package.json
}
# Fails unless the package.json of tarball <1> has no scripts and the
# repository that trusted publishing needs.
check_common() {
  local tarball="$1" json
  json="$(manifest "$tarball")"
  if [ "$(jq 'has("scripts")' <<< "$json")" != false ]; then
    fail "$(basename "$tarball") has scripts: $(jq -c .scripts <<< "$json")"
  fi
  if [ "$(jq -r '.repository.url' <<< "$json")" != "$repository" ]; then
    fail "the repository of $(basename "$tarball") is not $repository"
  fi
  if [ "$(jq -r .version <<< "$json")" != "$version" ]; then
    fail "the version of $(basename "$tarball") is not $version"
  fi
}

for row in "${platforms[@]}"; do
  read -r suffix name os cpu <<< "$row"
  tarball="$packages/stokehold-workflint-$name-$version.tgz"
  archive="$archives/workflint_${version}_${suffix}"
  [ -f "$tarball" ] || continue
  if [ "$os" = win32 ]; then binary=workflint.exe; else binary=workflint; fi

  want="$(printf '%s\n' LICENSE NOTICE README.md "$binary" package.json | LC_ALL=C sort)"
  got="$(entries "$tarball")"
  if [ "$got" != "$want" ]; then
    fail "$(basename "$tarball") does not hold the expected files"
    diff <(echo "$want") <(echo "$got") >&2 || true
  fi
  check_common "$tarball"
  json="$(manifest "$tarball")"
  got="$(jq -c '[.name, .os, .cpu, has("libc")]' <<< "$json")"
  want="$(jq -nc --arg name "@stokehold/workflint-$name" --arg os "$os" --arg cpu "$cpu" \
    '[$name, [$os], [$cpu], false]')"
  [ "$got" = "$want" ] || fail "$(basename "$tarball") has name, os, cpu, and libc $got, not $want"
  if [ "$os" != win32 ]; then
    mode="$(tar -tvzf "$tarball" "package/$binary" | cut -c1-10)"
    [ "$mode" = "-rwxr-xr-x" ] || fail "the binary in $(basename "$tarball") has the mode $mode"
  fi

  in_package="$(tar -xzOf "$tarball" "package/$binary" | sha256sum | cut -d ' ' -f 1)" || in_package=missing
  case "$archive" in
    *.zip)
      in_archive="$(python3 -c 'import sys, zipfile; sys.stdout.buffer.write(zipfile.ZipFile(sys.argv[1]).read(sys.argv[2]))' \
        "$archive" "$binary" | sha256sum | cut -d ' ' -f 1)" || in_archive=missing
      ;;
    *)
      in_archive="$(tar -xzOf "$archive" "$binary" | sha256sum | cut -d ' ' -f 1)" || in_archive=missing
      ;;
  esac
  echo "$binary in $(basename "$tarball"): $in_package"
  echo "$binary in $(basename "$archive"): $in_archive"
  [ "$in_package" = "$in_archive" ] || fail "the binary in $(basename "$tarball") differs from the binary in $(basename "$archive")"
done

tarball="$packages/workflint-$version.tgz"
if [ -f "$tarball" ]; then
  want="$(printf '%s\n' LICENSE NOTICE README.md bin/workflint.js package.json | LC_ALL=C sort)"
  got="$(entries "$tarball")"
  if [ "$got" != "$want" ]; then
    fail "$(basename "$tarball") does not hold the expected files"
    diff <(echo "$want") <(echo "$got") >&2 || true
  fi
  check_common "$tarball"
  json="$(manifest "$tarball")"
  got="$(jq -c '[.name, .bin, .optionalDependencies, has("dependencies")]' <<< "$json")"
  want="$(jq -nc --arg version "$version" '["workflint", {workflint: "bin/workflint.js"},
    ($ARGS.positional | map({key: "@stokehold/workflint-\(.)", value: $version}) | from_entries), false]' \
    --args linux-x64 linux-arm64 darwin-x64 darwin-arm64 win32-x64 win32-arm64)"
  [ "$got" = "$want" ] || fail "workflint has name, bin, optionalDependencies, and dependencies $got, not $want"
fi

if [ "$failed" -ne 0 ]; then
  exit 1
fi
echo "The npm packages hold the binaries of the archives."
