#!/usr/bin/env bash
# Installs workflint from a GitHub release of stokehold/workflint.
#
# Usage: install.sh [VERSION|latest] [DIR]
#        install.sh [--version VERSION] [--dir DIR] [--require-attestation]
#
# The script downloads the archive for this OS and architecture with
# checksums.txt, refuses an archive whose SHA-256 does not match, checks the
# archive's attestation with `gh attestation verify` when gh is installed and
# logged in, and puts the workflint binary into DIR. It prints the binary's
# path, and appends `executable=<path>` to $GITHUB_OUTPUT when that is set.
# The arguments are those of actionlint's download-actionlint.bash.
#
# Runs with bash 3.2 and later on Linux, macOS, and Git Bash on Windows.
# docs/releasing.md describes it.
set -euo pipefail

repo=stokehold/workflint
signer_workflow="$repo/.github/workflows/release.yml"
# The version to install when no VERSION is given. The release workflow sets
# it to the release's version in the copy it attaches to a release.
default_version=latest

usage() {
  cat << 'EOF'
Usage: install.sh [VERSION|latest] [DIR]
       install.sh [--version VERSION] [--dir DIR] [--require-attestation]

Installs workflint from https://github.com/stokehold/workflint/releases.

VERSION  A version such as 0.1.0 or v0.1.0, or latest.
DIR      The directory for the binary, created if missing. Defaults to the
         current directory.

--require-attestation  Fail when the attestation cannot be checked because
                       gh is missing or not logged in. The environment
                       variable WORKFLINT_REQUIRE_ATTESTATION=1 does the same.

WORKFLINT_DOWNLOAD_URL replaces the base URL
https://github.com/stokehold/workflint/releases/download, for a mirror. The
files come from <base URL>/v<version>/, and latest is read from the redirect
of the base URL with /download replaced by /latest.
EOF
}

say() {
  echo "$*" >&2
}

die() {
  echo "error: $*" >&2
  exit 1
}

version=""
dir=""
positional=0
require_attestation="${WORKFLINT_REQUIRE_ATTESTATION:-0}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    --version)
      [ "$#" -ge 2 ] || die "--version needs a value"
      version="$2"
      shift
      ;;
    --version=*) version="${1#--version=}" ;;
    --dir)
      [ "$#" -ge 2 ] || die "--dir needs a value"
      dir="$2"
      shift
      ;;
    --dir=*) dir="${1#--dir=}" ;;
    --require-attestation) require_attestation=1 ;;
    -*) die "unknown option $1; run install.sh --help for the usage" ;;
    *)
      positional=$((positional + 1))
      case "$positional" in
        1) version="$1" ;;
        2) dir="$1" ;;
        *) die "too many arguments; run install.sh --help for the usage" ;;
      esac
      ;;
  esac
  shift
done

[ -n "$version" ] || version="$default_version"
[ -n "$dir" ] || dir="$(pwd)"
case "$require_attestation" in
  0 | 1) ;;
  *) die "WORKFLINT_REQUIRE_ATTESTATION is '$require_attestation', not 0 or 1" ;;
esac

base_url="${WORKFLINT_DOWNLOAD_URL:-https://github.com/$repo/releases/download}"
base_url="${base_url%/}"

semver='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

# Prints the OS and architecture as they appear in the archive names.
detect_platform() {
  local os arch machine
  case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=darwin ;;
    MINGW* | MSYS* | CYGWIN*) os=windows ;;
    *) die "workflint has no build for the OS $(uname -s); see https://github.com/$repo/releases" ;;
  esac
  machine="$(uname -m)"
  if [ "$os" = windows ]; then
    # Git Bash runs as x86_64 on Windows on Arm, so uname -m does not tell.
    # Windows sets PROCESSOR_ARCHITEW6432 in a 32-bit process to the OS's
    # architecture, and PROCESSOR_IDENTIFIER to the CPU's in every process.
    machine="${PROCESSOR_ARCHITEW6432:-${PROCESSOR_ARCHITECTURE:-$machine}}"
    case "${PROCESSOR_IDENTIFIER:-}" in
      ARM*) machine=ARM64 ;;
    esac
  elif [ "$os" = darwin ] && [ "$machine" = x86_64 ]; then
    # A shell under Rosetta 2 on Apple silicon reports x86_64.
    if [ "$(sysctl -n sysctl.proc_translated 2> /dev/null || true)" = 1 ]; then
      machine=arm64
    fi
  fi
  case "$machine" in
    x86_64 | amd64 | AMD64) arch=amd64 ;;
    aarch64 | arm64 | ARM64) arch=arm64 ;;
    *) die "workflint has no build for the architecture $machine; see https://github.com/$repo/releases" ;;
  esac
  echo "$os $arch"
}

# Downloads a URL into a file, with retries. Fails on an HTTP error.
download() {
  local url="$1" out="$2" attempt
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL --retry 3 --retry-delay 1 -o "$out" "$url"
  elif command -v wget > /dev/null 2>&1; then
    for attempt in 1 2 3; do
      if wget -q -O "$out" "$url"; then
        return 0
      fi
      [ "$attempt" -eq 3 ] || sleep 1
    done
    return 1
  else
    die "install.sh needs curl or wget"
  fi
}

