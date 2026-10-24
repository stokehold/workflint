#!/usr/bin/env bash
# Usage: test.sh
#
# Tests plan.sh, notes.sh, smoke-test.sh, package.sh, install-scripts.sh,
# compare-wheel.sh, check-wheels.sh, docker-tags.sh, merge-oci.sh,
# npm-pack.sh, check-npm.sh, npm-publish.sh, homebrew-formula.sh,
# homebrew-update.sh, changes.sh, dry-run-result.sh, and
# check-publish-guard.py. The plan job of .github/workflows/release.yml runs
# it before it plans. check-publish-guard.py runs with $PYTHON, or python3
# when it is unset, which needs PyYAML (requirements-pyyaml.txt).
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

tags="$here/docker-tags.sh"
check "a release tags the default image with its version, its minor version, and latest" 0 \
  $'1.2.3\n1.2\nlatest' "$tags" 1.2.3 default
check "a release tags the full image with the same tags and -full" 0 \
  $'1.2.3-full\n1.2-full\nfull' "$tags" 1.2.3 full
check "a pre-release tags the default image with its version only" 0 "1.2.3-rc.1" \
  "$tags" 1.2.3-rc.1 default
check "a pre-release tags the full image with its version only" 0 "1.2.3-rc.1-full" \
  "$tags" 1.2.3-rc.1 full
check "a version with build metadata fails" 1 "" "$tags" 1.2.3+build default
check "a version with a v fails" 1 "" "$tags" v1.2.3 default
check "a version without a patch number fails" 1 "" "$tags" 1.2 default
check "an unknown image fails" 2 "" "$tags" 1.2.3 slim

# Writes <content> as a blob of the OCI layout <dir> and prints its descriptor
# with <media type>, and with <extra> merged into it.
# Usage: blob <dir> <media type> <content> [extra]
blob() {
  local dir="$1" media="$2" content="$3" extra="${4:-{\}}" hash
  mkdir -p "$dir/blobs/sha256"
  hash="$(printf '%s' "$content" | sha256sum | cut -d ' ' -f 1)"
  printf '%s' "$content" > "$dir/blobs/sha256/$hash"
  jq -cn --arg m "$media" --arg d "sha256:$hash" --argjson s "${#content}" --argjson e "$extra" \
    '{mediaType: $m, digest: $d, size: $s} + $e'
}
# Writes an OCI layout as buildx exports a build for one platform with
# attestations: index.json points at an image index that lists the image
# manifest and its attestation manifest.
# Usage: layout <dir> <architecture>
layout() {
  local dir="$1" arch="$2" config image attestation index
  mkdir -p "$dir"
  printf '{"imageLayoutVersion":"1.0.0"}' > "$dir/oci-layout"
  config="$(blob "$dir" application/vnd.oci.image.config.v1+json "{\"architecture\":\"$arch\"}")"
  image="$(blob "$dir" application/vnd.oci.image.manifest.v1+json "{\"config\":$config}" \
    "{\"platform\":{\"architecture\":\"$arch\",\"os\":\"linux\"}}")"
  attestation="$(blob "$dir" application/vnd.oci.image.manifest.v1+json "{\"attests\":\"$arch\"}" \
    "{\"platform\":{\"architecture\":\"unknown\",\"os\":\"unknown\"},\"annotations\":{\"vnd.docker.reference.digest\":$(jq '.digest' <<< "$image")}}")"
  index="$(blob "$dir" application/vnd.oci.image.index.v1+json \
    "{\"schemaVersion\":2,\"mediaType\":\"application/vnd.oci.image.index.v1+json\",\"manifests\":[$image,$attestation]}")"
  printf '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[%s]}' "$index" \
    > "$dir/index.json"
}
merge="$here/merge-oci.sh"
layout "$work/oci/amd64" amd64
layout "$work/oci/arm64" arm64
"$merge" "$work/oci/merged" "$work/oci/amd64" "$work/oci/arm64" > "$work/oci/digest"
merged_digest="$(cat "$work/oci/digest")"
merged_index="$work/oci/merged/blobs/sha256/${merged_digest#sha256:}"
check "the merged layout points at one image index, whose digest merge-oci.sh prints" 0 \
  "$merged_digest application/vnd.oci.image.index.v1+json" \
  jq -r '.manifests | if length == 1 then "\(.[0].digest) \(.[0].mediaType)" else "\(length) manifests" end' \
  "$work/oci/merged/index.json"
