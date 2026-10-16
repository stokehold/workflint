# workflint-diagnostics

This crate defines the types that every workflint crate uses to report a problem: source files with a line index, spans, severities, rule metadata, diagnostics, and fixes. It also sorts and deduplicates diagnostics, formats values the way actionlint's messages do with Go's quoting rules, and writes diagnostics in actionlint's default and one-line output formats.

This is an internal crate of [workflint](https://github.com/stokehold/workflint). Its API has no stability promise and can change in any release.
