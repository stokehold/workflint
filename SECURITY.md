# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's private vulnerability reporting on this repository (Security tab, "Report a vulnerability"). Do not open a public issue.

Include the workflint version, how you installed it, and a file or command that reproduces the problem. You will get a reply within seven days.

## Scope

These count as vulnerabilities:

- A crafted workflow, action metadata file, or config file that makes workflint execute code, write files, or make network requests.
- A release artifact, install script, package, or GitHub Action that can be tampered with without failing checksum or attestation verification.

workflint's findings about your workflows are lint results, not workflint vulnerabilities. Report a missed or wrong finding as a normal issue.

## Supported versions

Only the latest release receives security fixes.
