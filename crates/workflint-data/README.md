# workflint-data

This crate holds the facts about GitHub that workflint checks workflows against, such as the types of the contexts that `${{ }}` expressions read. It depends on no other workflint crate and reads no files, environment variables, or clocks.

Until workflint's data generators exist, the tables are ported from actionlint at commit `011a6d1`, so that workflint reports what actionlint reports.

This is an internal crate of [workflint](https://github.com/stokehold/workflint). Its API has no stability promise and can change in any release.
