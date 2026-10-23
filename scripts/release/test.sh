#!/usr/bin/env bash
# Usage: test.sh
#
# Tests plan.sh, notes.sh, smoke-test.sh, package.sh, install-scripts.sh,
# compare-wheel.sh, and check-wheels.sh. The plan job of
# .github/workflows/release.yml runs it before it plans.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=0

# Runs a command and checks its exit status and its standard output.
# Usage: check <name> <expected status> <expected output> <command...>
check() {
  local name="$1" want_status="$2" want_output="$3"
  shift 3
  local output status=0
  output="$("$@" 2> "$work/stderr")" || status=$?
  if [ "$status" -ne "$want_status" ] || [ "$output" != "$want_output" ]; then
    echo "FAIL: $name"
    echo "  status: $status, expected $want_status"
    echo "  output: $(printf '%q' "$output")"
    echo "  expected: $(printf '%q' "$want_output")"
    echo "  stderr: $(cat "$work/stderr")"
    failures=$((failures + 1))
  else
    echo "ok: $name"
  fi
}

plan="$here/plan.sh"
check "a tag that matches the version is a release" 0 $'mode=release\nversion=1.2.3\npublish=false' \
  "$plan" push refs/tags/v1.2.3 1.2.3 false
check "a release publishes when the switch is on" 0 $'mode=release\nversion=1.2.3\npublish=true' \
  "$plan" push refs/tags/v1.2.3 1.2.3 true
check "a tag that does not match the version fails" 1 "" \
  "$plan" push refs/tags/v1.2.4 1.2.3 true
check "a tag without the v fails" 1 "" \
  "$plan" push refs/tags/1.2.3 1.2.3 true
check "a push of a branch fails" 1 "" \
  "$plan" push refs/heads/main 1.2.3 true
check "a pull request is a dry run that never publishes" 0 $'mode=dry-run\nversion=0.0.1\npublish=false' \
  "$plan" pull_request refs/pull/7/merge 0.0.1 true
check "a manual run on main is a rehearsal that never publishes" 0 $'mode=rehearsal\nversion=0.0.1\npublish=false' \
  "$plan" workflow_dispatch refs/heads/main 0.0.1 true
check "a manual run on another branch fails" 1 "" \
  "$plan" workflow_dispatch refs/heads/feature 0.0.1 false
check "a manual run on a tag fails" 1 "" \
  "$plan" workflow_dispatch refs/tags/v0.0.1 0.0.1 false
check "another event fails" 1 "" \
  "$plan" schedule refs/heads/main 0.0.1 false
check "an empty version fails" 1 "" \
  "$plan" pull_request refs/pull/7/merge "" false
check "a switch that is neither true nor false fails" 1 "" \
  "$plan" push refs/tags/v1.2.3 1.2.3 yes

cat > "$work/CHANGELOG.md" << 'EOF'
# Changelog

## [Unreleased]

- Not yet.

## [1.2.3](https://github.com/stokehold/workflint/compare/v1.2.2...v1.2.3) - 2026-10-23

### Fixed

- One bug.

## 1.2.2

- Another bug.

## [1.2.20] - 2026-10-01

- Not this one.
EOF
notes="$here/notes.sh"
check "the notes are the section of the version" 0 $'### Fixed\n\n- One bug.' \
  "$notes" 1.2.3 "$work/CHANGELOG.md"
check "a heading without brackets starts a section" 0 "- Another bug." \
  "$notes" 1.2.2 "$work/CHANGELOG.md"
check "a version without a section has no notes" 0 "" \
  "$notes" 1.2.4 "$work/CHANGELOG.md"
check "a prefix of a version does not match its section" 0 "" \
  "$notes" 1.2 "$work/CHANGELOG.md"
check "a missing changelog has no notes" 0 "" \
  "$notes" 1.2.3 "$work/missing.md"

printf '#!/bin/sh\nprintf "1.2.3\\ninstalled by building from source\\n"\n' > "$work/good"
printf '#!/bin/sh\nprintf "1.2.4\\n"\n' > "$work/wrong"
printf '#!/bin/sh\nexit 3\n' > "$work/failing"
chmod +x "$work/good" "$work/wrong" "$work/failing"
smoke="$here/smoke-test.sh"
check "the smoke test passes a binary of the version" 0 \
  $'1.2.3\ninstalled by building from source\nsmoke test passed: '"$work/good"' is version 1.2.3' \
  "$smoke" "$work/good" 1.2.3
check "the smoke test fails a binary of another version" 1 "1.2.4" \
  "$smoke" "$work/wrong" 1.2.3
check "the smoke test fails a binary that exits with an error" 1 "" \
  "$smoke" "$work/failing" 1.2.3

mkdir "$work/repo"
printf 'readme\n' > "$work/repo/README.md"
printf 'license\n' > "$work/repo/LICENSE"
printf 'notice\n' > "$work/repo/NOTICE"
cp "$work/good" "$work/workflint"
chmod 644 "$work/workflint"
(cd "$work/repo" && "$here/package.sh" "$work/workflint" "$work/test.tar.gz") > /dev/null
check "the archive holds the four files at its root" 0 $'LICENSE\nNOTICE\nREADME.md\nworkflint' \
  bash -c "tar -tzf '$work/test.tar.gz' | LC_ALL=C sort"
