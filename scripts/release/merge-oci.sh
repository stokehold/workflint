#!/usr/bin/env bash
# Usage: merge-oci.sh <output directory> <layout>...
#
# Merges OCI image layouts, as `docker buildx build --output type=oci,tar=false`
# writes them for one platform each, into one multi-platform layout, and
# prints the digest of its image index. Each build job builds on a runner of
# its own architecture, so no single build sees both platforms.
#
# The index.json of each input must point at one image index, which lists the
# image manifest of the platform and its attestation manifests. The merged
# index lists those manifests in the order of the inputs, so the same inputs
# give the same digest. The script checks every blob against its digest and
# writes <output directory>, which must not exist.
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: merge-oci.sh <output directory> <layout>..." >&2
  exit 2
fi
out="$1"
shift
index_type=application/vnd.oci.image.index.v1+json

if [ -e "$out" ]; then
  echo "error: $out already exists" >&2
  exit 1
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/out/blobs/sha256"

: > "$work/manifests"
for layout in "$@"; do
  if ! top="$(jq -er --arg t "$index_type" \
    '.manifests | select(length == 1) | .[0] | select(.mediaType == $t) | .digest' "$layout/index.json")"; then
    echo "error: $layout/index.json does not point at exactly one image index" >&2
    exit 1
  fi
  for blob in "$layout"/blobs/sha256/*; do
    name="$(basename "$blob")"
    if [ "$(sha256sum "$blob" | cut -d ' ' -f 1)" != "$name" ]; then
      echo "error: the content of $blob does not match its digest" >&2
      exit 1
    fi
    cp "$blob" "$work/out/blobs/sha256/$name"
  done
  if [ ! -f "$layout/blobs/sha256/${top#sha256:}" ]; then
    echo "error: $layout has no blob for its index $top" >&2
    exit 1
  fi
  jq -c '.manifests[]' "$layout/blobs/sha256/${top#sha256:}" >> "$work/manifests"
done

jq -cs --arg t "$index_type" '{schemaVersion: 2, mediaType: $t, manifests: .}' "$work/manifests" \
  | tr -d '\n' > "$work/index"
digest="sha256:$(sha256sum "$work/index" | cut -d ' ' -f 1)"
size="$(wc -c < "$work/index" | tr -d ' ')"
mv "$work/index" "$work/out/blobs/sha256/${digest#sha256:}"
printf '{"imageLayoutVersion":"1.0.0"}' > "$work/out/oci-layout"
jq -cn --arg t "$index_type" --arg d "$digest" --argjson s "$size" \
  '{schemaVersion: 2, mediaType: $t, manifests: [{mediaType: $t, digest: $d, size: $s}]}' \
  | tr -d '\n' > "$work/out/index.json"
mkdir -p "$(dirname "$out")"
mv "$work/out" "$out"
echo "$digest"
