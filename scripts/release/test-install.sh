#!/usr/bin/env bash
# Usage: test-install.sh <assets directory> <version>
#
# Tests scripts/install.sh and scripts/install.ps1 against the release assets
# of <version> in <assets directory>: the archives, checksums.txt, and the
# release copies of the install scripts that the release workflow builds. No
# GitHub release is needed. serve.py serves the assets on 127.0.0.1, and
# WORKFLINT_DOWNLOAD_URL points the scripts at it.
#
# Runs install.sh with /bin/bash on macOS, which is bash 3.2, and with bash
# elsewhere. Runs install.ps1 with pwsh when it is installed, and also with
# Windows PowerShell 5.1 on Windows. Fails when pwsh is missing on CI, where
# the CI environment variable is set. Linux also runs install.sh with wget in
# place of curl. gh never counts as logged in, so every attestation check is
# skipped.
#
# Needs python3 (python on Windows), curl, and sha256sum or shasum.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: test-install.sh <assets directory> <version>" >&2
  exit 2
fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scripts="$(cd "$here/.." && pwd)"
assets="$(cd "$1" && pwd)"
version="$2"

work="$(mktemp -d)"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ]; then
    kill "$server_pid" 2> /dev/null || true
    wait "$server_pid" 2> /dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT
failures=0

windows=0
exe=""
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    windows=1
    exe=.exe
    ;;
esac

# Prints a path in the form that a native Windows program reads from an
# environment variable. Elsewhere prints it unchanged.
native() {
  if [ "$windows" = 1 ]; then cygpath -w "$1"; else printf '%s\n' "$1"; fi
}

# Prints a path that a native Windows program printed in the form that bash
# reads. Elsewhere prints it unchanged.
posix() {
  if [ "$windows" = 1 ]; then cygpath -u "$1"; else printf '%s\n' "$1"; fi
}

hash_of() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