check "the binary in the archive is executable" 0 "-rwxr-xr-x" \
  bash -c "tar -tvzf '$work/test.tar.gz' workflint | cut -c1-10"
check "an archive of another format fails" 2 "" \
  bash -c "cd '$work/repo' && '$here/package.sh' '$work/workflint' '$work/test.tar.xz'"

install_scripts="$here/install-scripts.sh"
"$install_scripts" 1.2.3 "$work/scripts"
check "the release copy of install.sh differs only in its default version" 0 \
  $'< default_version=latest\n> default_version=1.2.3' \
  bash -c "diff '$here/../install.sh' '$work/scripts/install.sh' | grep '^[<>]'"
check "the release copy of install.ps1 differs only in its default version" 0 \
  $'< $DefaultVersion = \'latest\'\n> $DefaultVersion = \'1.2.3\'' \
  bash -c "diff '$here/../install.ps1' '$work/scripts/install.ps1' | grep '^[<>]'"
check "install-scripts.sh refuses a version that is not a version" 1 "" \
  "$install_scripts" "1.2.3/x" "$work/scripts-bad"

# Packs <binary> into <wheel> with the files that maturin writes, and any
# further files given after it.
# Usage: make_wheel <binary> <wheel> [extra file...]
make_wheel() {
  local binary="$1" wheel="$2"
  shift 2
  local stage="$work/stage"
  rm -rf "$stage"
  mkdir -p "$stage/workflint-1.2.3.data/scripts" "$stage/workflint-1.2.3.dist-info/licenses" \
    "$stage/workflint-1.2.3.dist-info/sboms"
  cp "$binary" "$stage/workflint-1.2.3.data/scripts/workflint"
  for name in METADATA RECORD WHEEL licenses/LICENSE licenses/NOTICE sboms/workflint.cyclonedx.json "$@"; do
    printf '%s\n' "$name" > "$stage/workflint-1.2.3.dist-info/$name"
  done
  rm -f "$wheel"
  (cd "$stage" && python3 -m zipfile -c "$wheel" workflint-1.2.3.data workflint-1.2.3.dist-info)
}

compare="$here/compare-wheel.sh"
make_wheel "$work/workflint" "$work/same.whl"
make_wheel "$work/wrong" "$work/other.whl"
make_wheel "$work/workflint" "$work/extra.whl" README.md
check "a wheel with the binary of the archive passes" 0 \
  "workflint in same.whl: $(sha256sum "$work/workflint" | cut -d ' ' -f 1)
workflint in test.tar.gz: $(sha256sum "$work/workflint" | cut -d ' ' -f 1)
The wheel holds the binary of the archive." \
  "$compare" "$work/same.whl" "$work/test.tar.gz"
check "a wheel with another binary fails" 1 \
  "workflint in other.whl: $(sha256sum "$work/wrong" | cut -d ' ' -f 1)
workflint in test.tar.gz: $(sha256sum "$work/workflint" | cut -d ' ' -f 1)" \
  "$compare" "$work/other.whl" "$work/test.tar.gz"
check "a wheel with a file that maturin does not write fails" 1 "" \
  "$compare" "$work/extra.whl" "$work/test.tar.gz"

mkdir "$work/wheels"
for tag in manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_1_x86_64 \
  manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64 \
  macosx_10_12_x86_64 macosx_11_0_arm64 win_amd64 win_arm64; do
  printf '%s\n' "$tag" > "$work/wheels/workflint-1.2.3-py3-none-$tag.whl"
done
(cd "$work/wheels" && sha256sum -- *) > "$work/recorded.sha256"
wheels="$here/check-wheels.sh"
check "six wheels with the recorded checksums pass" 0 "The wheels are complete." \
  bash -c "'$wheels' '$work/wheels' '$work/recorded.sha256' | tail -n 1"
printf 'changed\n' > "$work/wheels/workflint-1.2.3-py3-none-win_arm64.whl"
check "a wheel that differs from its recorded checksum fails" 1 "" \
  bash -c "'$wheels' '$work/wheels' '$work/recorded.sha256' > /dev/null"
rm "$work/wheels/workflint-1.2.3-py3-none-win_arm64.whl"
grep -v win_arm64 "$work/recorded.sha256" > "$work/five.sha256"
check "a missing wheel fails" 1 "" \
  bash -c "'$wheels' '$work/wheels' '$work/five.sha256' > /dev/null"
printf 'win_arm64\n' > "$work/wheels/workflint-1.2.3-py3-none-win_arm64.whl"
printf 'other\n' > "$work/wheels/workflint-1.2.3.tar.gz"
check "a file other than the six wheels fails" 1 "" \
  bash -c "'$wheels' '$work/wheels' '$work/recorded.sha256' > /dev/null"

if [ "$failures" -ne 0 ]; then
  echo "$failures tests failed"
  exit 1
fi
echo "Every test passed."
