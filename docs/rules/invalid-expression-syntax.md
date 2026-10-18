# invalid-expression-syntax

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression in `${{ }}` cannot be lexed or parsed.

The lexer reports a character that no token starts with, such as `"` or `+`. The parser reports a token where the grammar has no place for it, and input that ends before the expression does, such as a missing `)`. Every `${{ }}` in a string is checked, so one string can give several of these errors.

## Why

GitHub rejects a workflow with an expression that it cannot parse. Expressions have no `+` operator and quote strings only with `'`, and both mistakes are common in code copied from other languages.

## Failing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "${{ "hello" }}"
      - run: echo '${{ 1 + 1 }}'
```

```text
test.yaml:6:24: got unexpected character '"' while lexing expression, expecting 'a'..'z', 'A'..'Z', '_', '0'..'9', ''', '}', '(', ')', '[', ']', '.', '!', '<', '>', '=', '&', '|', '*', ',', ' '. do you mean string literals? only single quotes are available for string delimiter [invalid-expression-syntax]
test.yaml:7:26: got unexpected character '+' while lexing expression, expecting 'a'..'z', 'A'..'Z', '_', '0'..'9', ''', '}', '(', ')', '[', ']', '.', '!', '<', '>', '=', '&', '|', '*', ',', ' ' [invalid-expression-syntax]
```

An expression that ends early is reported at its end, and the next expression of the string is still checked:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ toJSON(hashFiles('**/lock') }} ${{ github.event. }}'
```

```text
test.yaml:6:52: unexpected end of input while parsing arguments of function call. expecting ",", ")" [invalid-expression-syntax]
test.yaml:6:73: unexpected end of input while parsing object property dereference like 'a.b' or array element dereference like 'a.*'. expecting "IDENT", "*" [invalid-expression-syntax]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ 'hello' }} ${{ 1e+5 }} ${{ toJSON(hashFiles('**/lock')) }}'
```

## Differences from actionlint

- [`+` in a number's exponent](../differences.md#exponent-plus-sign)
- [Nesting limit for expressions](../differences.md#expression-nesting-limit)
- [Every `${{ }}` in a string is checked](../differences.md#all-expressions-in-string)
