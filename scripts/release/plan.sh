#!/usr/bin/env bash
# Usage: plan.sh <event name> <ref> <version> <publish>
#
# Decides how .github/workflows/release.yml runs, and prints `mode=<mode>`,
# `version=<version>`, and `publish=<true or false>` in the format of
# $GITHUB_OUTPUT. <version> is the version of the workflint package in
# Cargo.toml. The modes are:
#
#   release    A push of the tag v<version>.
#   dry-run    A pull request.
#   rehearsal  A manual run on main.
#
# <publish> is the PUBLISH switch of the workflow, `true` or `false`. The
# output `publish` is true only for a release with the switch on, and only
# then does the release job create and publish a GitHub release.
#
# Fails for a tag that is not v<version>, for a manual run on another branch,
# and for any other event, so that no later job runs.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: plan.sh <event name> <ref> <version> <publish>" >&2
  exit 2
fi
event="$1"
ref="$2"
version="$3"
switch="$4"

if [ -z "$version" ]; then
  echo "error: the workflint package has no version" >&2
  exit 1
fi
case "$switch" in
  true | false) ;;
  *)
    echo "error: the publish switch is '$switch', not true or false" >&2
    exit 1
    ;;
esac

case "$event" in
  push)
    case "$ref" in
      refs/tags/*) ;;
      *)
        echo "error: a push of $ref is not a release; only tags start one" >&2
        exit 1
        ;;
    esac
    tag="${ref#refs/tags/}"
    if [ "$tag" != "v$version" ]; then
      echo "error: the tag $tag does not match the version $version in Cargo.toml, which needs the tag v$version" >&2
      exit 1
    fi
    mode=release
    ;;
  pull_request)
    mode=dry-run
    ;;
  workflow_dispatch)
    if [ "$ref" != refs/heads/main ]; then
      echo "error: a rehearsal runs on main, not on $ref" >&2
      exit 1
    fi
    mode=rehearsal
    ;;
  *)
    echo "error: the release workflow does not run on $event" >&2
    exit 1
    ;;
esac

publish=false
if [ "$mode" = release ] && [ "$switch" = true ]; then
  publish=true
fi
if [ "$mode" = release ] && [ "$publish" = false ]; then
  echo "notice: publishing is off, so the release job skips $tag" >&2
fi

echo "mode=$mode"
echo "version=$version"
echo "publish=$publish"
