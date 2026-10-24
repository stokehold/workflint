#!/usr/bin/env bash
# Usage: test-homebrew.sh <assets> <version>
#
# Tests the Homebrew formula against the archives and checksums.txt in
# <assets>, since no release exists to download from. It renders the formula
# with file:// URLs that point at <assets>, adds it to a new local tap made
# with `brew tap-new`, runs `brew style` and `brew audit --strict` on it,
# installs it, runs `brew test`, and runs smoke-test.sh on the installed
# workflint. It then checks that the second line of `-version` is
# `installed via Homebrew`, and removes the formula and the tap.
#
# Needs brew on the PATH. Installing the formula also installs shellcheck.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: test-homebrew.sh <assets> <version>" >&2
  exit 2
fi
assets="$(cd "$1" && pwd)"
version="$2"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tap=workflint/dry-run
formula="$tap/workflint"

export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ANALYTICS=1 HOMEBREW_NO_ENV_HINTS=1 HOMEBREW_NO_INSTALL_CLEANUP=1

brew --version
brew tap-new --no-git "$tap"
cleanup() {
  brew uninstall --formula "$formula" > /dev/null 2>&1 || true
  brew untap "$tap" > /dev/null 2>&1 || true
}
trap cleanup EXIT
tap_dir="$(brew --repository "$tap")"
"$here/homebrew-formula.sh" "$version" "$assets/checksums.txt" "file://$assets" > "$tap_dir/Formula/workflint.rb"
cat "$tap_dir/Formula/workflint.rb"

brew style "$formula"
brew audit --strict --formula "$formula"
brew install --formula "$formula"
brew test "$formula"

binary="$(brew --prefix)/bin/workflint"
"$here/smoke-test.sh" "$binary" "$version"
want="installed via Homebrew"
got="$("$binary" -version | sed -n 2p)"
if [ "$got" != "$want" ]; then
  echo "error: the second line of $binary -version is '$got', not '$want'" >&2
  exit 1
fi
echo "the Homebrew formula of $version installs and passes its test"
