#!/usr/bin/env bash
# Usage: npm-publish.sh <packages> <version>
#
# Publishes the npm tarballs of <version> in <packages>, which check-npm.sh
# has checked: the six platform packages first and workflint last, so that
# workflint never points at a platform package that is not on npm yet.
#
# - Each package that `npm view` already lists at <version> is skipped, so a
#   re-run finishes a publish that failed halfway.
# - The scoped platform packages get --access public.
# - A pre-release version, one with a `-`, gets the dist-tag next instead of
#   latest.
#
# npm authenticates with trusted publishing, so the job needs id-token: write
# and no token, and npm adds the provenance statement.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: npm-publish.sh <packages> <version>" >&2
  exit 2
fi
packages="$1"
version="$2"
tag=latest
case "$version" in
  *-*) tag=next ;;
esac

names=(linux-x64 linux-arm64 darwin-x64 darwin-arm64 win32-x64 win32-arm64)
order=()
for name in "${names[@]}"; do
  order+=("@stokehold/workflint-$name stokehold-workflint-$name-$version.tgz --access public")
done
order+=("workflint workflint-$version.tgz")

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for row in "${order[@]}"; do
  read -r name file access <<< "$row"
  # npm view prints nothing for a version that does not exist, and fails
  # with E404 for a package that does not exist.
  if published="$(npm view "$name@$version" version 2> "$work/view.err")"; then
    if [ "$published" = "$version" ]; then
      echo "skip: npm already has $name@$version"
      continue
    fi
  elif ! grep -q E404 "$work/view.err"; then
    cat "$work/view.err" >&2
    echo "error: npm view $name@$version failed" >&2
    exit 1
  fi
  echo "publish: $name@$version with the dist-tag $tag"
  # shellcheck disable=SC2086 # $access is empty or two words.
  npm publish "$packages/$file" --provenance --tag "$tag" $access
done
echo "npm has the seven packages of $version."
