#!/usr/bin/env bash
# Usage: smoke-test-npm.sh <packages> <version>
#
# Installs workflint <version> from the npm tarballs in <packages> the ways
# that docs/releasing.md lists, and runs smoke-test.sh on each:
#
# - npm install, then npx workflint.
# - npm install --ignore-scripts, then npx workflint.
# - pnpm add with pnpm's default settings, then pnpm exec workflint.
# - bun add, then bunx workflint.
#
# npm-registry.mjs serves the tarballs on 127.0.0.1, so each package manager
# resolves workflint's optional dependencies itself and installs only the
# platform package of this machine. The script checks that it did, and that
# an install with --omit=optional fails with the launcher's message. No
# install reads the public registry. pnpm and Bun come from npm-tools/, which
# npm ci installs with --ignore-scripts. Needs Node.js and npm.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: smoke-test-npm.sh <packages> <version>" >&2
  exit 2
fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
packages="$(cd "$1" && pwd)"
version="$2"

work="$(mktemp -d)"
registry_pid=
cleanup() {
  if [ -n "$registry_pid" ]; then kill "$registry_pid" 2> /dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

# Keeps the user's npm config and caches out of the test.
: > "$work/user-npmrc"
export npm_config_userconfig="$work/user-npmrc"
export npm_config_cache="$work/npm-cache"
export npm_config_audit=false npm_config_fund=false npm_config_update_notifier=false
export BUN_INSTALL_CACHE_DIR="$work/bun-cache"
unset WORKFLINT_INSTALLED_FROM

echo "Installing pnpm and Bun from npm-tools/package-lock.json"
mkdir "$work/tools"
cp "$here/npm-tools/package.json" "$here/npm-tools/package-lock.json" "$work/tools/"
(cd "$work/tools" && npm ci --ignore-scripts --silent --registry https://registry.npmjs.org/)
# Without their install scripts, the pnpm and bun packages keep placeholders
# in place of their binaries, so the script runs the binaries from their
# platform packages.
pnpm="$(find "$work/tools/node_modules/@pnpm" -maxdepth 2 \( -name pnpm -o -name pnpm.exe \) -type f | head -n 1)"
bun="$(find "$work/tools/node_modules/@oven" -maxdepth 3 -path '*/bin/*' \( -name bun -o -name bun.exe \) -type f | head -n 1)"
if [ -z "$pnpm" ] || [ -z "$bun" ]; then
  echo "error: npm ci installed no pnpm or no bun binary for this platform" >&2
  exit 1
fi
echo "node $(node --version), npm $(npm --version), pnpm $("$pnpm" --version), bun $("$bun" --version)"

node "$here/npm-registry.mjs" "$packages" "$work/port" > "$work/registry.log" 2>&1 &
registry_pid=$!
for _ in $(seq 1 100); do
  [ -s "$work/port" ] && break
  sleep 0.1
done
if ! [ -s "$work/port" ]; then
  echo "error: the registry did not start" >&2
  cat "$work/registry.log" >&2
  exit 1
fi
registry="http://127.0.0.1:$(tr -d '\r\n' < "$work/port")/"
echo "registry $registry"

platform="$(node -p 'process.platform + "-" + process.arch' | tr -d '\r')"
expected="@stokehold/workflint-$platform"
failed=0
fail() {
  echo "error: $*" >&2
  failed=1
}

# Creates the project <name>, empty but for an .npmrc that points every
# package manager at the registry, and prints its path.
project() {
  local dir="$work/$1"
  mkdir "$dir"
  printf '{ "name": "smoke-%s", "private": true }\n' "$1" > "$dir/package.json"
  printf 'registry=%s\n' "$registry" > "$dir/.npmrc"
  echo "$dir"
}

# Fails unless <dir>/<node_modules path> holds exactly the platform package
# of this machine.
# Usage: check_platform <name> <directory> <command that lists the packages>
check_platform() {
  local name="$1" dir="$2" got
  got="$(cd "$dir" && bash -c "$3" | tr -d '\r' | LC_ALL=C sort | tr '\n' ' ')"
  if [ "$got" != "$expected " ]; then
    fail "$name installed the platform packages '$got', not $expected"
  else
    echo "$name installed $expected and no other platform package"
  fi
}

# Writes a script that runs workflint in <dir> through <runner>, and runs
# smoke-test.sh on it. Then checks the second line of -version.
# Usage: smoke <name> <directory> <runner...>
smoke() {
  local name="$1" dir="$2"
  shift 2
  local script="$work/run-$name"
  {
    echo '#!/usr/bin/env bash'
    printf 'cd %q && exec' "$dir"
    printf ' %q' "$@"
    echo ' "$@"'
  } > "$script"
  chmod +x "$script"
  echo "$name: $*"
  "$here/smoke-test.sh" "$script" "$version" || fail "$name: the smoke test failed"
  local second
  second="$("$script" -version | sed -n 2p | tr -d '\r')" || true
  [ "$second" = "installed from npm" ] || fail "$name: the second line of -version is '$second'"
}

echo "::group::npm install"
dir="$(project npm)"
(cd "$dir" && npm install --save-dev "workflint@$version" --registry "$registry")
check_platform "npm install" "$dir" "ls -d node_modules/@stokehold/*/ | sed -e 's|^node_modules/||' -e 's|/\$||'"
smoke npm "$dir" npx --no -- workflint
echo "::endgroup::"

echo "::group::npm install --ignore-scripts"
dir="$(project npm-ignore-scripts)"
(cd "$dir" && npm install --ignore-scripts --save-dev "workflint@$version" --registry "$registry")
check_platform "npm install --ignore-scripts" "$dir" "ls -d node_modules/@stokehold/*/ | sed -e 's|^node_modules/||' -e 's|/\$||'"
smoke npm-ignore-scripts "$dir" npx --no -- workflint
echo "::endgroup::"

echo "::group::pnpm add"
dir="$(project pnpm)"
(cd "$dir" && "$pnpm" add --save-dev "workflint@$version" --registry "$registry" --store-dir "$work/pnpm-store")
# pnpm keeps each package in node_modules/.pnpm/<name with + for />@<version>.
check_platform "pnpm add" "$dir" "ls node_modules/.pnpm | grep '^@stokehold+' | sed -e 's|+|/|' -e 's|@[^@]*\$||'"
smoke pnpm "$dir" "$pnpm" exec workflint
echo "::endgroup::"

echo "::group::bun add"
dir="$(project bun)"
(cd "$dir" && "$bun" add --dev "workflint@$version" --registry "$registry")
check_platform "bun add" "$dir" "ls -d node_modules/@stokehold/*/ | sed -e 's|^node_modules/||' -e 's|/\$||'"
smoke bun "$dir" "$bun" x workflint
echo "::endgroup::"

echo "::group::npm install --omit=optional"
dir="$(project npm-omit-optional)"
(cd "$dir" && npm install --omit=optional --save-dev "workflint@$version" --registry "$registry")
status=0
output="$(cd "$dir" && npx --no -- workflint -version 2>&1)" || status=$?
printf '%s\n' "$output"
if [ "$status" -ne 3 ] || ! grep -q "the package $expected" <<< "$output"; then
  fail "npm install --omit=optional: workflint exited with $status, not 3 with the launcher's message"
fi
echo "::endgroup::"

if [ "$failed" -ne 0 ]; then
  echo "The registry served:" >&2
  cat "$work/registry.log" >&2
  exit 1
fi
echo "npm, pnpm, and Bun installed workflint $version from its optional dependencies."