check "the digest is the SHA-256 of the merged index" 0 "${merged_digest#sha256:}" \
  bash -c "sha256sum '$merged_index' | cut -d ' ' -f 1"
check "the merged index lists each image and attestation manifest in the order of the layouts" 0 \
  $'amd64\nunknown\narm64\nunknown' jq -r '.manifests[].platform.architecture' "$merged_index"
check "the merged layout holds every blob of the layouts and the new index" 0 \
  "$(cd "$work/oci" && find amd64/blobs arm64/blobs -type f -printf '%f\n' | LC_ALL=C sort -u; echo "${merged_digest#sha256:}")" \
  bash -c "find '$work/oci/merged/blobs/sha256' -type f -printf '%f\n' | LC_ALL=C sort -u | grep -vxF '${merged_digest#sha256:}'; echo '${merged_digest#sha256:}'"
check "merging the same layouts again gives the same digest" 0 "$merged_digest" \
  "$merge" "$work/oci/again" "$work/oci/amd64" "$work/oci/arm64"
cp -r "$work/oci/amd64" "$work/oci/corrupt"
printf 'changed' > "$(find "$work/oci/corrupt/blobs/sha256" -type f | head -n 1)"
check "a blob whose content does not match its digest fails" 1 "" \
  "$merge" "$work/oci/out-corrupt" "$work/oci/corrupt" "$work/oci/arm64"
mkdir -p "$work/oci/single"
cp -r "$work/oci/amd64/oci-layout" "$work/oci/amd64/blobs" "$work/oci/single/"
jq '.manifests[0].mediaType = "application/vnd.oci.image.manifest.v1+json"' \
  "$work/oci/amd64/index.json" > "$work/oci/single/index.json"
check "a layout without an image index fails" 1 "" \
  "$merge" "$work/oci/out-single" "$work/oci/single" "$work/oci/arm64"
check "an existing output directory fails" 1 "" \
  "$merge" "$work/oci/merged" "$work/oci/amd64" "$work/oci/arm64"
check "merge-oci.sh creates the parents of the output directory" 0 "$merged_digest" \
  "$merge" "$work/oci/new/parent/merged" "$work/oci/amd64" "$work/oci/arm64"

# Six archives of 1.2.3, each with a binary that prints its own platform.
mkdir -p "$work/archives" "$work/bin"
for suffix in linux_amd64 linux_arm64 darwin_amd64 darwin_arm64 windows_amd64 windows_arm64; do
  case "$suffix" in
    windows_*)
      binary="$work/bin/$suffix/workflint.exe"
      mkdir -p "$(dirname "$binary")"
      printf 'MZ %s\n' "$suffix" > "$binary"
      (cd "$(dirname "$binary")" && cp "$work/repo/README.md" "$work/repo/LICENSE" "$work/repo/NOTICE" . &&
        python3 -m zipfile -c "$work/archives/workflint_1.2.3_$suffix.zip" workflint.exe README.md LICENSE NOTICE)
      ;;
    *)
      binary="$work/bin/$suffix/workflint"
      mkdir -p "$(dirname "$binary")"
      printf '#!/bin/sh\necho %s\n' "$suffix" > "$binary"
      (cd "$work/repo" && "$here/package.sh" "$binary" "$work/archives/workflint_1.2.3_$suffix.tar.gz") > /dev/null
      ;;
  esac
done
npm_pack="$here/npm-pack.sh"
check_npm="$here/check-npm.sh"
check "npm-pack.sh writes the seven tarballs" 0 \
  $'stokehold-workflint-darwin-arm64-1.2.3.tgz\nstokehold-workflint-darwin-x64-1.2.3.tgz\nstokehold-workflint-linux-arm64-1.2.3.tgz\nstokehold-workflint-linux-x64-1.2.3.tgz\nstokehold-workflint-win32-arm64-1.2.3.tgz\nstokehold-workflint-win32-x64-1.2.3.tgz\nworkflint-1.2.3.tgz' \
  "$npm_pack" 1.2.3 "$work/archives" "$work/npm"
