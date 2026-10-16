# workflint-expr

The `${{ }}` expression language of [workflint](https://github.com/stokehold/workflint). It has the lexer and the parser so far. The lexer turns the text after `${{` into tokens up to the `}}` that ends the expression, and the parser builds a syntax tree from them with GitHub's operator precedence. Both report actionlint's error messages, with a byte range for every token, node, and error. The type system and semantic checker follow in later tasks.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
