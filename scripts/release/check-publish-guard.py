"""Usage: check-publish-guard.py <workflow file>

Checks that no job of the release workflow can publish outside a tag run.
A job can publish when it has a write permission, an environment, or a
reference to a secret. Each such job needs an `if:` that requires a `v*` tag
ref: `startsWith(github.ref, 'refs/tags/v')` alone, or followed by `&&` and
a condition without `||`.

The attestation jobs are the exception. They run in rehearsals too, and
may write only `id-token` and `attestations`, with no environment and no
secret. The workflow itself may grant no write permission.

Prints one line per problem to standard error and exits with 1 when there is
one. The changes job of .github/workflows/release.yml runs it, and
scripts/release/test.sh tests it.
"""

import sys

import yaml

TAG_CONDITION = "startsWith(github.ref, 'refs/tags/v')"
ATTESTATION_JOBS = {"attest", "docker-attest"}
ATTESTATION_WRITES = {"id-token", "attestations"}


def writes(permissions):
    """Returns the scopes that a permissions value grants write access to."""
    if permissions == "write-all":
        return {"*"}
    if isinstance(permissions, dict):
        return {scope for scope, access in permissions.items() if access == "write"}
    return set()


def requires_tag(condition):
    """Tells whether an `if:` condition is false unless the ref is a v* tag."""
    if not isinstance(condition, str):
        return False
    text = " ".join(condition.split())
    if text.startswith("${{") and text.endswith("}}"):
        text = text[3:-2].strip()
    if text == TAG_CONDITION:
        return True
    prefix = TAG_CONDITION + " && "
    return text.startswith(prefix) and "||" not in text[len(prefix) :]


def listing(items):
    """Joins items as in "a, b, and c"."""
    if len(items) < 3:
        return " and ".join(items)
    return ", ".join(items[:-1]) + ", and " + items[-1]


def problems(workflow):
    if not isinstance(workflow, dict) or not isinstance(workflow.get("jobs"), dict):
        yield "the workflow has no jobs"
        return
    top = writes(workflow.get("permissions"))
    if top:
        yield f"the workflow grants write access to {listing(sorted(top))}; grant it per job"
    for name, job in workflow["jobs"].items():
        if not isinstance(job, dict):
            yield f"job {name} is not a mapping"
            continue
        granted = writes(job.get("permissions"))
        reasons = [f"{scope}: write" for scope in sorted(granted)]
        if "environment" in job:
            reasons.append("an environment")
        if "secrets." in yaml.safe_dump(job):
            reasons.append("a secret")
        if not reasons:
            continue
        if name in ATTESTATION_JOBS:
            extra = [r for r in reasons if r.removesuffix(": write") not in ATTESTATION_WRITES]
            if extra:
                yield f"job {name} attests, so it may write only id-token and attestations, but it has {listing(extra)}"
            continue
        if not requires_tag(job.get("if")):
            yield f"job {name} has {listing(reasons)}, but its if: does not start with {TAG_CONDITION} &&"


def main():
    if len(sys.argv) != 2:
        print("usage: check-publish-guard.py <workflow file>", file=sys.stderr)
        return 2
    path = sys.argv[1]
    try:
        with open(path, encoding="utf-8") as file:
            workflow = yaml.safe_load(file)
    except (OSError, yaml.YAMLError) as error:
        print(f"error: {path}: {error}", file=sys.stderr)
        return 1
    found = list(problems(workflow))
    for problem in found:
        print(f"error: {path}: {problem}", file=sys.stderr)
    if found:
        return 1
    print(f"{path}: every job that can publish runs only on a v* tag")
    return 0


if __name__ == "__main__":
    sys.exit(main())