check "the seven tarballs pass check-npm.sh" 0 "The npm packages hold the binaries of the archives." \
  bash -c "'$check_npm' '$work/npm' '$work/archives' 1.2.3 | tail -n 1"
check "a platform package has the os and cpu of its archive" 0 \
  '{"name":"@stokehold/workflint-win32-arm64","version":"1.2.3","os":["win32"],"cpu":["arm64"],"files":["workflint.exe","NOTICE"]}' \
  bash -c "tar -xzOf '$work/npm/stokehold-workflint-win32-arm64-1.2.3.tgz' package/package.json | jq -c '{name, version, os, cpu, files}'"
check "workflint pins the six platform packages at its version" 0 \
  '{"@stokehold/workflint-linux-x64":"1.2.3","@stokehold/workflint-linux-arm64":"1.2.3","@stokehold/workflint-darwin-x64":"1.2.3","@stokehold/workflint-darwin-arm64":"1.2.3","@stokehold/workflint-win32-x64":"1.2.3","@stokehold/workflint-win32-arm64":"1.2.3"}' \
  bash -c "tar -xzOf '$work/npm/workflint-1.2.3.tgz' package/package.json | jq -c .optionalDependencies"
check "workflint has no scripts" 0 "false" \
  bash -c "tar -xzOf '$work/npm/workflint-1.2.3.tgz' package/package.json | jq 'has(\"scripts\")'"
check "npm-pack.sh refuses a version that is not a version" 1 "" \
  "$npm_pack" "1.2.3/x" "$work/archives" "$work/npm-bad"
check "npm-pack.sh fails when an archive is missing" 1 "" \
  bash -c "'$npm_pack' 1.2.4 '$work/archives' '$work/npm-missing' > /dev/null 2>&1 || exit 1"

# Repacks <tarball> after <command> changes the files in its package/
# directory, as an attacker who replaced the tarball would.
# Usage: tamper <tarball> <command>
tamper() {
  local dir="$work/tamper"
  rm -rf "$dir"
  mkdir -p "$dir"
  tar -xzf "$1" -C "$dir"
  (cd "$dir/package" && bash -c "$2")
  tar -czf "$1" -C "$dir" package
}
cp -R "$work/npm" "$work/npm-binary"
tamper "$work/npm-binary/stokehold-workflint-linux-arm64-1.2.3.tgz" "printf 'other\n' > workflint"
check "a platform package with another binary fails" 1 "" \
  bash -c "'$check_npm' '$work/npm-binary' '$work/archives' 1.2.3 > /dev/null"
cp -R "$work/npm" "$work/npm-cpu"
tamper "$work/npm-cpu/stokehold-workflint-darwin-x64-1.2.3.tgz" \
  "jq '.cpu = [\"arm64\"]' package.json > p && mv p package.json"
check "a platform package with another cpu fails" 1 "" \
  bash -c "'$check_npm' '$work/npm-cpu' '$work/archives' 1.2.3 > /dev/null"
cp -R "$work/npm" "$work/npm-mode"
tamper "$work/npm-mode/stokehold-workflint-linux-x64-1.2.3.tgz" "chmod 644 workflint"
check "a platform package whose binary is not executable fails" 1 "" \
  bash -c "'$check_npm' '$work/npm-mode' '$work/archives' 1.2.3 > /dev/null"
cp -R "$work/npm" "$work/npm-scripts"
tamper "$work/npm-scripts/workflint-1.2.3.tgz" \
  "jq '.scripts = {postinstall: \"node x.js\"}' package.json > p && mv p package.json"
check "workflint with an install script fails" 1 "" \
  bash -c "'$check_npm' '$work/npm-scripts' '$work/archives' 1.2.3 > /dev/null"
cp -R "$work/npm" "$work/npm-pin"
tamper "$work/npm-pin/workflint-1.2.3.tgz" \
  "jq '.optionalDependencies[\"@stokehold/workflint-win32-x64\"] = \"^1.2.3\"' package.json > p && mv p package.json"
check "workflint with an optional dependency at another version fails" 1 "" \
  bash -c "'$check_npm' '$work/npm-pin' '$work/archives' 1.2.3 > /dev/null"
