# Expression parity inputs

`cargo xtask expr-parity` runs every expression here through the reference's expression checker and through `workflint-expr`, and compares the results (ADR 0004). The Go side is `xtask/expr-check`, which the command builds inside `.reference/actionlint` with `go build -overlay`.

For each expression, both sides append `}}`, parse it, and, if it parses, check it and ask whether it is constant. The command compares:

- the parse error, with its line, column, and message, or
- the type, the constant flag, and each error with its line, column, and message, in the order the checker reports them. Errors at the same position are compared in message order. The linter sorts them that way (`docs/differences.md#sort-order`), and the reference reports the missing placeholders of `format` in Go map order, which changes between runs.

Columns count code points from the start of the expression.

## Expression sets

- `reference`: the 1,858 real-world expressions in `tests/conformance/actionlint/testdata/bench/expressions.txt`.
- `generated`: 2,000 random expressions from a fixed seed, made by `xtask/src/expr_parity/generate.rs`.
- `cases`: every `.txt` file in `cases/`, written by hand.
- `availability`: `availability.txt`, each context and special function.

A file holds one expression per line, as it is, spaces included. Empty lines and lines that start with `#` are skipped.

## Setups

`setups.toml` lists the environments of the checker. Each `[[setup]]` runs the expression sets in `expressions` with these settings:

- `name`: the name in the report and the allow-list.
- `key`: the workflow key whose contexts and special functions are allowed, such as `jobs.<job_id>.steps.run`. `*` turns the setup into one setup for each key in `workflint-data` and one for the unknown key `jobs.<job_id>.unknown`, named `name (key)`.
- `untrusted`: whether the untrusted-input check is on, as in a script. Off by default.
- `config_vars`: the configured variable names. Without it, any `vars.NAME` is allowed.
- `matrix`, `steps`, `needs`, `secrets`, `jobs`: the types of those contexts. Both checkers add the built-in secrets to `secrets`.
- `inputs`: a list of input types, added in order, as for `workflow_call` and other triggers.
- `dispatch_inputs`: the inputs of `workflow_dispatch`, added after `inputs`. They also set `github.event.inputs`.

Types use the reference's printed form: `any`, `null`, `number`, `bool`, `string`, `object` for an empty loose object, `array<T>`, `{string => T}` for a map, and `{a: T; b: U}` for a strict object.

## Allowed differences

`allowed-differences.toml` lists the differences that `docs/differences.md` explains:

```toml
[[difference]]
anchor = "int-literal-range"
setups = ["step-run"]
expression = "2147483648"
upstream = "parse error at 1:1: parsing invalid integer literal ..."
workflint = "type: number, constant: true"
```

`anchor` must exist in `docs/differences.md`. `setups` is optional. Without it, the record applies in every setup. `upstream` and `workflint` are the outcomes as the report prints them: `parse error at LINE:COLUMN: MESSAGE`, or `type: TYPE, constant: BOOL` followed by one `LINE:COLUMN: MESSAGE` line per error. A record explains a difference only if both outcomes match it exactly.

A new error that workflint reports wherever an expression has some construct, such as a `case()` predicate that is not a boolean, can change the outcome of many generated expressions. One `[[added_error]]` record explains all of them:

```toml
[[added_error]]
anchor = "case-predicate-type"
message = "case() predicate must evaluate to a boolean value but "
```

It applies to every expression in every setup. A difference is explained when the outcomes are equal once workflint's errors whose message starts with `message` are removed. Any other change in the same outcome still needs a `[[difference]]` record, and an expression that has both kinds of difference is unexplained.

The command fails on a difference that no record explains, and on a record of either kind that explains no difference.

`cases/differences.txt` holds an expression for each documented difference of the expression language that a setup can show, so that the documented outcome cannot change unnoticed.
