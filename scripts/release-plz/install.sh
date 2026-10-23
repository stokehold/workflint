#!/usr/bin/env bash
# Usage: install.sh <directory>
#
# Downloads release-plz for x86_64 Linux at the version below, checks its
# SHA-256, and puts the binary into <directory>. release-plz.yml and the
# release-plz job in gate.yml install it with this script.
#
# This is the glibc build. The musl build of 0.3.172 stops with "stack limit
# reached" when it renders the changelog.
set -euo pipefail

version=0.3.172
target=x86_64-unknown-linux-gnu
sha256=84b7013d164e1fc780b31dc9ef190977830599d6cb5ce81c993f6fcbd3cdc1aa

if [ "$#" -ne 1 ]; then
  echo "usage: install.sh <directory>" >&2
  exit 2
fi
directory="$1"

if [ "$(uname -s)-$(uname -m)" != Linux-x86_64 ]; then
  echo "install.sh: release-plz is pinned for x86_64 Linux only, not $(uname -s)-$(uname -m)" >&2
  exit 1
fi

archive="release-plz-$target.tar.gz"
url="https://github.com/release-plz/release-plz/releases/download/release-plz-v$version/$archive"
download="$(mktemp -d)"
trap 'rm -rf "$download"' EXIT

curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$download/$archive" "$url"
(cd "$download" && echo "$sha256  $archive" | sha256sum --check --strict)
mkdir -p "$directory"
tar -xzf "$download/$archive" -C "$directory" release-plz
"$directory/release-plz" --version