cp -R "$work/npm" "$work/npm-extra"
printf 'x\n' > "$work/npm-extra/extra.tgz"
check "a file other than the seven tarballs fails" 1 "" \
  bash -c "'$check_npm' '$work/npm-extra' '$work/archives' 1.2.3 > /dev/null"

# A fake npm that logs each call. `npm view` lists the versions in
# $FAKE_NPM_PUBLISHED, fails with E404 for the packages in $FAKE_NPM_MISSING,
# and fails with another error when $FAKE_NPM_DOWN is set.
mkdir -p "$work/fake-npm"
cat > "$work/fake-npm/npm" << 'EOF'
#!/usr/bin/env bash
echo "npm $*" >> "$FAKE_NPM_LOG"
if [ "$1" = view ]; then
  if [ -n "${FAKE_NPM_DOWN:-}" ]; then echo "npm error code ECONNRESET" >&2; exit 1; fi
  case " $FAKE_NPM_MISSING " in *" ${2%@*} "*) echo "npm error code E404" >&2; exit 1 ;; esac
  case " $FAKE_NPM_PUBLISHED " in *" $2 "*) echo "${2##*@}" ;; esac
fi
EOF
chmod +x "$work/fake-npm/npm"
# Runs npm-publish.sh with the fake npm and prints the calls that publish.
# Usage: publish <version> [variable=value...]
publish() {
  local version="$1"
  shift
  : > "$work/npm.log"
  env PATH="$work/fake-npm:$PATH" FAKE_NPM_LOG="$work/npm.log" FAKE_NPM_PUBLISHED= FAKE_NPM_MISSING= "$@" \
    "$here/npm-publish.sh" "$work/npm" "$version" > /dev/null || return
  grep '^npm publish' "$work/npm.log" | sed "s|$work/npm/||"
}
# The six platform packages at <version>, as $FAKE_NPM_PUBLISHED lists them.
platforms_at() {
  printf '@stokehold/workflint-%s@'"$1"' ' linux-x64 linux-arm64 darwin-x64 darwin-arm64 win32-x64 win32-arm64
}
check "npm-publish.sh publishes the platform packages before workflint" 0 \
  "npm publish stokehold-workflint-linux-x64-1.2.3.tgz --provenance --tag latest --access public
npm publish stokehold-workflint-linux-arm64-1.2.3.tgz --provenance --tag latest --access public
npm publish stokehold-workflint-darwin-x64-1.2.3.tgz --provenance --tag latest --access public
npm publish stokehold-workflint-darwin-arm64-1.2.3.tgz --provenance --tag latest --access public
npm publish stokehold-workflint-win32-x64-1.2.3.tgz --provenance --tag latest --access public
npm publish stokehold-workflint-win32-arm64-1.2.3.tgz --provenance --tag latest --access public
npm publish workflint-1.2.3.tgz --provenance --tag latest" \
  publish 1.2.3
check "npm-publish.sh skips the packages that npm already has" 0 \
  "npm publish stokehold-workflint-win32-arm64-1.2.3.tgz --provenance --tag latest --access public
npm publish workflint-1.2.3.tgz --provenance --tag latest" \
  publish 1.2.3 FAKE_NPM_PUBLISHED="@stokehold/workflint-linux-x64@1.2.3 @stokehold/workflint-linux-arm64@1.2.3 @stokehold/workflint-darwin-x64@1.2.3 @stokehold/workflint-darwin-arm64@1.2.3 @stokehold/workflint-win32-x64@1.2.3 workflint@1.2.2"
check "npm-publish.sh publishes a package that npm does not have at all" 0 \
  "npm publish workflint-1.2.3.tgz --provenance --tag latest" \
  publish 1.2.3 FAKE_NPM_MISSING=workflint FAKE_NPM_PUBLISHED="$(platforms_at 1.2.3)"
check "npm-publish.sh gives a pre-release the dist-tag next" 0 \
  "npm publish workflint-1.2.3-rc.1.tgz --provenance --tag next" \
  publish 1.2.3-rc.1 FAKE_NPM_PUBLISHED="$(platforms_at 1.2.3-rc.1)"
check "npm-publish.sh stops when npm view fails for another reason" 1 "" \
  publish 1.2.3 FAKE_NPM_DOWN=1

