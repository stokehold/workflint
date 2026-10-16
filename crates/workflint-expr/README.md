# workflint-expr

The `${{ }}` expression language of [workflint](https://github.com/stokehold/workflint). It has the lexer and the type system so far. The lexer turns the text after `${{` into tokens up to the `}}` that ends the expression, with actionlint's error messages and a byte range for every token and error. The type system has actionlint's types of expression values, with their assignability, merge, and printed form. The parser and the semantic checker follow in later tasks.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
