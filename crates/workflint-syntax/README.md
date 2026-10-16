# workflint-syntax

This crate defines the typed AST of a GitHub Actions workflow: the workflow root, its events, jobs, and steps, and every section under them. Each key and value keeps its span and its YAML node, keyed collections keep source order, and values that may be `${{ }}` expressions tell an absent key, an invalid value, a literal, and an expression apart. The AST borrows from the span tree of `workflint-yaml`.

The crate also decodes the metadata of local actions and local reusable workflows, `ActionMetadata` and `ReusableWorkflowMetadata`, with the typed decoding rules of the reference: YAML 1.1 bools, exact duplicate keys rejected, unknown keys ignored, and the reference's error texts.

The `glob` module validates branch, tag, and path filter patterns and returns each problem with its message and its byte range in the pattern. It does not depend on the AST.

This is an internal crate of [workflint](https://github.com/stokehold/workflint). Its API has no stability promise and can change in any release.
