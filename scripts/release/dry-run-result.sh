#!/usr/bin/env bash
# Usage: dry-run-result.sh <packaging> <mode> <needs>
#
# Decides the result of the release-dry-run job of
# .github/workflows/release.yml, and prints its job summary: a table with one
# row per channel and the result of its jobs. <packaging> is the output of
# the changes job, <mode> the mode of the plan job, empty when plan skipped,
# and <needs> the JSON of the `needs` context.
#
# Fails when a job failed or was cancelled. When <packaging> is true, it also
# fails when a dry-run job did not succeed, and in a rehearsal or a release
# when an attestation job did not succeed. It fails when <needs> lacks a job
# of the table below or has a job that is not in it, so that a new dry-run job
# gets a row.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: dry-run-result.sh <packaging> <mode> <needs>" >&2
  exit 2
fi
packaging="$1"
mode="$2"
needs="$3"

# One channel per line: its name, then its jobs.
channels="Release scripts|plan
Archives, checksums, and SBOM|build sbom checksums
Install scripts|install-scripts
PyPI|pypi-check pypi-smoke pypi-smoke-alpine
Docker|docker-build docker-index
npm|npm-pack npm-smoke
Homebrew|homebrew
pre-commit|pre-commit
Attestations|attest docker-attest"

failed=0
fail() {
  echo "::error::$1" >&2
  failed=1
}

if ! jq -e 'type == "object"' <<< "$needs" > /dev/null 2>&1; then
  echo "error: <needs> is not a JSON object" >&2
  exit 2
fi
result() {
  jq -r --arg job "$1" '.[$job].result // "missing"' <<< "$needs"
}

changes="$(result changes)"
if [ "$changes" != success ]; then
  fail "The changes job ended with $changes."
fi
case "$packaging" in
  true | false) ;;
  *) fail "The changes job says packaging is '$packaging', not true or false." ;;
esac

known=" changes "
rows=""
while IFS='|' read -r channel jobs; do
  results=""
  for job in $jobs; do
    known+="$job "
    got="$(result "$job")"
    results+="$got "
    case "$got" in
      missing) fail "release-dry-run does not need the $job job." ;;
      failure | cancelled) fail "The $job job ended with $got." ;;
      success) ;;
      *)
        if [ "$packaging" = true ]; then
          if [ "$channel" != Attestations ] || [ "$mode" = rehearsal ] || [ "$mode" = release ]; then
            fail "The $job job ended with $got, though this run needs it."
          fi
        fi
        ;;
    esac
  done
  # The worst result of the channel's jobs.
  summary=success
  for worst in missing failure cancelled skipped; do
    if [[ " $results" == *" $worst "* ]]; then
      summary="$worst"
      break
    fi
  done
  rows+="| $channel | \`${jobs// /\`, \`}\` | $summary |"$'\n'
done <<< "$channels"

while IFS= read -r job; do
  if [[ "$known" != *" $job "* ]]; then
    fail "release-dry-run needs the $job job, which has no channel in dry-run-result.sh."
  fi
done < <(jq -r 'keys[]' <<< "$needs")

echo "## Release dry run"
echo
if [ "$packaging" = false ]; then
  echo "This pull request changes no file of scripts/release/packaging-paths.txt, so every dry-run job skipped."
  echo
fi
echo "| Channel | Jobs | Result |"
echo "| --- | --- | --- |"
printf '%s' "$rows"
echo
if [ "$failed" -ne 0 ]; then
  echo "**Failed.** The log of the release-dry-run job lists each reason."
  exit 1
fi
echo "**Passed.**"