# The server's trees, each laid out like https://github.com/stokehold/workflint:
#
#   good      The assets as built.
#   tampered  Every archive has one more byte than checksums.txt says.
#   unlisted  checksums.txt lists no archive.
www="$work/www"
for tree in good tampered unlisted; do
  mkdir -p "$www/$tree/releases/download/v$version"
  cp "$assets"/* "$www/$tree/releases/download/v$version/"
done
grep -v ' workflint_.*_.*_' "$assets/checksums.txt" > "$www/unlisted/releases/download/v$version/checksums.txt" || true

python=python3
if ! command -v python3 > /dev/null 2>&1 || [ "$windows" = 1 ]; then
  python=python
fi

# Each tampered archive still unpacks to the same files, so only the
# checksum can catch it: a .tar.gz is compressed again at another level, and
# a .zip gains a one-byte archive comment.
for archive in "$www/tampered/releases/download/v$version"/workflint_*_*_*; do
  "$python" -I -c '
import gzip, sys
path = sys.argv[1]
with open(path, "rb") as f:
    data = f.read()
if path.endswith(".tar.gz"):
    data = gzip.compress(gzip.decompress(data), compresslevel=1, mtime=0)
elif data[-22:-18] == b"PK\x05\x06" and data[-2:] == b"\x00\x00":
    data = data[:-2] + b"\x01\x00x"
else:
    sys.exit("cannot tamper with " + path)
with open(path, "wb") as f:
    f.write(data)
' "$archive"
done
"$python" -I "$here/serve.py" "$www" "$work/port" "v$version" 2> "$work/server.log" &
server_pid=$!
for _ in $(seq 1 100); do
  [ -s "$work/port" ] && break
  sleep 0.1
done
if ! [ -s "$work/port" ]; then
  echo "error: serve.py did not start" >&2
  cat "$work/server.log" >&2
  exit 1
fi
server="http://127.0.0.1:$(cat "$work/port")"

# The settings of each run. The cases change them.
base_url="$server/good/releases/download"
require_env=""
sh_script="$scripts/install.sh"
ps_script="$scripts/install.ps1"
sh_bash=bash
if [ "$(uname -s)" = Darwin ]; then
  sh_bash=/bin/bash
fi
sh_path="$PATH"

mkdir -p "$work/gh"

# Runs an install script and records its exit status in $status, and its
# standard output and standard error in $work/stdout and $work/stderr.
# $work/github_output is its GITHUB_OUTPUT. The arguments use the syntax of
# install.sh, which this function maps to that of install.ps1.
# Usage: run <runner> <working directory> <argument...>
# <runner> is sh, pwsh, or powershell.
run() {
  local runner="$1" cwd="$2"
  shift 2
  : > "$work/github_output"
  status=0
  (
    cd "$cwd"
    unset GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN GH_HOST WORKFLINT_REQUIRE_ATTESTATION
    GH_CONFIG_DIR="$(native "$work/gh")"
    WORKFLINT_DOWNLOAD_URL="$base_url"
    export GH_CONFIG_DIR WORKFLINT_DOWNLOAD_URL
    if [ -n "$require_env" ]; then
      export WORKFLINT_REQUIRE_ATTESTATION="$require_env"
    fi
    case "$runner" in
      sh)
        GITHUB_OUTPUT="$work/github_output" PATH="$sh_path" "$sh_bash" "$sh_script" "$@"
        ;;
      *)
        local n="$#" arg
        while [ "$n" -gt 0 ]; do
          arg="$1"
          shift
          case "$arg" in
            --version) arg=-Version ;;
            --dir) arg=-Dir ;;
            --require-attestation) arg=-RequireAttestation ;;
          esac
          set -- "$@" "$arg"
          n=$((n - 1))
        done
        if [ "$windows" = 1 ]; then
          set -- -ExecutionPolicy Bypass -File "$(native "$ps_script")" "$@"
        else
          set -- -File "$ps_script" "$@"
        fi
        GITHUB_OUTPUT="$(native "$work/github_output")" "$runner" -NoProfile -NonInteractive "$@"
        ;;
    esac
  ) > "$work/stdout" 2> "$work/stderr" || status=$?
}

pass() {
  echo "ok: $1"
}

fail() {
  echo "FAIL: $1: $2"
  echo "  exit status: $status"
  echo "  stdout:"
  sed 's/^/    /' "$work/stdout"
  echo "  stderr:"
  sed 's/^/    /' "$work/stderr"
  failures=$((failures + 1))
}

# Checks that the last run installed $version into <directory>: it exited
# with 0, printed the binary's path last, wrote the same path to
# GITHUB_OUTPUT, and the binary passes smoke-test.sh.
# Usage: expect_installed <name> <directory>
expect_installed() {
  local name="$1" dir="$2" printed want
  if [ "$status" -ne 0 ]; then
    fail "$name" "the install failed"
    return
  fi
  want="$dir/workflint$exe"
  printed="$(tail -n 1 "$work/stdout" | tr -d '\r')"
  if [ -z "$printed" ] || ! [ "$(posix "$printed")" -ef "$want" ]; then
    fail "$name" "the printed path '$printed' is not $want"
    return
  fi
  if [ "$(tr -d '\r' < "$work/github_output")" != "executable=$printed" ]; then
    fail "$name" "GITHUB_OUTPUT holds '$(cat "$work/github_output")', not 'executable=$printed'"
    return
  fi
  if ! "$here/smoke-test.sh" "$want" "$version" > "$work/smoke" 2>&1; then
    cat "$work/smoke" >> "$work/stderr"
    fail "$name" "the installed binary fails the smoke test"
    return
  fi
  pass "$name"
}

# Checks that the last run failed with an error line that holds <text>.
# Usage: expect_failure <name> <text>
expect_failure() {
  local name="$1" text="$2"
  if [ "$status" -eq 0 ]; then
    fail "$name" "the install succeeded"
  elif ! tr -d '\r' < "$work/stderr" | grep '^error: ' | grep -qF -- "$text"; then
    fail "$name" "standard error has no error line with '$text'"
  else
    pass "$name"
  fi
}

# A directory with an old binary in it, to check that a failed install leaves
# it unchanged.
prepare_untouched() {
  rm -rf "$work/untouched"
  mkdir -p "$work/untouched"
  echo old > "$work/untouched/workflint$exe"
}

# Checks that the directory from prepare_untouched still holds only the old
# binary, and that <missing directory> was not created.
# Usage: expect_untouched <name> <missing directory>
expect_untouched() {
  local name="$1" missing="$2"
  if [ "$(ls -A "$work/untouched")" != "workflint$exe" ] ||
    [ "$(cat "$work/untouched/workflint$exe")" != old ]; then
    fail "$name" "the failed install changed its directory"
  elif [ -e "$missing" ]; then
    fail "$name" "the failed install created $missing"
  else
    pass "$name"
  fi
}

runners="sh"
if command -v pwsh > /dev/null 2>&1; then
  runners="$runners pwsh"
elif [ -n "${CI:-}" ]; then
  echo "error: pwsh is missing, so install.ps1 cannot be tested" >&2
  exit 1
fi
if [ "$windows" = 1 ] && command -v powershell > /dev/null 2>&1; then
  runners="$runners powershell"
fi
echo "Testing install.sh with $("$sh_bash" --version | head -n 1)"
echo "Runners: $runners"

for runner in $runners; do
  if [ "$runner" = sh ]; then script=install.sh; else script="install.ps1 ($runner)"; fi
  t="$work/$runner"
  mkdir -p "$t"

  base_url="$server/good/releases/download"
  require_env=""

  run "$runner" "$t" "$version" "$t/bare"
  expect_installed "$script installs a bare version into DIR" "$t/bare"

  run "$runner" "$t" "v$version" "$t/prefixed"
  expect_installed "$script installs a version with a leading v" "$t/prefixed"

  run "$runner" "$t" --version "$version" --dir "$t/named/nested"
  expect_installed "$script takes the version and DIR as named options and creates DIR" "$t/named/nested"

  mkdir -p "$t/cwd"
  run "$runner" "$t/cwd" "$version"
  expect_installed "$script installs into the current directory without DIR" "$t/cwd"

  run "$runner" "$t" latest "$t/latest"
  expect_installed "$script resolves latest from the redirect of releases/latest" "$t/latest"

  # The release copy installs its own version when no version is given.
  sh_script="$assets/install.sh"
  ps_script="$assets/install.ps1"
  mkdir -p "$t/release-copy"
  run "$runner" "$t/release-copy"
  expect_installed "the release copy of $script installs its release without arguments" "$t/release-copy"
  sh_script="$scripts/install.sh"
  ps_script="$scripts/install.ps1"

  base_url="$server/tampered/releases/download"
  prepare_untouched
  run "$runner" "$t" "$version" "$work/untouched"
  archive="$(tr -d '\r' < "$work/stderr" | sed -n 's/^error: the SHA-256 of \([^ ]*\) is .*/\1/p')"
  if [ -z "$archive" ]; then
    expect_failure "$script refuses a tampered archive" "the SHA-256 of"
  else
    expect_failure "$script names the actual SHA-256 of a tampered archive" \
      "is $(hash_of "$www/tampered/releases/download/v$version/$archive")"
    expect_failure "$script names the expected SHA-256 of a tampered archive" \
      "expects $(hash_of "$assets/$archive")"
  fi
  expect_untouched "$script leaves DIR unchanged after a checksum mismatch" "$t/none"
  run "$runner" "$t" "$version" "$t/none"
  expect_untouched "$script does not create DIR after a checksum mismatch" "$t/none"

  base_url="$server/unlisted/releases/download"
  prepare_untouched
  run "$runner" "$t" "$version" "$work/untouched"
  expect_failure "$script refuses an archive that checksums.txt does not list" "checksums.txt has no SHA-256"
  expect_untouched "$script leaves DIR unchanged when checksums.txt does not list the archive" "$t/none"

  base_url="$server/good/releases/download"
  prepare_untouched
  run "$runner" "$t" 9.9.9 "$work/untouched"
  expect_failure "$script fails for an unknown version" "is 9.9.9 a release of workflint?"
  expect_untouched "$script leaves DIR unchanged for an unknown version" "$t/none"

  run "$runner" "$t" 1.2 "$work/untouched"
  expect_failure "$script fails for a malformed version" "is not a version"

  prepare_untouched
  run "$runner" "$t" --require-attestation "$version" "$work/untouched"
  expect_failure "$script fails with --require-attestation when gh is not logged in" "the attestation check needs gh"
  expect_untouched "$script leaves DIR unchanged when the attestation check is required and skipped" "$t/none"

  require_env=1
  run "$runner" "$t" "$version" "$work/untouched"
  expect_failure "$script fails with WORKFLINT_REQUIRE_ATTESTATION=1 when gh is not logged in" "the attestation check needs gh"
  require_env=""

  run "$runner" "$t" "$version" "$t/skipped"
  if grep -qF "gh attestation verify workflint_${version}_" "$work/stderr"; then
    pass "$script prints the attestation command when it skips the check"
  else
    fail "$script prints the attestation command when it skips the check" "standard error does not name the command"
  fi
done

# Runs install.sh with wget in place of curl, on a PATH with only the tools
# it needs.
if [ "$(uname -s)" = Linux ] && command -v wget > /dev/null 2>&1; then
  bin="$work/wget-bin"
  mkdir -p "$bin"
  for tool in bash env uname mktemp rm mkdir mv chmod tar gzip awk sed tr sha256sum wget sleep pwd cat; do
    ln -s "$(command -v "$tool")" "$bin/$tool"
  done
  sh_path="$bin"
  sh_bash="$bin/bash"
  run sh "$work" "$version" "$work/wget"
  expect_installed "install.sh installs with wget when curl is missing" "$work/wget"
  run sh "$work" latest "$work/wget-latest"
  expect_installed "install.sh resolves latest with wget" "$work/wget-latest"
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures install script tests failed"
  exit 1
fi
echo "All install script tests passed."