# Prints the version that a GitHub latest-release URL redirects to, read from
# the redirect's Location header, so no GitHub API call or rate limit applies.
resolve_latest() {
  local url="$1" location=""
  if command -v curl > /dev/null 2>&1; then
    location="$(curl -fsS --retry 3 --retry-delay 1 -o /dev/null -w '%{redirect_url}' "$url")" ||
      die "could not reach $url to find the latest version"
  elif command -v wget > /dev/null 2>&1; then
    location="$(wget -S --spider "$url" 2>&1 | awk 'tolower($1) == "location:" { print $2; exit }')" || true
  else
    die "install.sh needs curl or wget"
  fi
  location="$(printf '%s' "$location" | tr -d '\r')"
  local tag="${location##*/}"
  [[ "${tag#v}" =~ $semver ]] ||
    die "could not find the latest version: $url redirects to '$location', not to a release tag"
  echo "${tag#v}"
}

# Prints the lowercase SHA-256 of a file.
sha256() {
  local hash
  if command -v sha256sum > /dev/null 2>&1; then
    hash="$(sha256sum "$1")"
  elif command -v shasum > /dev/null 2>&1; then
    hash="$(shasum -a 256 "$1")"
  elif command -v certutil > /dev/null 2>&1; then
    # certutil prints the hash on its second line, with spaces between bytes
    # in older versions of Windows.
    hash="$(certutil -hashfile "$1" SHA256 | sed -n 2p | tr -d ' \r')"
  else
    die "install.sh needs sha256sum, shasum, or certutil to check the SHA-256"
  fi
  printf '%s\n' "${hash%% *}" | tr 'A-F' 'a-f'
}

case "$version" in
  latest | LATEST)
    latest_url="${base_url%/download}/latest"
    say "Finding the latest version from $latest_url"
    version="$(resolve_latest "$latest_url")"
    ;;
  *)
    version="${version#v}"
    [[ "$version" =~ $semver ]] ||
      die "the version '$version' is not a version such as 0.1.0 or v0.1.0, or latest"
    ;;
esac

platform="$(detect_platform)"
os="${platform% *}"
arch="${platform#* }"
if [ "$os" = windows ]; then
  archive="workflint_${version}_${os}_${arch}.zip"
  binary=workflint.exe
else
  archive="workflint_${version}_${os}_${arch}.tar.gz"
  binary=workflint
fi
release_url="$base_url/v$version"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/workflint-install.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

say "Downloading workflint $version for $os/$arch from $release_url"
download "$release_url/checksums.txt" "$tmp/checksums.txt" ||
  die "could not download $release_url/checksums.txt; is $version a release of workflint?"
download "$release_url/$archive" "$tmp/$archive" ||
  die "could not download $release_url/$archive"

# checksums.txt has one `<SHA-256>  <file name>` line per asset.
expected="$(tr -d '\r' < "$tmp/checksums.txt" |
  awk -v name="$archive" '$2 == name || $2 == "*" name { print tolower($1); exit }')"
[ -n "$expected" ] || die "checksums.txt has no SHA-256 for $archive; nothing was installed"
actual="$(sha256 "$tmp/$archive")"
if [ "$actual" != "$expected" ]; then
  die "the SHA-256 of $archive is $actual, but checksums.txt expects $expected; nothing was installed"
fi
say "Checked the SHA-256 of $archive: $actual"

verify_command="gh attestation verify $archive --repo $repo --signer-workflow $signer_workflow"
if command -v gh > /dev/null 2>&1 && gh auth status > /dev/null 2>&1; then
  gh attestation verify "$tmp/$archive" --repo "$repo" --signer-workflow "$signer_workflow" > /dev/null ||
    die "the attestation of $archive does not check out; nothing was installed"
  say "Checked the attestation of $archive"
elif [ "$require_attestation" = 1 ]; then
  die "the attestation check needs gh, installed and logged in, and --require-attestation or WORKFLINT_REQUIRE_ATTESTATION=1 requires it; nothing was installed"
else
  say "Skipped the attestation check because gh is missing or not logged in. To check it, download $archive and run: $verify_command"
fi

mkdir "$tmp/unpacked"
if [ "$os" = windows ]; then
  command -v unzip > /dev/null 2>&1 || die "install.sh needs unzip on Windows"
  unzip -q "$tmp/$archive" "$binary" -d "$tmp/unpacked"
else
  tar -xzf "$tmp/$archive" -C "$tmp/unpacked" "$binary"
fi
[ -f "$tmp/unpacked/$binary" ] || die "$archive has no $binary"

mkdir -p "$dir"
dir="$(cd "$dir" && pwd)"
mv -f "$tmp/unpacked/$binary" "$dir/$binary"
chmod +x "$dir/$binary"
executable="$dir/$binary"

say "Installed workflint $version to $executable"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "executable=$executable" >> "$GITHUB_OUTPUT"
fi
echo "$executable"
