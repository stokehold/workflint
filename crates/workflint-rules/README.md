# workflint-rules

The rules of [workflint](https://github.com/stokehold/workflint) and the registry that lists them. A visitor runs the rules over a parsed workflow in actionlint's order: the workflow-pre hook, then for each job the job-pre hook, the step hook for each step, and the job-post hook, then the workflow-post hook. The registry lists the rule metadata of every rule, including the rules that the YAML layer, the workflow parser, and the expression checker report, and the description of each actionlint kind.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
