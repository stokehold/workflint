# workflint-expr

The `${{ }}` expression language of [workflint](https://github.com/stokehold/workflint). It has the lexer, the parser, the type system, and the type checker so far. The lexer turns the text after `${{` into tokens up to the `}}` that ends the expression, and the parser builds a syntax tree from them with GitHub's operator precedence. Both report actionlint's error messages, with a byte range for every token, node, and error. The type system has actionlint's types of expression values, with their assignability, merge, and printed form. The type checker checks a tree against the types of the contexts at one place in a workflow and reports actionlint's errors for undefined variables and properties, property, index, and filter access, comparisons, and configuration variable names. Built-in functions, context availability, and untrusted input follow in later tasks.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
