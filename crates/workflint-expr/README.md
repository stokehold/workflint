# workflint-expr

The `${{ }}` expression language of [workflint](https://github.com/stokehold/workflint). It has the lexer so far: it turns the text after `${{` into tokens up to the `}}` that ends the expression, with actionlint's error messages and a byte range for every token and error. The parser, type system, and semantic checker follow in later tasks.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
