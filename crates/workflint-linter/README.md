# workflint-linter

The part of [workflint](https://github.com/stokehold/workflint) that reads the file system around the linted files. It finds the project root of a file, the nearest ancestor with a `.github/workflows` directory and a `.git` entry, as actionlint does. Its per-project cache reads and decodes each local action and local reusable workflow once per run, and gives the rules their metadata.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
