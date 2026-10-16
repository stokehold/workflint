# workflint-syntax

This crate defines the typed AST of a GitHub Actions workflow: the workflow root, its events, jobs, and steps, and every section under them. Each key and value keeps its span and its YAML node, keyed collections keep source order, and values that may be `${{ }}` expressions tell an absent key, an invalid value, a literal, and an expression apart. The AST borrows from the span tree of `workflint-yaml`.

This is an internal crate of [workflint](https://github.com/stokehold/workflint). Its API has no stability promise and can change in any release.
