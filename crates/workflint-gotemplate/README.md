# workflint-gotemplate

The template engine behind `-format` in [workflint](https://github.com/stokehold/workflint). actionlint formats its errors with Go's `text/template`, and users copy its templates, so this crate parses the same templates and prints the same bytes. The lexer, parser, executor, and built-in functions follow Go's `text/template`, `fmt`, and `strconv` code function by function, and parse and execution errors use Go's wording and positions.

It covers the whole language: text and actions, trim markers, comments, pipelines, parentheses, fields of maps and structs, variables declared with `:=` and set with `=`, `if`, `else if`, `with`, `else with`, `range` over lists, maps, and integers with `else`, `break`, and `continue`, `define`, `template`, and `block`, and every built-in function. `printf` follows Go's `fmt`, including `%q` with `strconv.Quote`. `docs/differences.md#format-template-subset` lists the places where it differs from Go.

The caller builds the data as a `Value` from strings, numbers, booleans, lists, maps, and structs, each with the Go type name that errors print, and adds functions by name with their parameter types. actionlint's own functions, such as `json` and `toPascalCase`, belong to the `workflint` binary.

The tests in `tests/golden.rs` run 245 templates and compare the output and errors with Go 1.26.4's. `xtask/go-template-golden` writes the expected results.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.
