#!/usr/bin/env bash
# Usage: test-pre-commit.sh <hook repository> <hook id>
#
# Runs the hook <hook id> of the pre-commit hook repository at the path
# <hook repository> with `pre-commit try-repo`, on the workflows of a new
# project in a temporary directory, and checks that:
#
# - a valid workflow passes,
# - a workflow with an error fails, and the output has workflint's error,
# - a YAML file outside .github/workflows/ is skipped, even with an error.
#
# try-repo includes the uncommitted changes to the tracked files of
# <hook repository>, so a pin rewritten in a working tree is the one tested.
# The caller sets up what the hook installs from: workflint on the PATH for
# the language: system hook, or PIP_FIND_LINKS and PIP_NO_INDEX for the
# language: python hook of stokehold/workflint-pre-commit. Needs pre-commit
# and git on the PATH.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: test-pre-commit.sh <hook repository> <hook id>" >&2
  exit 2
fi
repo="$(cd "$1" && pwd)"
hook="$2"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
project="$work/project"
mkdir -p "$project/.github/workflows" "$project/config"
cat > "$project/.github/workflows/valid.yml" << 'EOF'
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
EOF
cat > "$project/.github/workflows/invalid.yml" << 'EOF'
on: pushh
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
EOF
# The same error, outside the directory that the hook's files filter matches.
cp "$project/.github/workflows/invalid.yml" "$project/config/invalid.yml"
git -C "$project" init --quiet
git -C "$project" add .

# pre-commit keeps the environments it installs here, so no run reuses the
# environment of an earlier one.
export PRE_COMMIT_HOME="$work/cache"

failures=0

# Runs the hook on one file and checks its exit status and that its output
# contains a string.
# Usage: check <name> <expected status> <expected output> <file>
check() {
  local name="$1" want_status="$2" want_output="$3" file="$4"
  local output status=0
  output="$(cd "$project" && pre-commit try-repo "$repo" "$hook" --verbose --files "$file" 2>&1)" || status=$?
  if [ "$status" -ne "$want_status" ] || [[ "$output" != *"$want_output"* ]]; then
    echo "FAIL: $name"
    echo "  status: $status, expected $want_status"
    echo "  expected the output to contain: $want_output"
    echo "  output:"
    while IFS= read -r line; do echo "    $line"; done <<< "$output"
    failures=$((failures + 1))
  else
    echo "ok: $name"
  fi
}

check "$hook passes a valid workflow" 0 "Passed" .github/workflows/valid.yml
check "$hook fails a workflow with an error" 1 \
  '.github/workflows/invalid.yml:1:5: unknown Webhook event "pushh"' .github/workflows/invalid.yml
check "$hook skips a YAML file outside .github/workflows/" 0 "(no files to check)Skipped" config/invalid.yml

if [ "$failures" -ne 0 ]; then
  echo "$failures check(s) failed"
  exit 1
fi