# A checksums.txt with a line for each of the six archives and two other
# assets. Each SHA-256 is one hex digit repeated.
cat > "$work/brew-checksums.txt" << 'EOF'
1111111111111111111111111111111111111111111111111111111111111111  checksums-not-this.txt
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  workflint_1.2.3_darwin_amd64.tar.gz
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  workflint_1.2.3_darwin_arm64.tar.gz
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  workflint_1.2.3_linux_amd64.tar.gz
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  workflint_1.2.3_linux_arm64.tar.gz
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  workflint_1.2.3_windows_amd64.zip
ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff  workflint_1.2.3_windows_arm64.zip
EOF
formula="$here/homebrew-formula.sh"
release_url=https://github.com/stokehold/workflint/releases/download/v1.2.3
"$formula" 1.2.3 "$work/brew-checksums.txt" "$release_url" > "$work/workflint.rb"
check "the formula has the version" 0 '  version "1.2.3"' \
  grep '^  version ' "$work/workflint.rb"
check "the formula has the URL and SHA-256 of each macOS and Linux archive" 0 \
  "url \"$release_url/workflint_1.2.3_darwin_arm64.tar.gz\"
sha256 \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"
url \"$release_url/workflint_1.2.3_darwin_amd64.tar.gz\"
sha256 \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"
url \"$release_url/workflint_1.2.3_linux_arm64.tar.gz\"
sha256 \"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\"
url \"$release_url/workflint_1.2.3_linux_amd64.tar.gz\"
sha256 \"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"" \
  bash -c "grep -E '^ +(url|sha256) ' '$work/workflint.rb' | sed 's/^ *//'"
check "the formula has no placeholder left" 1 "" \
  grep '@[A-Z0-9_]*@' "$work/workflint.rb"
check "the formula sets WORKFLINT_INSTALLED_FROM" 0 \
  '    (bin/"workflint").write_env_script libexec/"workflint", WORKFLINT_INSTALLED_FROM: "installed via Homebrew"' \
  grep 'WORKFLINT_INSTALLED_FROM' "$work/workflint.rb"
check "the formula takes a file URL for a dry run" 0 \
  "  url \"file://$work/dist/workflint_1.2.3_linux_amd64.tar.gz\"" \
  bash -c "'$formula' 1.2.3 '$work/brew-checksums.txt' 'file://$work/dist' | grep -F linux_amd64 | sed 's/^  *url/  url/'"
grep -v linux_arm64 "$work/brew-checksums.txt" > "$work/brew-missing.txt"
check "the formula fails when checksums.txt has no line for an archive" 1 "" \
  "$formula" 1.2.3 "$work/brew-missing.txt" "$release_url"
{ cat "$work/brew-checksums.txt"; grep linux_arm64 "$work/brew-checksums.txt"; } > "$work/brew-twice.txt"
check "the formula fails when checksums.txt lists an archive twice" 1 "" \
  "$formula" 1.2.3 "$work/brew-twice.txt" "$release_url"
sed 's/^bbbb/BBBB/' "$work/brew-checksums.txt" > "$work/brew-upper.txt"
check "the formula fails on a SHA-256 that is not 64 lowercase hex digits" 1 "" \
  "$formula" 1.2.3 "$work/brew-upper.txt" "$release_url"
check "the formula fails on a version that is not a version" 1 "" \
  "$formula" '1.2.3"' "$work/brew-checksums.txt" "$release_url"
check "the formula fails on a base URL with a trailing slash" 1 "" \
  "$formula" 1.2.3 "$work/brew-checksums.txt" "$release_url/"
check "the formula fails on a base URL that is neither https nor file" 1 "" \
  "$formula" 1.2.3 "$work/brew-checksums.txt" http://example.com/v1.2.3
check "the formula fails on a base URL with a character sed would read" 1 "" \
  "$formula" 1.2.3 "$work/brew-checksums.txt" "https://example.com/a|b"

