#!/usr/bin/env bash
# Usage: homebrew-update.sh <formula> <version>
#
# Opens the pull request in stokehold/homebrew-tap that sets
# Formula/workflint.rb to <formula>, and turns on its auto-merge with the
# squash method. GH_TOKEN is a stokehold-bot token for the tap with
# contents: write and pull-requests: write.
#
# The commit goes through the contents API, so GitHub signs it and its author
# is stokehold-bot[bot]. The branch is workflint-<version>, and the pull
# request is titled `workflint <version>`. A re-run for the same version
# finishes or updates what an earlier run started:
#
# - When main already has <formula>, it does nothing.
# - When the branch exists, it commits to it, and only if its formula differs.
# - When the branch has an open pull request, it opens no second one.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: homebrew-update.sh <formula> <version>" >&2
  exit 2
fi
formula="$1"
version="$2"
tap=stokehold/homebrew-tap
path=Formula/workflint.rb
branch="workflint-$version"
title="workflint $version"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Runs `gh api <args>` and prints its output, or prints nothing when the API
# answers 404. Fails on any other error.
get() {
  if gh api "$@" > "$work/out" 2> "$work/err"; then
    cat "$work/out"
  elif ! grep -q 'HTTP 404' "$work/err"; then
    cat "$work/err" >&2
    echo "error: gh api $1 failed" >&2
    return 1
  fi
}

# The blob SHA of the formula, as the contents API reports it for a file.
blob="$(git hash-object "$formula")"

main_blob="$(get "repos/$tap/contents/$path?ref=main" --jq .sha)"
if [ "$main_blob" = "$blob" ]; then
  echo "skip: $tap already has this formula on main"
  exit 0
fi

if [ -z "$(get "repos/$tap/git/ref/heads/$branch" --jq .object.sha)" ]; then
  main_sha="$(get "repos/$tap/git/ref/heads/main" --jq .object.sha)"
  if [ -z "$main_sha" ]; then
    echo "error: $tap has no main branch" >&2
    exit 1
  fi
  echo "create: the branch $branch from main at $main_sha"
  gh api -X POST "repos/$tap/git/refs" -f "ref=refs/heads/$branch" -f "sha=$main_sha" > /dev/null
fi

branch_blob="$(get "repos/$tap/contents/$path?ref=$branch" --jq .sha)"
if [ "$branch_blob" = "$blob" ]; then
  echo "skip: $branch already has this formula"
else
  echo "commit: $path on $branch"
  content="$(base64 < "$formula" | tr -d '\n')"
  args=(-f "message=$title" -f "content=$content" -f "branch=$branch")
  if [ -n "$branch_blob" ]; then
    args+=(-f "sha=$branch_blob")
  fi
  gh api -X PUT "repos/$tap/contents/$path" "${args[@]}" > /dev/null
fi

pr="$(gh pr list --repo "$tap" --head "$branch" --state open --json number --jq '.[0].number // empty')"
if [ -n "$pr" ]; then
  echo "skip: $branch already has the open pull request #$pr"
else
  gh pr create --repo "$tap" --base main --head "$branch" --title "$title" --body-file - << EOF
Updates the workflint formula to [$version](https://github.com/stokehold/workflint/releases/tag/v$version).

The release workflow of stokehold/workflint wrote this pull request. It merges when the tests pass.
EOF
fi
gh pr merge "$branch" --repo "$tap" --auto --squash
echo "$tap has the pull request $title, with auto-merge on."
