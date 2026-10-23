#!/usr/bin/env bash
# Usage: check-image.sh <image> <default|full> <archive> <version>
#
# Checks an image of ghcr.io/stokehold/workflint that the local Docker daemon
# has, against the release archive of the same architecture:
#
# - smoke-test.sh passes `docker run --rm <image>`, and the second line of
#   -version is `installed from Docker image`.
# - /usr/local/bin/workflint is byte-identical to the binary in <archive>.
# - The image runs as a user other than root.
# - workflint lints a mounted repository in /repo. In the full image it
#   reports a shellcheck finding and a Python finding, and shellcheck, ruff,
#   and pyflakes print the versions that docker/ pins.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: check-image.sh <image> <default|full> <archive> <version>" >&2
  exit 2
fi
image="$1"
stage="$2"
archive="$3"
version="$4"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
docker_dir="$here/../../docker"

case "$stage" in
  default | full) ;;
  *)
    echo "error: the image is '$stage', not default or full" >&2
    exit 2
    ;;
esac

work="$(mktemp -d)"
container=""
cleanup() {
  if [ -n "$container" ]; then docker rm "$container" > /dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

printf '#!/bin/sh\nexec docker run --rm %s "$@"\n' "$image" > "$work/workflint"
chmod +x "$work/workflint"
"$here/smoke-test.sh" "$work/workflint" "$version"
got="$("$work/workflint" -version | sed -n 2p)"
if [ "$got" != "installed from Docker image" ]; then
  echo "error: the second line of -version is '$got', not 'installed from Docker image'" >&2
  exit 1
fi

mkdir "$work/archive"
tar -xzf "$archive" -C "$work/archive" workflint
container="$(docker create "$image")"
docker cp "$container:/usr/local/bin/workflint" "$work/image-workflint"
in_image="$(sha256sum "$work/image-workflint" | cut -d ' ' -f 1)"
in_archive="$(sha256sum "$work/archive/workflint" | cut -d ' ' -f 1)"
echo "workflint in $image: $in_image"
echo "workflint in $(basename "$archive"): $in_archive"
if [ "$in_image" != "$in_archive" ]; then
  echo "error: the binary in $image differs from the binary in $archive" >&2
  exit 1
fi

user="$(docker image inspect --format '{{.Config.User}}' "$image")"
case "${user%%:*}" in
  "" | 0 | root)
    echo "error: $image runs as root (USER is '$user')" >&2
    exit 1
    ;;
esac
echo "$image runs as $user"

# A repository that the user of the image can read.
repo="$work/repo"
mkdir -p "$repo/.github/workflows"
if [ "$stage" = default ]; then
  cat > "$repo/.github/workflows/ci.yml" << 'YAML'
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
YAML
  chmod -R a+rX "$repo"
  docker run --rm -v "$repo:/repo:ro" -w /repo "$image"
  echo "$image lints a clean repository"
else
  cat > "$repo/.github/workflows/ci.yml" << 'YAML'
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo $FOO
      - run: import os
        shell: python
YAML
  chmod -R a+rX "$repo"
  status=0
  output="$(docker run --rm -v "$repo:/repo:ro" -w /repo "$image" -no-color)" || status=$?
  printf '%s\n' "$output"
  if [ "$status" -ne 1 ]; then
    echo "error: workflint exited with $status, not 1, on a repository with findings" >&2
    exit 1
  fi
  for want in SC2086 "imported but unused"; do
    if ! grep -qF -- "$want" <<< "$output"; then
      echo "error: workflint did not report '$want' in $image" >&2
      exit 1
    fi
  done

  want_shellcheck="$(sed -n 's|.*/shellcheck/releases/download/v\([^/]*\)/.*|\1|p' "$docker_dir/Dockerfile" | sort -u)"
  want_ruff="$(sed -n 's/^ruff==\([^ ]*\).*/\1/p' "$docker_dir/requirements-full.txt")"
  want_pyflakes="$(sed -n 's/^pyflakes==\([^ ]*\).*/\1/p' "$docker_dir/requirements-full.txt")"
  got_shellcheck="$(docker run --rm --entrypoint shellcheck "$image" --version | sed -n 's/^version: //p')"
  got_ruff="$(docker run --rm --entrypoint ruff "$image" --version)"
  got_pyflakes="$(docker run --rm --entrypoint pyflakes "$image" --version)"
  echo "shellcheck $got_shellcheck, $got_ruff, pyflakes $got_pyflakes"
  if [ "$got_shellcheck" != "$want_shellcheck" ]; then
    echo "error: shellcheck is $got_shellcheck, not $want_shellcheck" >&2
    exit 1
  fi
  if [ "$got_ruff" != "ruff $want_ruff" ]; then
    echo "error: '$got_ruff' is not ruff $want_ruff" >&2
    exit 1
  fi
  if [ "${got_pyflakes%% *}" != "$want_pyflakes" ]; then
    echo "error: pyflakes is '$got_pyflakes', not $want_pyflakes" >&2
    exit 1
  fi
  for file in LICENSE.txt SOURCE.txt; do
    if ! docker run --rm --entrypoint test "$image" -s "/usr/share/doc/shellcheck/$file"; then
      echo "error: $image has no /usr/share/doc/shellcheck/$file" >&2
      exit 1
    fi
  done
fi
echo "The $stage image passed its checks."
