#!/usr/bin/env bash
# Usage: docker-tags.sh <version> <default|full>
#
# Prints the tags of ghcr.io/stokehold/workflint for an image of a release,
# one per line (docs/releasing.md). A release such as 1.2.3 tags the default
# image 1.2.3, 1.2, and latest, and the full image 1.2.3-full, 1.2-full, and
# full. A pre-release such as 1.2.3-rc.1 gets only its exact tag, so latest
# and full keep pointing at the last release.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: docker-tags.sh <version> <default|full>" >&2
  exit 2
fi
version="$1"
image="$2"

case "$image" in
  default) suffix="" moving=latest ;;
  full) suffix=-full moving=full ;;
  *)
    echo "error: the image is '$image', not default or full" >&2
    exit 2
    ;;
esac

# A tag allows [A-Za-z0-9_.-], so a version with build metadata (+) has no
# tag.
if ! [[ "$version" =~ ^([0-9]+)\.([0-9]+)\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a version such as 1.2.3 or 1.2.3-rc.1" >&2
  exit 1
fi

echo "$version$suffix"
if [ -z "${BASH_REMATCH[3]}" ]; then
  echo "${BASH_REMATCH[1]}.${BASH_REMATCH[2]}$suffix"
  echo "$moving"
fi