# A fake gh that logs each call and answers as the GitHub API would for the
# tap. $FAKE_GH_MAIN and $FAKE_GH_BRANCH hold the blob SHA of
# Formula/workflint.rb on main and on the branch, or are empty for no file.
# The branch exists when $FAKE_GH_HAS_BRANCH is set. $FAKE_GH_PR is the
# number of the open pull request of the branch, or empty. Any call fails
# with HTTP 500 when $FAKE_GH_DOWN is set.
mkdir -p "$work/fake-gh"
cat > "$work/fake-gh/gh" << 'EOF'
#!/usr/bin/env bash
echo "gh $*" >> "$FAKE_GH_LOG"
if [ -n "${FAKE_GH_DOWN:-}" ]; then echo "gh: Server Error (HTTP 500)" >&2; exit 1; fi
not_found() { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
answer() { if [ -n "$1" ]; then echo "$1"; else not_found; fi; }
case "$*" in
  "api repos/stokehold/homebrew-tap/contents/Formula/workflint.rb?ref=main --jq .sha") answer "$FAKE_GH_MAIN" ;;
  "api repos/stokehold/homebrew-tap/contents/Formula/workflint.rb?ref=workflint-"*" --jq .sha") answer "$FAKE_GH_BRANCH" ;;
  "api repos/stokehold/homebrew-tap/git/ref/heads/main --jq .object.sha") echo 0123abcd ;;
  "api repos/stokehold/homebrew-tap/git/ref/heads/workflint-"*) answer "${FAKE_GH_HAS_BRANCH:+4567cdef}" ;;
  "pr list "*) echo "$FAKE_GH_PR" ;;
  "pr create "*) echo "https://github.com/stokehold/homebrew-tap/pull/9" ;;
esac
EOF
chmod +x "$work/fake-gh/gh"
printf 'class Workflint < Formula\nend\n' > "$work/update.rb"
update_blob="$(git hash-object "$work/update.rb")"
# Runs homebrew-update.sh with the fake gh and prints the calls that change
# the tap, with the content of the file left out.
# Usage: update [variable=value...]
update() {
  : > "$work/gh.log"
  env PATH="$work/fake-gh:$PATH" FAKE_GH_LOG="$work/gh.log" FAKE_GH_MAIN= FAKE_GH_BRANCH= \
    FAKE_GH_HAS_BRANCH= FAKE_GH_PR= "$@" \
    "$here/homebrew-update.sh" "$work/update.rb" 1.2.3 > /dev/null || return
  { grep -E '^gh (api -X|pr (create|merge))' "$work/gh.log" || true; } | sed 's/ -f content=[^ ]*//'
}
check "homebrew-update.sh creates the branch, commits the formula, and opens a pull request" 0 \
  "gh api -X POST repos/stokehold/homebrew-tap/git/refs -f ref=refs/heads/workflint-1.2.3 -f sha=0123abcd
gh api -X PUT repos/stokehold/homebrew-tap/contents/Formula/workflint.rb -f message=workflint 1.2.3 -f branch=workflint-1.2.3
gh pr create --repo stokehold/homebrew-tap --base main --head workflint-1.2.3 --title workflint 1.2.3 --body-file -
gh pr merge workflint-1.2.3 --repo stokehold/homebrew-tap --auto --squash" \
  update
check "homebrew-update.sh commits the content of the formula" 0 "$(cat "$work/update.rb")" \
  bash -c "sed -n 's/^gh api -X PUT .* -f content=\\([^ ]*\\) .*/\\1/p' '$work/gh.log' | base64 -d"
check "homebrew-update.sh replaces an older formula on main" 0 \
  "gh api -X POST repos/stokehold/homebrew-tap/git/refs -f ref=refs/heads/workflint-1.2.3 -f sha=0123abcd
gh api -X PUT repos/stokehold/homebrew-tap/contents/Formula/workflint.rb -f message=workflint 1.2.3 -f branch=workflint-1.2.3 -f sha=0ld0
gh pr create --repo stokehold/homebrew-tap --base main --head workflint-1.2.3 --title workflint 1.2.3 --body-file -
gh pr merge workflint-1.2.3 --repo stokehold/homebrew-tap --auto --squash" \
  update FAKE_GH_MAIN=0ld0 FAKE_GH_BRANCH=0ld0
check "homebrew-update.sh updates the branch and the pull request of an earlier run" 0 \
  "gh api -X PUT repos/stokehold/homebrew-tap/contents/Formula/workflint.rb -f message=workflint 1.2.3 -f branch=workflint-1.2.3 -f sha=0ld1
gh pr merge workflint-1.2.3 --repo stokehold/homebrew-tap --auto --squash" \
  update FAKE_GH_MAIN=0ld0 FAKE_GH_HAS_BRANCH=1 FAKE_GH_BRANCH=0ld1 FAKE_GH_PR=9
check "homebrew-update.sh commits nothing when the branch already has the formula" 0 \
  "gh pr create --repo stokehold/homebrew-tap --base main --head workflint-1.2.3 --title workflint 1.2.3 --body-file -
gh pr merge workflint-1.2.3 --repo stokehold/homebrew-tap --auto --squash" \
  update FAKE_GH_MAIN=0ld0 FAKE_GH_HAS_BRANCH=1 FAKE_GH_BRANCH="$update_blob"
check "homebrew-update.sh does nothing when main already has the formula" 0 "" \
  update FAKE_GH_MAIN="$update_blob"
check "homebrew-update.sh stops when the API fails for another reason" 1 "" \
  update FAKE_GH_DOWN=1

changes="$here/changes.sh"
paths="$here/packaging-paths.txt"
check "a pull request that changes only docs touches no packaging file" 0 "packaging=false" \
  "$changes" "$paths" <<< $'docs/releasing.md\ndocs/adr/README.md'
check "a pull request that changes Cargo.lock touches packaging" 0 "packaging=true" \
  "$changes" "$paths" <<< $'docs/releasing.md\nCargo.lock'
check "a file deep under a ** path matches it" 0 "packaging=true" \
  "$changes" "$paths" <<< "crates/workflint/src/cli/args.rs"
check "another crate is not packaging" 0 "packaging=false" \
  "$changes" "$paths" <<< "crates/workflint-rules/src/lib.rs"
check "a Cargo.toml outside the root and crates/workflint is not packaging" 0 "packaging=false" \
  "$changes" "$paths" <<< "fuzz/Cargo.toml"
check "the release workflow is packaging" 0 "packaging=true" \
  "$changes" "$paths" <<< ".github/workflows/release.yml"
check "no changed file is not packaging" 0 "packaging=false" \
  "$changes" "$paths" < /dev/null
printf '# only a comment\n\n' > "$work/no-paths.txt"
check "a paths file without paths fails" 1 "" \
  "$changes" "$work/no-paths.txt" <<< "Cargo.lock"

result="$here/dry-run-result.sh"
dry_run_jobs=(plan build sbom checksums install-scripts pypi-check pypi-smoke pypi-smoke-alpine
  docker-build docker-index npm-pack npm-smoke homebrew pre-commit)
# Prints the needs context in which changes succeeded and each job has the
# result <default>, or the result given as <job>=<result>.
# Usage: needs <default> [job=result...]
needs() {
  local default="$1" job
  shift
  {
    echo "changes success"
    for job in "${dry_run_jobs[@]}"; do echo "$job $default"; done
    echo "attest skipped"
    echo "docker-attest skipped"
    for job in "$@"; do echo "${job%%=*} ${job#*=}"; done
  } | jq -R -n '[inputs | split(" ") | {key: .[0], value: {result: .[1], outputs: {}}}] | from_entries'
}
# Runs dry-run-result.sh and prints its status and its summary row for <row>.
# Usage: verdict <row> <packaging> <mode> <needs>
verdict() {
  local row="$1" status=0 output
  shift
  output="$("$result" "$@" 2> /dev/null)" || status=$?
  echo "status=$status"
  grep -F "| $row |" <<< "$output" | sed 's/.* | //; s/ |$//'
}
check "release-dry-run passes a pull request without packaging changes" 0 $'status=0\nskipped' \
  verdict PyPI false "" "$(needs skipped plan=skipped)"
check "release-dry-run passes a dry run in which every job passed" 0 $'status=0\nsuccess' \
  verdict PyPI true dry-run "$(needs success)"
check "release-dry-run shows the attestations as skipped in a dry run" 0 $'status=0\nskipped' \
  verdict Attestations true dry-run "$(needs success)"
check "release-dry-run fails when one dry-run job failed" 0 $'status=1\nfailure' \
  verdict PyPI true dry-run "$(needs success pypi-smoke=failure)"
check "release-dry-run fails when a job was cancelled" 0 $'status=1\ncancelled' \
  verdict Docker true dry-run "$(needs success docker-index=cancelled)"
check "release-dry-run fails when a dry-run job skipped in a packaging change" 0 $'status=1\nskipped' \
  verdict Homebrew true dry-run "$(needs success homebrew=skipped)"
check "release-dry-run fails a rehearsal whose attestations skipped" 0 $'status=1\nskipped' \
  verdict Attestations true rehearsal "$(needs success)"
check "release-dry-run passes a rehearsal whose attestations passed" 0 $'status=0\nsuccess' \
  verdict Attestations true rehearsal "$(needs success attest=success docker-attest=success)"
check "release-dry-run fails when the changes job failed" 0 $'status=1\nskipped' \
  verdict npm "" "" "$(needs skipped changes=failure)"
check "release-dry-run fails when it needs a job without a channel" 0 $'status=1\nsuccess' \
  verdict npm true dry-run "$(needs success new-channel=success)"
check "release-dry-run fails when it does not need a job of a channel" 0 $'status=1\nmissing' \
  verdict npm true dry-run "$(needs success | jq 'del(.["npm-smoke"])')"
check "release-dry-run lists every channel" 0 \
  "Release scripts
Archives, checksums, and SBOM
Install scripts
PyPI
Docker
npm
Homebrew
pre-commit
Attestations" \
  bash -c "'$result' true dry-run '$(needs success)' | sed -n 's/^| \([^|]*\) | \`.*/\1/p'"

python="${PYTHON:-python3}"
guard="$here/check-publish-guard.py"
workflow="$here/../../.github/workflows/release.yml"
# Runs the guard on a copy of release.yml that a sed script changed, and
# prints its status and the jobs it reports.
# Usage: guard_on <sed script>
guard_on() {
  local status=0
  sed "$1" "$workflow" > "$work/release.yml"
  "$python" "$guard" "$work/release.yml" > /dev/null 2> "$work/guard.err" || status=$?
  echo "status=$status"
  sed -n 's/.*: job \([^ ]*\) .*/\1/p; s/.*: the workflow grants.*/workflow/p' "$work/guard.err"
}
if ! "$python" -c 'import yaml' 2> /dev/null; then
  echo "FAIL: $python has no PyYAML. Install scripts/release/requirements-pyyaml.txt and set PYTHON."
  failures=$((failures + 1))
else
  check "the publishing guard passes release.yml" 0 "status=0" guard_on ""
  check "the publishing guard fails a publish job without the tag condition" 0 $'status=1\nrelease' \
    guard_on "/^  release:\$/,/^  pypi-check:\$/ s/startsWith(github.ref, 'refs\\/tags\\/v') && //"
  check "the publishing guard fails a tag condition that || can bypass" 0 $'status=1\nnpm-publish' \
    guard_on "/^  npm-publish:\$/,/^  homebrew:\$/ s/publish == 'true'\$/publish == 'true' || always()/"
  check "the publishing guard accepts the tag condition inside \${{ }}" 0 "status=0" \
    guard_on "/^  docker-publish:\$/,/^  npm-pack:\$/ s/^    if: \\(.*\\)\$/    if: \${{ \\1 }}/"
  check "the publishing guard fails a job with an environment and no if" 0 $'status=1\nsneaky' \
    guard_on "\$a\\
\\
  sneaky:\\
    runs-on: ubuntu-24.04\\
    environment: release\\
    steps:\\
      - run: echo"
  check "the publishing guard fails a job that reads a secret" 0 $'status=1\nleaky' \
    guard_on "\$a\\
\\
  leaky:\\
    runs-on: ubuntu-24.04\\
    permissions: {}\\
    steps:\\
      - env:\\
          TOKEN: \${{ secrets.STOKEHOLD_BOT_PRIVATE_KEY }}\\
        run: echo"
  check "the publishing guard fails an attestation job that writes contents" 0 $'status=1\nattest' \
    guard_on "/^  attest:\$/,/^  gate:\$/ s/^      id-token: write.*/&\\
      contents: write/"
  check "the publishing guard fails write permissions for the whole workflow" 0 $'status=1\nworkflow' \
    guard_on "s/^permissions: {}\$/permissions: write-all/"
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures tests failed"
  exit 1
fi
echo "Every test passed."
