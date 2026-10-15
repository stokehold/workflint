# 02 — The `${{ }}` expression language engine (actionlint → Rust)

Upstream snapshot: `.reference/actionlint` at commit `011a6d1`. All `file:line` references point into that tree.
Files covered: `expr.go`, `expr_ast.go`, `expr_lexer.go`, `expr_parser.go`, `expr_type.go`, `expr_sema.go`,
`expr_insecure.go`, `availability.go`, `rule_expression.go`, `rule_if_cond.go`, their `_test.go` files, and
`scripts/generate-availability`, `scripts/generate-webhook-events`.

Every behaviour marked **(verified)** was checked by building the upstream code (`go build ./cmd/actionlint`,
plus a small probe program linking the package) and running it on crafted input. The probe lives in a
scratch directory outside the reference tree.

---

## 0. Architecture at a glance

```
YAML string (*String{Value, Quoted, Pos})
  └─ RuleExpression.checkExprsIn()            rule_expression.go:734
       loop: find "${{" → hand the REST of the string to a fresh lexer
         ExprLexer (text/scanner based)       expr_lexer.go
         ExprParser (recursive descent)       expr_parser.go     → ExprNode tree (expr_ast.go)
         ExprSemanticsChecker (new per expr)  expr_sema.go       → ExprType (expr_type.go) + []*ExprError
           ├─ context availability           availability.go (generated)
           └─ UntrustedInputChecker          expr_insecure.go (only for run:/github-script script)
       errors → convertExprLineColToPos() → rule.Error()
RuleIfCond (separate rule, re-lexes/re-parses if: conditions)  rule_if_cond.go
```

`ExprError` (`expr.go:7-20`) holds `Message`, `Offset` (byte, 0-based), `Line`, `Column` (1-based).
`Error()` prints `"%d:%d:%d: %s"` (line:col:offset: msg). Only the tests use that format. Rules use `Message`
together with Line/Column.

Final error ordering doesn't depend on emission order. `linter.go:638` sorts all errors with `compareErrors`
(`error.go:161-172`): filepath, then line, then column, then message. That's a total order, so the Rust port
only has to produce the same *set* of errors.

---

## 1. Lexer (`expr_lexer.go`)

### 1.1 Mechanics
- It's built on Go's `text/scanner` (`expr_lexer.go:158-180`) and uses only `Peek()`/`Next()`/`Pos()`, never `Scan()`.
  Two consequences matter for compatibility:
  - **Columns count runes (characters), offsets count bytes.** `scanner.Position.Column` is "character count per line".
    The lexer test `TestLexTokenPos` (`expr_lexer_test.go:820`) checks offset/column on ASCII only. **(verified)**:
    for `'あ' == 'い' || x`, the error for `x` is reported at col 15, offset 18.
  - Lines increment on `\n`. Expressions can span lines (`3:4:13` for a `?` on line 3) **(verified)**.
  - text/scanner itself raises errors for NUL and invalid UTF‑8. They are wrapped as
    `scan error while lexing expression: invalid character NUL` / `... invalid UTF-8 encoding` (`expr_lexer.go:176-178`) **(verified)**.
    A BOM at offset 0 is silently skipped by text/scanner but still ends up inside the token's `Value`. For example `﻿1` gives
    `parsing invalid integer literal "﻿1": strconv.ParseInt: ...` **(verified)**. This edge case isn't worth reproducing.
- Only the **first** lexer error is kept (`error()` at `expr_lexer.go:182-192`). Its position is `scan.Pos()` at the
  moment of the error, i.e. the offending char (or end of input for EOF).
- After an error, every `Next()` returns an `End` token positioned at `lex.start` (`eof()`, `expr_lexer.go:208-216`). The
  parser therefore sees "end of input". `ExprParser.Err()` returns the lexer error first (`expr_parser.go:325-330`), which masks the parser error.
- Whitespace is exactly `' ' '\n' '\r' '\t'` (`expr_lexer.go:131-133`). `skipWhite` moves `start` past it.
- **There is no "end of input" success state.** An expression must be terminated by the `}}` token (`TokenKindEnd`,
  value `"}}"`). Plain EOF is the error `unexpected EOF while lexing expression` (`expr_lexer.go:258-261, 466-469`). Callers append
  `"}}"` when they lex a bare expression (if: conditions, tests). `Offset()` (`expr_lexer.go:518`) returns the byte offset just past
  the `}}`. That's how the caller learns where the expression ended inside the host string.

### 1.2 Token kinds (`expr_lexer.go:13-59`, names from `String()` 61-110)
| Kind | String() | Lexeme |
|---|---|---|
| Unknown | `UNKNOWN` | — |
| End | `END` | `}}` |
| Ident | `IDENT` | `[A-Za-z_][A-Za-z0-9_-]*` (`expr_lexer.go:263-271, 473`) |
| String | `STRING` | `'…'` with `''` escape |
| Int | `INTEGER` | decimal or hex, optional leading `-` |
| Float | `FLOAT` | fraction and/or exponent |
| `(` `)` `[` `]` `.` `!` `<` `<=` `>` `>=` `==` `!=` `&&` `\|\|` `*` `,` | same text | |

`true`/`false`/`null` are lexed as `IDENT`. The parser turns them into keywords (§2).

### 1.3 Numbers (`lexNum` 273-350, `lexHexInt` 352-378)
Dispatch: a digit **or `-`** starts a number (`expr_lexer.go:477`). Grammar actually accepted:
```
number  := '-'? ( '0' ( 'x' hex )? | [1-9][0-9]* ) ( '.' [0-9]+ )? ( [eE] '-'? ( '0' | [1-9][0-9]* ) )?
hex     := '0' | [1-9a-fA-F][0-9a-fA-F]*          (note: after 0x, a leading 0 must stand alone)
```
- A `.fraction` or exponent makes it `FLOAT`. Otherwise it's `INTEGER`. The hex form is always `INTEGER`.
- No `+` exponent sign (`1E+5` is an error), no `0X`, no leading `.` (`.5` gives a parser error), no `0o`/`0b`, no `NaN`/`Infinity` **(verified)**.
- `-` must be followed *immediately* by a digit. `- 1` and `-a` are errors, and `1-2` lexes as `1` then `-2` (a parser "remaining tokens" error) **(verified)**.
- After a number, an alnum char (`[A-Za-z0-9]`) is an error, but `_` and `-` are not. So `1_x` lexes as `1`, `_x` (parser error).
- Leading zeros: `0123` → `got unexpected character '1' while lexing character following number 0`. `1e01` → `... following number 1e0`. `0x0123` → `... following hex integer 0x0`.
- Integer values are parsed by the **parser** with `strconv.ParseInt(v, 0, 32)` (`expr_parser.go:142`), so anything beyond int32
  is a parse error: `parsing invalid integer literal "2147483648": strconv.ParseInt: parsing "2147483648": value out of range`
  **(verified)**. GitHub numbers are doubles, so this is a false positive. Floats use `ParseFloat(v, 64)` (`1e400` → `value out of range`).

### 1.4 String literals (`lexString` 380-392)
- Delimiter is only `'`. `''` inside a string is an escaped quote. Any other char is allowed, including newlines, `}}`, and non-ASCII.
- EOF before the closing quote: `got unexpected EOF while lexing end of string literal, expecting '''`.
- A `"` at token start gets a special note (`expr_lexer.go:241-244`):
  `got unexpected character '"' while lexing expression, expecting <all chars>. do you mean string literals? only single quotes are available for string delimiter`.
- The parser strips the outer quotes and replaces `''` with `'` (`expr_parser.go:166-172`). The token `Value` keeps the raw text.

### 1.5 `}}` boundary detection
- Inside the lexer, `}` must be followed by `}`. Otherwise you get `got unexpected character 'x' while lexing end marker }}, expecting '}'`
  (`expr_lexer.go:394-402`). Because string literals are lexed as whole tokens, `}}` inside `'…'` does **not** end the
  expression: `${{ '}}' }}` works **(verified)**.
- In the host string, `checkExprsIn` (`rule_expression.go:734-768`) finds `${{` with plain `strings.Index` and hands
  **the whole remainder** of the string to a fresh lexer. The lexer stops at the first `End` token, and `l.Offset()` says how
  far to skip. There's no escape for a literal `${{`.
- Other helpers use naive substring logic that ignores quoting:
  - `ContainsExpression(s)` (`ast.go:45-50`): `i := Index(s,"${{"); i >= 0 && i < Index(s,"}}")`.
  - `isExprAssigned(s)` (`ast.go:57-65`): after TrimSpace, has prefix `${{`, suffix `}}`, and exactly one `${{`. The comment explains why the number of
    `}}` isn't counted (JSON in strings).
  - `RuleIfCond.checkIfCond` uses `Index(v,"}}")`, i.e. the **first** `}}`. That's a false positive for `if: ${{ github.ref == '}}' }}`
    (reported as "always evaluated to true") **(verified)**.

### 1.6 Operators and punctuation (`expr_lexer.go:404-458, 481-514`)
`<`/`<=`, `>`/`>=`, `!`/`!=` are greedy. `=` must be `==`, `&` must be `&&`, `|` must be `||`.

### 1.7 Lexer error messages (exact)
Generic template (`expr_lexer.go:246-252`):
```
got unexpected {what} while lexing {where}, expecting {expected}{note}
  what     = "EOF" | "character " + strconv.QuoteRune(r)      e.g. character '?' , character '\''
  note     = ". do you mean string literals? only single quotes are available for string delimiter"  (only when r == '"')
```
| where | expected |
|---|---|
| `integer part of number` | `'0'..'9'` |
| `fraction part of float number` | `'0'..'9'` |
| `exponent part of float number` | `'0'..'9'` |
| `character following number {lexeme-so-far}` | punct list |
| `hex integer` | `'0'..'9', 'a'..'f', 'A'..'F'` |
| `character following hex integer {lexeme-so-far}` | punct list |
| `end of string literal` | `'''` |
| `end marker }}` | `'}'` |
| `== operator` | `'='` |
| `&& operator` | `'&'` |
| `\|\| operator` | `'\|'` |
| `expression` | all chars |

Constants (`expr_lexer.go:151-154`):
```
punct = ''', '}', '(', ')', '[', ']', '.', '!', '<', '>', '=', '&', '|', '*', ',', ' '
digit = '0'..'9'
alpha = 'a'..'z', 'A'..'Z', '_'
all   = alpha + ", " + digit + ", " + punct
```
Other messages: `unexpected EOF while lexing expression` and `scan error while lexing expression: {text/scanner msg}`.
Note that `QuoteRune` uses Go escaping (`'\''`, `'\x00'`, `'﻿'`). Unicode printable chars are kept as-is (`'ａ'`).

---

## 2. Parser (`expr_parser.go`)

### 2.1 Grammar (as implemented)
```
Parse      := LogicalOr  END                          (Parse 333-362)
LogicalOr  := LogicalAnd ( '||' LogicalOr )?          right-recursive (308-322)
LogicalAnd := Compare    ( '&&' LogicalAnd )?         right-recursive (292-306)
Compare    := Prefix ( ('<'|'<='|'>'|'>='|'=='|'!=') Compare )?   ONE level, right-recursive (259-290)
Prefix     := '!' Prefix | Postfix                    (244-257)
Postfix    := Primary ( '.' IDENT | '.' '*' | '[' LogicalOr ']' )*   (201-242)
Primary    := IDENT '(' ( LogicalOr (',' LogicalOr)* )? ')'          function call (69-105)
            | IDENT                                    null/true/false keywords or variable (106-119)
            | '(' LogicalOr ')'                         (122-138; returns the inner node, no Paren node)
            | INTEGER | FLOAT | STRING
```
**Precedence/associativity deviation (verified):** all six comparison operators share one level and are
right-associative. `'a' < 'b' == true` parses as `'a' < ('b' == true)` and yields a spurious
`"string" value cannot be compared to "bool" value with "<" operator`. GitHub's documented operator order
(`( ) [ ] . ! < <= > >= == != && ||`) and the reference parser in `@actions/expressions` both use left-associative
`equality` *below* `comparison` (that `@actions/expressions` structure is from memory, so check it against the source before relying on it).
`&&`/`||` being right-recursive doesn't change their meaning, but it does change the tree shape that the narrowing logic sees (§4.7).

Other syntax facts:
- Function calls are only possible on a bare identifier. `a.b()` and `(f)(x)` fail with "did not reach end of input". The callee keeps
  its original case (`FuncCallNode.Callee`). Lookup lowercases it later.
- `null`, `true`, `false` are case-sensitive keywords (`TRUE` is an undefined variable). An identifier followed by `(`
  is always a call, so `true()` becomes a call to `true`.
- Variable names and `.prop` names are **lowercased in the parser** (`strings.ToLower`, `expr_parser.go:117, 218`).
  Index-access string literals are **not** lowercased (see §8).
- `*` is only valid right after `.`. `a[*]` and `*` alone are errors.
- Trailing commas aren't allowed: `f(1,)` → unexpected token `")"` while parsing variable access....
- Empty parentheses `()` aren't allowed. Empty input gives `unexpected end of input while parsing variable access, ...`.

### 2.2 AST (`expr_ast.go`)
| Node | Fields | `Token()` (position used for errors) |
|---|---|---|
| `VariableNode` | `Name` (lowercased) | ident token |
| `NullNode`, `BoolNode{Value}`, `IntNode{Value int}`, `FloatNode{Value float64}`, `StringNode{Value}` (unescaped) | | literal token |
| `ObjectDerefNode` | `Receiver`, `Property` (lowercased) | **Receiver.Token()**, i.e. the start of the whole chain |
| `ArrayDerefNode` (`.*`) | `Receiver` | Receiver.Token() |
| `IndexAccessNode` | `Operand`, `Index` | Operand.Token() |
| `NotOpNode` | `Operand` | the `!` token |
| `CompareOpNode` | `Kind` (Less, LessEq, Greater, GreaterEq, Eq, NotEq), `Left`, `Right` | Left.Token() |
| `LogicalOpNode` | `Kind` (And, Or), `Left`, `Right` | Left.Token() |
| `FuncCallNode` | `Callee` (original case), `Args` | callee ident token |

So errors on `a.b.c` point at `a`, and errors on `x == y` point at `x`. Parentheses don't create a node, so `(42)` has offset 1
(`expr_parser_test.go:833-836`). `CompareOpNodeKind.String()` gives `< <= > >= == !=`. `LogicalOpNodeKind.String()` gives `&& ||`.

`VisitExprNode` (`expr_ast.go:258-288`) calls the visitor before and after children (`entering` flag). It visits
**`IndexAccessNode.Index` before `Operand`**, which the untrusted-input checker relies on (§6).

### 2.3 Error handling and recovery
**There is no recovery.** The first error is stored (`error()` 31-35) and every parse function returns `nil` up the stack.
Messages:
```
unexpected {what} while parsing {where}. expecting {expected}           (unexpected() 41-57)
  what      = "end of input" | "token " + %q(Kind.String())             e.g. token "INTEGER", token ")"
  expected  = comma-joined Go-quoted Kind.String() list                 e.g. "IDENT", "*"
```
| where | expected kinds |
|---|---|
| `arguments of function call` | `","`, `")"` |
| `closing ')' of nested expression (...)` | `")"` |
| `variable access, function call, null, bool, int, float or string` | `"IDENT", "(", "INTEGER", "FLOAT", "STRING"` |
| `object property dereference like 'a.b' or array element dereference like 'a.*'` | `"IDENT", "*"` |
| `closing bracket ']' for index access` | `"]"` |

Plus:
- `parsing invalid integer literal %q: %s` and `parsing invalid float literal %q: %s` (Go strconv error text).
- `parser did not reach end of input after parsing the expression. %d remaining token(s) in the input: %s`
  (`expr_parser.go:344-358`). The remaining tokens are pulled by calling `l.Next()` until `End`, and listed as quoted kind names, e.g.
  `2 remaining token(s) in the input: "IDENT", "IDENT"`. The error is positioned at the first unconsumed token.

---

## 3. Type system (`expr_type.go`)

Interface `ExprType { String(); Assignable(other) bool; Merge(other) ExprType; DeepCopy() }` (`expr_type.go:12-22`).

### 3.1 Types and `String()`
| Type | Representation | `String()` |
|---|---|---|
| `AnyType` | unit | `any` |
| `NullType` | unit | `null` |
| `NumberType` | unit (int and float are the same type) | `number` |
| `BoolType` | unit | `bool` |
| `StringType` | unit | `string` |
| `*ObjectType{Props map[string]ExprType, Mapped ExprType}` | see below | see below |
| `*ArrayType{Elem ExprType, Deref bool}` | `Deref` = produced by `.*` filtering | `array<{elem}>` (Deref not shown) |

Object flavours (`expr_type.go:183-242`). The invariant comment says every prop must be assignable to `Mapped`.
- **strict**: `Mapped == nil`. Unknown props are an error. `String()` = `{a: T; b: U}`: keys sorted (`sort.Strings`, byte order), unquoted,
  separator `"; "`, empty = `{}` (`expr_type.go:252-273`).
- **loose**: `Mapped == AnyType{}`. Unknown props are `any`. `String()` = `object`, **and the props are hidden**.
- **map**: `Mapped == T` (not Any). Every unknown prop has type T. `String()` = `{string => T}` (props hidden).
- Constructors: `NewEmptyObjectType()` loose/empty, `NewObjectType(p)` loose, `NewEmptyStrictObjectType()`,
  `NewStrictObjectType(p)`, `NewMapObjectType(t)` (Props = nil). `Strict()`/`Loose()` mutate in place.

### 3.2 Assignability: `to.Assignable(from)`
| to \ from | accepts |
|---|---|
| any | everything |
| null | null, any |
| number | number, any (TODO at `expr_type.go:88`: "Is string of numbers coerced into number?") |
| **bool** | **everything** (`expr_type.go:123-128`). This makes every "must be bool" check in sema a no-op |
| string | string, number, any (bool and null deliberately rejected, `expr_type.go:157-158`) |
| object | any, or an object such that (`expr_type.go:278-316`): to non-strict & from non-strict → `to.Mapped ⊒ from.Mapped`; to non-strict & from strict → every `from.Props[i]` ⊑ `to.Mapped`; to strict & from non-strict → every `to.Props[i]` ⊒ `from.Mapped`; both strict → every from-prop exists in `to` and is assignable (`to` may have extras) |
| array | any, or an array whose elem is assignable (Deref ignored) |

`EqualTypes(l,r) = l⊒r && r⊒l` (`expr_type.go:427-429`).

### 3.3 Merge (`a.Merge(b)`, used for unions, e.g. `&&`/`||` results, matrix rows, JSON arrays)
| a \ b | result |
|---|---|
| any | any |
| null | null if b is null, else any |
| number | number + number → number; number + string → string; else any |
| bool | bool + bool → bool; bool + string → string; else any |
| string | with string/number/bool → string; else any (null → any) |
| object + object | shortcuts: if `a` has no props and `b` is loose, return **b itself**; if `b` has no props and `a` is loose, return **a itself**. Otherwise `mapped`: if a is strict, take b.Mapped; else if b is non-strict, `a.Mapped.Merge(b.Mapped)`. Props = copy of a's; for each b prop, if it's in both then `l.Merge(r)`, else add it, and if mapped is non-nil then `mapped = mapped.Merge(r)` (`expr_type.go:321-361`). Object + non-object → any |
| array + array | if `a.Elem` is any, return **a**; if `b.Elem` is any, return **b**; else a new `array<a.Elem.Merge(b.Elem)>` with `Deref=false`. Array + non-array → any |

Notes: merge is asymmetric (number+bool is any, but both of them with string give string). Merging can break the stated invariant:
`{a: bool}`(strict).Merge(`{string=>string}`) gives `{a: bool}` with mapped string, because a's own props are never folded into
mapped. The shortcuts return *aliases*, which interacts with the in-place mutation bug in §8.

### 3.4 `.*` object filters
Typing for `recv.*` (`checkArrayDeref`, `expr_sema.go:716-757`):
| receiver type | result |
|---|---|
| any | `array<any>` (deref) |
| array | **same instance with `Deref = true` set in place** (bug, §8) |
| object with Mapped = any (loose) | `array<any>` (deref) |
| object with Mapped = object T | `array<T>` (deref) |
| object with Mapped = other | error `elements of object at receiver of object filtering `.*` must be type of object but got %q. the type of receiver was %q` → any |
| strict object | needs at least one prop of object type, else `object type %q cannot be filtered by object filtering `.*` since it has no object element` → any. Otherwise `array<any>` (deref). No union of the prop types is attempted |
| other | `receiver of object filtering `.*` must be type of array or object but got %q` → any |

Property access on a deref array maps over its elements (`expr_sema.go:631-659`):
- non-deref array: `receiver of object dereference %q must be type of object but got %q`
- elem any: returns the same array
- elem object: `array<prop type | Mapped | any>`, with Deref = true. If the elem is strict and the prop is missing:
  `property %q is not defined in object type %s as element of filtered array`
- elem non-object: `property filtered by %q at object filtering must be type of object but got %q`

So `github.event.commits.*.message` is `array<any>`, and `test().foo.*.bar.piyo` maps through nested objects.

### 3.5 `typeOfJSONValue` (`expr_type.go:441-474`)
bool → bool, float64 → number, string → string, null → null, array → `array<merge of elem types>` (empty → `array<any>`),
object → **strict** object with keys **as-is (not lowercased)**. Used for `fromJSON('<literal>')`.

---

## 4. Semantic checker (`expr_sema.go`)

### 4.1 Builtin functions (`BuiltinFuncSignatures`, `expr_sema.go:132-255`)
Keys are lowercase and lookup is case-insensitive. Display name in signatures is camelCase. `Sig.String()` =
`name(p1, p2...) -> ret`, with `...` appended after the last param when variadic (`expr_sema.go:117-127`).

| fn | overloads (`Params` → `Ret`) | variadic | IsConstFunc |
|---|---|---|---|
| contains | `(string, string) -> bool`; `(array<any>, any) -> bool` | no | yes |
| startsWith | `(string, string) -> bool` | no | yes |
| endsWith | `(string, string) -> bool` | no | yes |
| format | `(string, any...) -> string` (**at least 2 args**) | yes | yes |
| join | `(array<string>, string) -> string`; `(array<string>) -> string` | no | yes |
| toJSON | `(any) -> string` | no | yes |
| fromJSON | `(string) -> any` | no | **no** |
| hashFiles | `(string...) -> string` (at least 1) | yes | **no** |
| success / always / cancelled / failure | `() -> bool` | no | **no** |
| case | `(bool, any, any...) -> any` (at least 3) | yes | yes |

Overload resolution (`checkFuncCall`, `expr_sema.go:908-941`): undefined function →
`undefined function %q. available functions are %s` (sorted, Go-quoted, `", "`-joined). Arg types are checked once,
then each overload is tried in order. The **first passing overload wins**. If all of them fail, **every overload's error** is reported.
`checkFuncSignature` (`expr_sema.go:808-859`):
- Arity: `number of arguments is wrong. function %q takes %s%d parameters but %d arguments are given` where `%s` is
  `"at least "` for variadic functions. Variadic means `len(args) >= len(params)`, so zero repetitions of the rest param are **not** allowed.
- Type: `%s argument of function call is not assignable. %q cannot be assigned to %q. called function type is %q`.
  The ordinal comes from `ordinal()` (`1st 2nd 3rd 4th … 11th 12th 13th 21st`, `expr_sema.go:10-27`). The error is positioned at the arg, and only
  the **first** failing arg per overload is reported. Rest args are checked against the last param.

Post-resolution special handling (`checkBuiltinFuncCall`, `expr_sema.go:861-906`), which runs after
`checkSpecialFunctionAvailability`:
- **format**: only when `Args[0]` is a `StringNode`. `parseFormatFuncSpecifiers` (`expr_sema.go:31-96`) is a small state
  machine that collects the integer indices of valid `{N}` placeholders, honouring `{{`/`}}` escapes, including runs like
  `{0}}}` (an odd run of closing braces means it's still a placeholder) and `{{{0}` (an escaped `{` followed by a placeholder). Malformed braces like
  `{}`, `{x}`, or a lone `{` are silently ignored. Errors (positioned at the call):
  - for each arg index `i` with no `{i}`: `format string %q does not contain placeholder {%d}. remove argument which is unused in the format string`
  - for each placeholder ≥ arg count: `format string %q contains placeholder {%d} but only %d arguments are given to format`
    (iterates a Go map, so emission order is random, but the final sort makes it deterministic)
- **fromJSON**: only for a `StringNode` arg. It runs `json.Unmarshal`. On success it **returns `typeOfJSONValue`** instead of `any`.
  On `*json.SyntaxError`: `broken JSON string is passed to fromJSON() at offset %d: %s`, positioned at the literal, where the
  message text is Go's encoding/json message (e.g. `unexpected end of JSON input`, `invalid character 'x' looking for beginning of value`).
- **case**: if `len(Args)` is even: `case() requires an odd number of arguments (pred/value pairs + default) but got %d`. Return type is always `any`.

### 4.2 Global contexts (`BuiltinGlobalVariableTypes`, `expr_sema.go:261-357`)
All keys are lowercase (enforced by `TestBuiltinGlobalVariableTypesValidation`, `expr_sema_test.go:1770`).
- `github`: **strict** object with: action, action_path, action_ref, action_repository, action_status, actor, actor_id,
  api_url, base_ref, env, event_name, event_path, graphql_url, head_ref, job, output, path, ref, ref_name, ref_type,
  repository, repository_id, repository_owner, repository_owner_id, repository_visibility, repositoryurl, run_attempt,
  run_id, run_number, secret_source, server_url, sha, state, step_summary, token, triggering_actor, workflow,
  workflow_ref, workflow_sha, workspace: **string**. artifact_cache_size_limit, retention_days: **number**.
  ref_protected: **bool**. `event`: **loose object** (no payload typing. Webhook data is *not* used here).
- `env`: `{string => string}`.  `vars`: `{string => string}`.  `secrets`: `{string => string}` (default).
- `job`: strict `{check_run_id: number; container: {id: string; network: string}; services: {string => {id: string; network: string; ports: {string => string}}}; status: string}`.
- `steps`: strict `{}` (updated contextually). `matrix`: strict `{}`. `needs`: strict `{}`. `inputs`: strict `{}`.
- `runner`: strict `{arch, debug, environment, name, os, temp, tool_cache}`, all string.
- `strategy`: **loose** `{fail-fast: bool; job-index: number; job-total: number; max-parallel: number}`.
- `jobs` is **absent** by default. It's added only when checking `on.workflow_call.outputs.*.value` (`UpdateJobs`).
  Because it's absent, the `undefined variable` list only contains "jobs" in that context.

### 4.3 Contextual updates (`expr_sema.go:413-508`)
The checker starts from the shared global map. `ensureVarsCopied` shallow-copies the map before replacing an entry.
- `UpdateMatrix/Steps/Needs/Jobs(ty)`: plain replacement (the rule's object is shared by reference, not copied).
- `UpdateSecrets(ty)`: a strict object of `{github_token, actions_step_debug, actions_runner_debug}: string` plus the declared props (#130).
- `UpdateInputs(ty)`: if the current inputs are the empty strict object, replace them. Otherwise `o.Merge(ty)` (workflow_call ∪ workflow_dispatch, #263).
- `UpdateDispatchInputs(ty)`: `UpdateInputs(ty)`, then **deep-copies `github`** and sets
  `github.event.inputs = strict {each input: string}` (dispatch inputs are always strings under `github.event.inputs`).
  `ensureGithubVarCopied` never sets `githubVarCopied = true` (`expr_sema.go:427-434`). That's harmless only because a
  new checker is created per expression.

How the rule computes each context (`rule_expression.go`):
- **inputs** (`VisitWorkflowPre` 58-169):
  - workflow_dispatch: boolean → bool, number → number, string/choice/environment → string, other → any. Keys are lowercase
    (`DispatchInput` map). Stored as `dispatchInputsTy`.
  - workflow_call: `rule.inputsTy` is set *before* the inputs are iterated, and the inputs are an ordered slice (`ast.go:322`). So a
    default can reference *earlier* inputs but not itself or later ones. string → string, boolean → bool, number → number, other → any.
    For boolean/number inputs with a single-expression default: `type of input %q must be bool but found type %s` /
    `type of input %q must be number but found type %s` (accepted: exact type or any).
- **secrets**: only if `on.workflow_call.secrets` exists (non-nil map): strict `{declared: string}` + auto secrets. Otherwise
  `{string=>string}`. This applies to the **whole workflow**, even jobs run by other triggers.
- **needs** (`calcNeedsType` 831-870): strict `{<needed job id lowercased>: {outputs: O; result: string}}`, **direct
  dependencies only** (#151). O = strict `{output names: string}` for normal jobs. For reusable-workflow jobs, O = strict string
  props from the called workflow's metadata (`getWorkflowCallOutputsType` 361-380), or `{string=>string}` when unknown
  (remote workflow, or metadata error, which is reported at `uses`). Unknown job IDs are skipped.
- **matrix** (`checkMatrix` 905-973, `checkMatrixRow` 975-998, `checkMatrixExpression` 872-903):
  - whole `matrix: ${{ }}`: must be an object (`type of expression at "matrix" must be object ...`). If the result is an object, delete
    `include` (after merging the props of its elements' object type) and `exclude`. Otherwise it's a loose empty object.
  - rows: `name: [v...]` → merge of the elem types. `name: ${{ }}` must be an array (`... at "matrix row" must be array ...`) → elem type,
    else any. Empty row → any.
  - raw YAML values (`checkRawYAMLValue` 1029-1051): mapping → strict object; sequence → `array<merged elems>` (empty → `array<any>`);
    scalar → expression type if it's a single `${{}}`, else `true`/`false` → bool, `null` → null, `ParseFloat` ok → number, else string.
  - `include`: `${{ }}` → `o.Merge(elem)` if it's an array of objects, else a loose empty object. Each list item's key/values are merged
    into props (new keys added as-is, even though they exist only in some combinations). An item that is itself an expression is
    **never checked** (bug, §8).
  - `exclude`: type-checked (array of objects) but not used for typing.
  - All matrix expressions use workflow key `jobs.<job_id>.strategy`.
- **steps** (`VisitStep` 282-325): `rule.stepsTy` is reset to strict `{}` at the *end* of `VisitJobPre` (so job-level keys
  see the global empty `steps`). Each step with an `id` registers `steps.<lowercased id> = strict {outputs: O; conclusion: string; outcome: string}`
  **after** its own fields are checked (a step can't see itself). If the id contains `${{` it's checked with key `""` and
  `stepsTy.Loose()` is called, after which any step name resolves to any. O (`getActionOutputsType` 328-359):
  - `run:` step → `{string=>string}`
  - `./local` action → strict outputs from action.yml (lowercased), or loose if `SkipOutputs`. A missing action file gives `{string=>string}`, and a metadata error is reported at `uses`
  - `actions/github-script@…` → loose object (#104)
  - entry in `PopularActions[spec]` (exact `owner/repo@ref` key) → strict outputs, or loose if SkipOutputs
  - otherwise `{string=>string}`
  `VisitJobPost` checks `environment` and `outputs` with the full steps type, then clears matrix/steps/needs.
- **jobs** (`checkWorkflowCallOutputs` 1000-1027, run in `VisitWorkflowPost`): strict `{<job>: {outputs: strict{names: string}}}`, with
  reusable-call jobs getting a loose `outputs`. **`result` is missing**, so `${{ jobs.j.result }}` gives `property "result" is not defined in object type {outputs: {}}` **(verified, false positive)**.

### 4.4 Variable / property / index checks
- `checkVariable` (`expr_sema.go:598-611`): unknown → `undefined variable %q. available variables are %s`. Uses the
  **original token text** (`TRUE`) and a sorted quoted list. Otherwise it calls `checkAvailableContext` (§5) and returns the type.
- `checkObjectDeref` (`expr_sema.go:613-664`):
  - any → any
  - object → `Props[p]`, else Mapped (if `Mapped != nil` and the receiver is the bare `vars` variable, it also runs `checkConfigVariables`), else for strict:
    `property %q is not defined in object type %s` (note `%s`: the type is **unquoted**) → any
  - array → §3.4
  - else `receiver of object dereference %q must be type of object but got %q`
- `checkConfigVariables` (`expr_sema.go:666-714`), only for `vars.NAME` (not `vars['NAME']`):
  - prefix `github_` → `configuration variable name %q must not start with the GITHUB_ prefix (case insensitive). note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables`
  - chars outside `[0-9a-z_]` → `configuration variable name %q can only contain alphabets, decimal numbers, and '_'. note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables`
  - `configVars == nil` → no further checks. Empty list → `no configuration variable is allowed since the variables list is empty in actionlint.yaml. you may forget adding the variable %q to the list`.
    Not found (EqualFold) → `undefined configuration variable %q. defined configuration variables in actionlint.yaml are %s`.
- `checkIndexAccess` (`expr_sema.go:759-806`): **Index is checked before Operand** (needed by the untrusted checker).
  - operand any → any
  - array: index must be any/number, else (at Index) `index access of array must be type of number but got %q` → elem
  - object: index any → any. Index string: if the index is a **string literal**, look up `Props[lit]` (case-sensitive!), then Mapped, else for strict
    `property %q is not defined in object type %s`. Non-literal string → Mapped or any. Other index (at Index):
    `property access of object must be type of string but got %q`
  - else `index access operand must be type of object or array but got %q`

### 4.5 Operators
- `!x` (`expr_sema.go:943-949`): the check `type of operand of ! operator %q is not assignable to type "bool"` is **dead** (bool accepts all). Result: bool.
- Comparisons (`validateCompareOpOperands` 951-1003, `checkCompareOp` 1005-1014): result bool. Error at Left:
  `%q value cannot be compared to %q value with %q operator` (types and op Go-quoted).
  - `==`/`!=`: any/null on the left → ok. number/bool/string vs object/array → error. Object vs object/null/any ok, else error.
    Array vs array → recurse on the elems. Array vs null/any ok, else error. (Asymmetric: `null == {}` ok, `{} == 1` error.)
  - `< <= > >=`: left must be any/number/string, and right must not be null/bool/object/array. Left null/bool/object/array → always error.
- `&&` / `||` (`checkLogicalOp` 1047-1062, `checkWithNarrowing` 1022-1045): type = `narrow(L, assume) .Merge( type(R) )`
  where for `&&` the left is narrowed assuming *falsy*, and for `||` assuming *truthy*. Narrowing: `(a && b)` assumed truthy → type(b);
  `(a || b)` assumed falsy → type(b); `!x` flips the assumption; otherwise the plain type. E.g. `('foo' && 10) || 20` → number,
  `('foo' && 10) && 20` → string, `10 && 'foo'` → string (number+string). Tests: `expr_sema_test.go:657-711`.

### 4.6 Error ordering inside `Check`
`Check` (`expr_sema.go:1101-1113`) returns the type plus the semantic errors, followed by the untrusted-input errors.

### 4.7 `IsConstant` (`expr_sema.go:1121-1152`)
- Literals → true. Variable/deref/index → false.
- `!`, compare, logical → all operands constant (no short-circuit reasoning: `x && false` isn't constant).
- Calls → all args constant **and every overload has `IsConstFunc`**.

Used only by `rule_if_cond`.

### 4.8 Case-insensitivity summary
- Lowercased: context names, `.prop` names (parser), function names (lookup), step IDs, job IDs, needs keys, matrix keys,
  input/secret/output keys (lowercased by the YAML parser, `ast.go` comments), local action outputs, config var comparison.
- **Not lowercased:** `['Literal']` index keys (sema and untrusted checker), keys produced by `fromJSON` literals.

---

## 5. Context availability (`availability.go`, generated)

`WorkflowKeyAvailability(key) (ctx []string, sp []string)` (`availability.go:12-47`). Unknown key → `(nil, nil)`.
`SpecialFunctionNames` maps each special function to its allowed keys (`availability.go:55`). `AllContexts` is the inverse map
(`availability.go:60`). `allWorkflowKeys` is used by tests.

| workflow key(s) | contexts | special fns |
|---|---|---|
| `run-name`, `concurrency`, `on.workflow_call.inputs.<inputs_id>.default` | github, inputs, vars | — |
| `env` (workflow) | github, inputs, secrets, vars | — |
| `on.workflow_call.outputs.<output_id>.value` | github, inputs, jobs, vars | — |
| `jobs.<job_id>.if` | github, inputs, needs, vars | always, cancelled, failure, success |
| `jobs.<job_id>.strategy` | github, inputs, needs, vars | — |
| `jobs.<job_id>.{concurrency, container, container.image, continue-on-error, environment, name, runs-on, services, timeout-minutes, with.<with_id>}` | github, inputs, matrix, needs, strategy, vars | — |
| `jobs.<job_id>.snapshot.if` (hard-coded by the generator, docs issue 41255) | github, inputs, matrix, needs, strategy, vars | — |
| `jobs.<job_id>.env`, `jobs.<job_id>.secrets.<secrets_id>` | github, inputs, matrix, needs, secrets, strategy, vars | — |
| `jobs.<job_id>.defaults.run` | env, github, inputs, matrix, needs, strategy, vars | — |
| `jobs.<job_id>.container.credentials`, `jobs.<job_id>.services.<service_id>.credentials` | env, github, inputs, matrix, needs, secrets, strategy, vars | — |
| `jobs.<job_id>.container.env.<env_id>`, `jobs.<job_id>.services.<service_id>.env.<env_id>` | env, github, inputs, job, matrix, needs, runner, secrets, strategy, vars | — |
| `jobs.<job_id>.environment.url` | env, github, inputs, job, matrix, needs, runner, steps, strategy, vars | — |
| `jobs.<job_id>.outputs.<output_id>` | env, github, inputs, job, matrix, needs, runner, secrets, steps, strategy, vars | — |
| `jobs.<job_id>.steps.if` | env, github, inputs, job, matrix, needs, runner, steps, strategy, vars | always, cancelled, failure, hashfiles, success |
| `jobs.<job_id>.steps.{continue-on-error, env, name, run, timeout-minutes, with, working-directory}` | env, github, inputs, job, matrix, needs, runner, secrets, steps, strategy, vars | hashfiles |

How the sema check works:
- `checkAvailableContext` (`expr_sema.go:523-546`) runs for every *defined* variable. Error:
  `context %q is not allowed here. %s. see https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability for more details`
  with notes `no context is available here` (0) / `available context is "x"` (1) / `available contexts are "a", "b"` (n), listed in the table's sorted order.
  The context name is the lowercased name.
- **An empty or nil list means NO context is allowed.** The doc comments in `availability.go:6` and `expr_sema.go:516-517` say the
  opposite and are wrong. When `workflowKey == ""`, `SetContextAvailability` isn't called, so any context reference at those keys errors
  **(verified)**: workflow `name:`, `on.*` filters (`branches`, `types`, `paths`, …), cron/timezone, dispatch input fields,
  workflow_call input/secret/output `description`, image_version names/versions, workflow-level `defaults.run`, `jobs.*.needs`,
  step `shell`, step `uses`, job `uses`, and step `id`. Unknown non-empty keys log `rule.Debug("No context availability was found for workflow key %q")`.
- `checkSpecialFunctionAvailability` (`expr_sema.go:563-584`) runs only after the overload resolved, and only for names in
  `SpecialFunctionNames`. Error: `calling function %q is not allowed here. %q is only available in %s. see https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability for more details`
  (callee in its original case, then a quoted list of allowed keys).

Generator (`scripts/generate-availability/main.go`), run via `//go:generate` at `rule_expression.go:8`:
1. Fetches the raw markdown `github/docs/.../contexts.md` (`main.go:24`) or reads a local file.
2. Parses it with goldmark plus the table extension. Finds the h3 `Context availability`, then the first table before the next h3 (`main.go:85-107`).
3. Each row must have 3 cells: key, contexts, functions. Liquid `{% … %}` is stripped (a row containing `{% else %}` is an error, and a first cell equal to `{% else %}` stops the loop).
   Cells are HTML-unescaped, split on `,`, trimmed, lowercased, and sorted. `None` → `[]`. Empty keys are skipped (`main.go:168-215`).
4. Appends `jobs.<job_id>.snapshot.if` if it's missing (`main.go:217-236`, values taken from actions/runner `WorkflowTemplateConverter.cs`).
5. Groups keys with identical (ctx, sp) into `case` arms and emits gofmt'ed Go (`main.go:238-276`).

`scripts/generate-webhook-events` scrapes `events-that-trigger-workflows` HTML (`div[data-search=article-body] div.markdown-body`,
h2 ids after `about-events-that-trigger-workflows`, tables `aria-labelledby=<hook>`, header row `Webhook event payload | Activity types`,
`<code>` items. `Not applicable` → `{}`, `Custom` → nil) into `AllWebhookTypes` (`all_webhooks.go`). That table is used by `rule_events.go`,
**not** by the expression engine. `github.event` is untyped.

**Rust recommendation:** generate a `phf`/`match` table at build time from a checked-in JSON or TOML snapshot, produced by a port of the
generator (an `xtask`). Represent context sets as a `u16` bitset (12 contexts) and special functions as a `u8` bitset. Keep string lists only for messages.

---

## 6. Untrusted-input / script-injection check (`expr_insecure.go`)

### 6.1 Untrusted paths (`BuiltinUntrustedInputs`, `expr_insecure.go:86-141`)
It's a tree of `UntrustedInputMap{Name, Parent, Children}`. Leaves (`Children == nil`) are the untrusted values. `*` stands for array elements.
```
github.head_ref
github.event.issue.{title, body}
github.event.pull_request.{title, body}
github.event.pull_request.head.{ref, label}
github.event.pull_request.head.repo.default_branch
github.event.comment.body
github.event.review.body
github.event.review_comment.body
github.event.pages.*.page_name
github.event.commits.*.message
github.event.commits.*.author.{email, name}
github.event.head_commit.message
github.event.head_commit.author.{email, name}
github.event.discussion.{title, body}
```
(20 paths, `expr_insecure_test.go:10-31`.) The TODO at `expr_insecure.go:79` notes the list was meant to be generated from CodeQL's ExpressionInjection.ql.
Not covered, as far as I recall CodeQL's list (check before relying on it): `github.event.workflow_run.{head_branch, display_title, head_commit.message, head_commit.author.*, pull_requests.*.head.ref}`,
`github.event.pull_request.head.repo.description`/`homepage`, and similar. There is no taint propagation through `env`, `steps.*.outputs`, `needs.*.outputs`, or `inputs`.

### 6.2 Algorithm (state machine driven by `check()` enter/leave callbacks, `expr_sema.go:1064-1066`)
State: `cur []*UntrustedInputMap` (multiple cursors because of `.*`), `start` (the root VariableNode, used for the error position),
`filteringObject`, `safeCalls` (`expr_insecure.go:150-157`). Leave-callback dispatch (`OnVisitNodeLeave` 305-332):
- If `safeCalls > 0`: do nothing except decrement when leaving a safe call. **Everything inside `contains/startsWith/endsWith`
  is skipped, including nested `format`/`fromJSON`** (`isSafeFuncCall` 7-10).
- `VariableNode` → `end()` (flush the previous chain) then `onVar` (start a chain if the name is a root, currently only `github`).
- `ObjectDerefNode` → `onPropAccess(prop)`: advance each cursor to its child, dropping the ones that don't match.
- `IndexAccessNode` with a `StringNode` index → `onPropAccess(lit.Value)` (**case-sensitive**). Otherwise `onIndexAccess()`: advance to the `*` child,
  unless the previous op was `.*` (`filteringObject`), so `github.event.*.body[0]` still matches.
- `ArrayDerefNode` → `onObjectFilter()`: a cursor with a `*` child moves to it. Otherwise it fans out to **all** children (object filter). A leaf cursor is dropped.
- Any other node → `end()`.
- `end()` (266-296) collects the cursors that sit on leaves. One leaf:
  `%q is potentially untrusted. avoid using it directly in inline scripts. instead, pass it through an environment variable. see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details`
  Several leaves (object filter):
  `object filter extracts potentially untrusted properties %s. avoid using the value directly in inline scripts. instead, pass the value through an environment variable. see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details`
  (sorted, quoted). The path is printed through `buildPath` with `*` for indices, e.g. `github.event.commits[0].author.name` → `"github.event.commits.*.author.name"` **(verified)**.
  Errors are positioned at `start` (the `github` token).
- Because Index is visited before Operand, nested indices are flushed before the outer chain resumes (`expr_sema.go:760-763`).

Not flagged (by design, `expr_insecure_test.go:309-346`): `github.event`, `github.event.issue` (object, non-leaf), `toJSON(github.event)`,
`github.event.issue.body.foo`, `github.event.issue.body[0]`, dynamic `github[x]...`, anything under a safe call.

### 6.3 Sinks
Only `checkScriptString` (`rule_expression.go:689-700`) enables the check:
- `jobs.<id>.steps[*].run` (`rule_expression.go:289`)
- `with.script` of a step whose `uses` starts with `actions/github-script@` (`rule_expression.go:295-296`). Forks, `docker://`, and other
  eval-ish actions aren't covered.

Not sinks: `if:`, `env:`, other `with:` inputs, `working-directory`, `shell`. Composite `action.yml` files aren't linted by this rule at all.
An untrusted-input error counts like any other error and **aborts checking the remaining `${{ }}` in the same string** (§8).

---

## 7. `rule_expression.go` (rule name `expression`) and `rule_if_cond.go` (rule name `if-cond`)

### 7.1 Core helpers
- `checkExprsIn(s, pos, quoted, checkUntrusted, key)` (734-768) is the loop over `${{`. `line, col = pos.Line, pos.Col`, with `col++` if the YAML
  scalar is quoted. For each expression, the column of its first char after `${{` = `col + byteOffsetInString`. Each expression gets a new lexer, parser, and checker
  (`checkSemantics` 819-829, `checkSemanticsOfExprNode` 775-817, which also applies matrix/steps/needs/secrets/inputs/dispatch/jobs
  types and availability). **The first expression with any error (parse or sema) makes the function return `(nil, false)`, so later
  expressions in the same string are never checked** **(verified: `echo ${{ foo }} ${{ bar }}` reports only `foo`)**.
  It returns `[]typedExpr{ty, Pos{line, col-3}}`, where the position is that of the `${{`.
- `exprError` → `convertExprLineColToPos(errLine, errCol, line, col)` = `{line + errLine - 1, col + errCol - 1}` (1077-1083).
- `checkString(str, key)` (675-687) = `checkExprsIn` + `checkTemplateEvaluatedType`: an object/array/null result gives
  `object, array, and null values should not be evaluated in template with ${{ }} but evaluating the value of type %s` at the `${{` position.
- `checkScriptString` = the same, with the untrusted check on.
- `checkOneExpression(s, what, key)` (382-400): needs exactly one `${{ }}`, else
  `one ${{ }} expression should be included in %q value but got %d expressions` (normally unreachable, because the YAML parser has already enforced single-expression form).
- Typed wrappers (errors at the scalar's `Pos`):
  - `checkBool` (702-718): only when `Bool.Expression != nil`. bool/any is ok, else `type of expression must be bool but found type %s`.
  - `checkInt` → `checkNumberExpression(…, "integer value", …)`, `checkFloat` → `"float number value"`. Errors use `type of expression at %q must be number but found type %s`.
  - `checkObjectTy`: `type of expression at %q must be object but found type %s`. `checkArrayTy`: `type of expression at %q must be array but found type %s`.
- The YAML parser decides which keys may hold expressions. For bool/int/float keys, a `!!str` value must be a single `${{ }}`, else
  `expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node` (`parse.go:182-190, 239-246`).

### 7.2 Every checked location, its workflow key (for availability), and the expected type
| Location | Check | Workflow key |
|---|---|---|
| `name` | string template | `""` (no contexts) |
| `on.<webhook>.{types, branches, branches-ignore, tags, tags-ignore, paths, paths-ignore, workflows}` | string | `""` |
| `on.schedule[].{cron, timezone}` | string | `""` |
| `on.workflow_dispatch.inputs.*.{description, default, options[]}` | string; `required` bool | `""` |
| `on.repository_dispatch.types` | string | `""` |
| `on.workflow_call.inputs[].description` | string | `""` |
| `on.workflow_call.inputs[].default` | string, plus bool/number type match for typed inputs | `on.workflow_call.inputs.<inputs_id>.default` |
| `on.workflow_call.secrets.*.description`, `outputs.*.description` | string | `""` |
| `on.workflow_call.outputs.*.value` (in `VisitWorkflowPost`, with `jobs`) | string | `on.workflow_call.outputs.<output_id>.value` |
| `on.image_version.{names, versions}` | string | `""` |
| `run-name` | string | `run-name` |
| `env` (workflow) | each name and value string, or the whole `${{}}` must be an object (`"env"`) | `env` |
| `defaults.run.{shell, working-directory}` (workflow) | string | `""` |
| `concurrency.group` / `cancel-in-progress` | string / bool | `concurrency` |
| `jobs.*.name` | string | `jobs.<job_id>.name` |
| `jobs.*.needs[]` | string | `""` |
| `jobs.*.runs-on` whole `${{}}` | string, array, or any, else `type of expression at "runs-on" must be string or array but found type %q` (`%q`, unlike the others) | `jobs.<job_id>.runs-on` |
| `jobs.*.runs-on` labels[] / `group` | string | `jobs.<job_id>.runs-on` |
| `jobs.*.concurrency.*` | string / bool | `jobs.<job_id>.concurrency` |
| `jobs.*.env` | as workflow env | `jobs.<job_id>.env` |
| `jobs.*.defaults.run.*` | string | `jobs.<job_id>.defaults.run` |
| `jobs.*.if` | `checkIfCondition` | `jobs.<job_id>.if` |
| `jobs.*.strategy.matrix` | see §4.3 | `jobs.<job_id>.strategy` |
| `jobs.*.strategy.fail-fast` / `max-parallel` | bool / number ("integer value") | `jobs.<job_id>.strategy` |
| `jobs.*.continue-on-error` | bool | `jobs.<job_id>.continue-on-error` |
| `jobs.*.timeout-minutes` | number ("float number value") | `jobs.<job_id>.timeout-minutes` |
| `jobs.*.container.{image, ports[], volumes[], options, command, entrypoint}` | string | `jobs.<job_id>.container` |
| `jobs.*.container.credentials` | object (`"credentials"`) or username/password strings | `jobs.<job_id>.container.credentials` |
| `jobs.*.container.env` | env | `jobs.<job_id>.container.env.<env_id>` |
| `jobs.*.services` whole `${{}}` | object (`"services"`) | `jobs.<job_id>.services` |
| `jobs.*.services.*` container fields | as container | `jobs.<job_id>.services` (+ `.<service_id>.credentials` / `.<service_id>.env.<env_id>`) |
| `jobs.*.uses` | string | `""` |
| `jobs.*.with.*` | string, plus a type check against the called workflow's inputs: `input %q is typed as %s by reusable workflow %q. %s value cannot be assigned`. A literal value's type is null/bool/number (ParseFloat)/string. A single expression gives its type, anything else is string. **Boolean inputs never fail** (bool accepts all) | `jobs.<job_id>.with.<with_id>` |
| `jobs.*.secrets.*` | string | `jobs.<job_id>.secrets.<secrets_id>` |
| `jobs.*.snapshot.if` | `checkIfCondition` | `jobs.<job_id>.snapshot.if` |
| `jobs.*.environment.{name}` / `deployment` (post) | string / bool | `jobs.<job_id>.environment` |
| `jobs.*.environment.url` (post) | string | `jobs.<job_id>.environment.url` |
| `jobs.*.outputs.*` (post) | string | `jobs.<job_id>.outputs.<output_id>` |
| `steps[].name` | string | `jobs.<job_id>.steps.name` |
| `steps[].if` | `checkIfCondition` | `jobs.<job_id>.steps.if` |
| `steps[].run` | **script** (untrusted check) | `jobs.<job_id>.steps.run` |
| `steps[].shell` | string | `""` |
| `steps[].working-directory` | string | `jobs.<job_id>.steps.working-directory` |
| `steps[].uses` | string | `""` |
| `steps[].with.*` (`script` of github-script = **script**) | string | `jobs.<job_id>.steps.with` |
| `steps[].with.{entrypoint, args}` (docker) | string | `jobs.<job_id>.steps.with` |
| `steps[].env` | env | `jobs.<job_id>.steps.env` |
| `steps[].continue-on-error` | bool | `jobs.<job_id>.steps.continue-on-error` |
| `steps[].timeout-minutes` | number | `jobs.<job_id>.steps.timeout-minutes` |
| `steps[].id` containing `${{` | string; makes `steps` loose | `""` |

### 7.3 `checkIfCondition` (609-664)
- If the string `ContainsExpression()`: `checkString` (so an object-typed `${{ github.event }}` gives the "template" error **(verified)**).
  If it's a single assigned expression, `condTy` = its type.
- Otherwise the whole string is an implicit expression. It lexes `str.Value + "}}"` at `str.Pos` **without the quoted-column adjustment**,
  so `if: "foo"` reports column 13 instead of 14 **(verified)**. The lexer stops at the first `}}`, so in `if: github.ref == 'x' }}` the trailing
  `}}` is silently ignored **(verified)**.
- The final check `"if" condition should be type "bool" but got type %q` (661-663) can never fire (`BoolType.Assignable` is always true). It's dead code.

### 7.4 `rule_if_cond.go`
For `steps[].if`, `jobs.*.if`, `jobs.*.snapshot.if`:
- If both `${{` and `}}` are found (first occurrences) and `${{` isn't at index 0, or the first `}}` doesn't end the string, or there's more than one `${{`, it reports:
  `if: condition %q is always evaluated to true because extra characters are around ${{ }}` (51-57). The `%q` shows `\n`,
  e.g. a `|` block scalar `"${{ x }}\n"` triggers it (test `rule_if_cond_test.go:22`).
- Otherwise it trims, lexes and parses again (`NewExprLexer(i+"}}")`). If parsing succeeds and `IsConstant`: `constant expression %q in condition. remove the if: section`.
- Errors are at `n.Pos` (start of the scalar). The expression is parsed a second time (rule_expression already parsed it).

---

## 8. Known gaps, bugs, approximations, and performance issues

Bugs (all **verified** unless noted):
1. **In-place `Deref` mutation**: `checkArrayDeref` does `ty.Deref = true` on the receiver's array (`expr_sema.go:720-722`). Types are shared
   (the rule keeps `matrixTy`/`stepsTy` for the whole job), so after `${{ matrix.os.* }}`, a later `${{ matrix.os.a }}` is wrongly accepted as `array<number>`.
2. **Include-combination expressions are never checked**: `rule_expression.go:950` passes `m.Include.Expression` (always nil on that branch)
   instead of `combi.Expression`. `include: ["${{ undefined }}"]` gives no error, and its props aren't merged into the matrix type (false positive `property "x" is not defined` later).
3. **Case-sensitive literal index**: `steps['MyStep']`, `matrix['OS']`, `github['EVENT']` all give "property not defined" (`expr_sema.go:783-784`).
   The untrusted checker also misses `github['EVENT']['ISSUE']['TITLE']` (`expr_insecure.go:321-323`).
4. **`fromJSON` literal keys keep their case** while property access is lowercased: `fromJSON('{"Foo":1}').Foo` gives `property "foo" is not defined in object type {Foo: number}`.
5. **First error aborts the rest of the string** (`rule_expression.go:755-757`). Also applies to untrusted errors.
6. **Positions inside strings are approximate** (TODO at `rule_expression.go:735`):
   a) multi-line scalars (`|`, `>`, multi-line plain): the line is always the scalar's start line, and the column adds the byte offset across newlines;
   b) quoted scalars with escapes (`\"`, `''`) drift by one column per escape;
   c) the host offset is in bytes while the in-expression column is in runes, so non-ASCII before `${{` shifts columns (`'あいう'` → +6);
   d) bare `if:` ignores the quote adjustment.
7. Comparison operators are one precedence level and right-associative (§2.1).
8. `ensureGithubVarCopied` never sets its flag (`expr_sema.go:427-434`).
9. `populateDependantNeedsTypes` compares the lowercased need id with `root.ID.Value` (original case) for its cycle guard (`rule_expression.go:840-842`). Minor.
10. `rule_if_cond` uses the first `}}`. False positive for `'}}'` inside the expression.
11. `jobs.<job_id>.result` is missing from the `jobs` context.

Approximations and dead checks:
- `BoolType.Assignable` accepts everything, so the `!` operand check, the `if` bool check, the `case()` predicate type, and bool-typed reusable-workflow inputs are all vacuous.
- int literals are limited to int32. No `NaN`/`Infinity`/`+e`. Format strings aren't validated for malformed braces (GitHub errors at runtime on `{x}`; worth confirming).
  `join('str')`: docs say a string is allowed (worth confirming), but actionlint rejects it. `null` isn't assignable to string params (stricter than runtime coercion).
- `strategy` is loose. `github.event` is untyped (webhook payloads aren't modelled). Matrix `include`-only keys are typed as always present.
  The `secrets` type is workflow-wide. `.*` on a strict object gives `array<any>` instead of a union.
- No short-circuit constant folding in `IsConstant`. `case()` returns any (it could merge its branch types).
- `vars['X']` skips the config-variable checks.
- The untrusted list is hand-maintained (TODO `expr_insecure.go:79`), with no taint flow. Only `run:`/github-script are sinks.
- There are only three TODO/FIXME comments: `expr_insecure.go:79`, `expr_type.go:88`, `rule_expression.go:735` (plus a test TODO at `expr_sema_test.go:142`). One XXX at `scripts/generate-availability/main.go:217`.

Performance (from reading the code. Benchmark corpus: `testdata/bench/expressions.txt`, 1858 real-world expressions, `expr_test.go:35-116`):
- A **new `ExprSemanticsChecker` per expression** (`rule_expression.go:780`) shallow-copies the vars map. With `workflow_dispatch` inputs it
  **deep-copies the whole `github` object type** (≈45 props) for every expression. Also a new `UntrustedInputChecker` per script expression.
- Overload resolution formats error strings (`fmt.Sprintf` including `sig.String()`) for every failing overload, even when a later
  overload succeeds (e.g. `contains(array, x)` always formats the `(string,string)` failure).
- If conditions are lexed and parsed twice (rule_expression + rule_if_cond), and `runs-on` labels a third time (`rule_runner_label.go:246-247`).
- `text/scanner` works per rune, plus a heap `*Token` per token. Type objects are heap interfaces. `ObjectType.String()` sorts keys on every call (error path only).

---

## 9. Rust design recommendations

### 9.1 Crate layout
`expr/` module (or crate `workflint-expr`): `lexer.rs`, `parser.rs`, `ast.rs`, `types.rs`, `sema.rs`, `builtins.rs` (functions and
global context types), `untrusted.rs`, `availability.rs` (generated), `format_spec.rs`, `gofmt.rs` (Go-compatible quoting and
messages), `json_go.rs` (Go-compatible JSON syntax errors). Rule glue lives elsewhere and uses an `ExprEnv`.

### 9.2 Lexer
- Hand-written over `&[u8]` with a byte cursor. Track `line` and a **char column** (count UTF‑8 lead bytes, i.e. `b & 0xC0 != 0x80`) to match
  text/scanner. Keep the byte offset too. Validate UTF‑8 lazily (the input is already `&str` from YAML, so the invalid-UTF‑8 branch can't happen. NUL needs a
  check: emit `scan error while lexing expression: invalid character NUL`).
- `Token { kind: TokKind (u8), start: u32, end: u32, line: u32, col: u32 }`. The lexeme is `&src[start..end]`. Zero allocations.
- Keep the "`}}` terminates" contract: `lex_expression(src) -> Result<(Vec<Token>, consumed_bytes), ExprError>`. Better, make the parser pull tokens
  lazily and return `consumed`. The parser's "remaining tokens" message needs the lexer to keep running until End.
- Port the number state machine exactly (including the `0x0`-only rule and the error "lexeme so far").

### 9.3 Parser
- Recursive descent or Pratt (binding powers: `||`=1, `&&`=2, `== !=`=3, `< <= > >=`=4, prefix `!`=5, postfix `.`/`[`=6).
  **Decision needed:** a correct grammar (left-assoc, two comparison levels) or bug-compatible mode. Recommendation: correct by default. The only
  observable difference is errors/types for chained comparisons, which are rare in the testdata. Behind a `compat` flag, the right-recursive single level
  is a 5-line change.
- AST: a `Vec<Node>` arena with `NodeId(u32)` and a parent-less, child-index layout. Or simply `Box`-based enums (expressions are tiny,
  roughly 2–20 nodes). Store `prop: SmolStr` (lowercased, usually ≤22 bytes, so inline) plus the token span for the original text. Lowercase **string literal
  index keys** in a side field to fix bug 3 while keeping the original for messages.
- Errors: single first-error semantics, same message strings. Error recovery isn't needed for parity. If wanted later, add sync on `}}`/`,`/`)`.

### 9.4 Types
```rust
#[derive(Clone)]
enum Ty { Any, Null, Number, Bool, String, Object(Rc<ObjTy>), Array(Rc<Ty>, /*deref*/ bool) }
struct ObjTy { props: BTreeMap<SmolStr, Ty>, rest: Rest }    // BTreeMap gives Go's sorted String() for free
enum Rest { Strict, Loose, Map(Ty) }
```
- **Immutable values.** Build new nodes on merge. `deref` is part of the value, so the Go in-place mutation bug can't exist. Use `Rc` (single-threaded per file)
  or `Arc` if files are checked in parallel and types are shared across threads (globals in a `static LazyLock<GlobalTypes>` need `Arc` or
  `&'static`). Builtin context types can be `&'static` trees built once.
- Implement `assignable`, `merge`, and `Display` exactly per §3. The Display output is part of the error messages (`{a: T; b: U}`, `{string => T}`, `object`, `array<T>`).
  Key ordering = byte-wise ascending (BTreeMap<String> matches Go `sort.Strings`).
- Interning: optional. Most keys are short. `SmolStr` or `compact_str` avoids allocation. An interner (`lasso`) only pays off when a whole
  repository is cached.

### 9.5 Semantic environment instead of per-expression copying
```rust
struct ExprEnv<'a> {            // built once per workflow/job/step, borrowed by each check
    matrix: Option<Ty>, steps: Option<Ty>, needs: Option<Ty>, secrets: Option<Ty>,
    inputs: Option<Ty>, jobs: Option<Ty>, github_event_inputs: Option<Ty>,  // overlay, no deep copy
    avail_ctx: CtxSet, avail_fns: FnSet, config_vars: Option<&'a [String]>,
    check_untrusted: bool,
}
```
- Variable lookup is a `match` on the lowercased name. `github.event.inputs` is resolved by an overlay check in `check_object_deref`, so there is
  no deep copy at all.
- Overload resolution: compute failures as a small enum (`Arity{..}`, `ArgType{idx, ..}`) and format only if *all* overloads fail.
- Run the untrusted checker as part of the same bottom-up walk (keep the Index-before-Operand order), or as a separate pass over the arena. Use
  `SmallVec<[&'static UntrustedNode; 4]>` for cursors. Represent the untrusted tree as a `static` slice/phf tree.
- Steps context: keep a persistent map (`im-rc::OrdMap` or rebuild `Rc<ObjTy>` per step, since steps are few) so that each expression sees an
  immutable snapshot (this also kills the aliasing hazard).

### 9.6 Case-insensitive maps
- All expression-side keys are ASCII identifiers (`[A-Za-z0-9_-]`), so `to_ascii_lowercase` is correct for lexer-originated names.
  YAML-originated keys (step/job IDs, matrix keys, input names) go through Go's Unicode `strings.ToLower`, so use `str::to_lowercase()` there to match.
  Normalize at insertion (store lowercased keys), not at lookup. Wrap them in a `LowerKey(SmolStr)` newtype so they can't be mixed up.
- Fixes to apply on purpose: lowercase `['literal']` index keys (sema and untrusted) and `fromJSON` object keys.

### 9.7 Byte-compatible error messages (for reusing `testdata/**/*.out`)
- 48 `.out` files contain 126 `[expression]` lines and 21 `[if-cond]` lines. Lines wrapped in `/…/` are regexes (`testdata/err/context_availability.out`). Reuse them directly.
  Also port the table-driven unit tests (`expr_*_test.go`). Most assert `contains(msg, want)` plus columns.
- Put **all message templates in one module** as `const`/`format!` functions, copied verbatim from §1.7, §2.3, §4, §5, §6, §7.
- Implement a Go-compatible `go_quote(&str)` (`strconv.Quote`: `\a\b\f\n\r\t\v\\\"`, `\xNN` for invalid bytes, `\uNNNN`/`\UNNNNNNNN` for
  non-printables using Go's `unicode.IsPrint`, which treats U+00A0 and other spaces as non-printable except ASCII space) and
  `go_quote_rune` (`'\''` for the quote char). `quotes()` = `", "`-joined `go_quote`. Also `ordinal()`.
- `fromJSON` errors: write a small validator that mirrors Go's `encoding/json` scanner messages and `SyntaxError.Offset` semantics. The common ones are
  `unexpected end of JSON input`, `invalid character %s looking for beginning of value`, `... after object key`,
  `... after object key:value pair`, `... after array element`, `... in string literal`, `... in literal true (expecting 'r')`,
  `... in numeric literal`, and `invalid character %s after top-level value`, where `%s` is Go's `quoteChar` (`'x'`, `'\''`, `'"'`, `'\x01'`).
  Alternatively, compare only the prefix `broken JSON string is passed to fromJSON() at offset` in tests.
- Integer-overflow messages embed Go's `strconv` text (`strconv.ParseInt: parsing "…": value out of range`). If int32 limiting is kept for
  parity, hard-code that exact string. Better, fix it (parse as f64) and drop that test.
- Positions: to keep `.out` columns identical, reproduce the column arithmetic in §7.1, i.e. byte offset in host string + rune column in expression, line
  = scalar start. Then optionally offer a `precise_positions` mode that maps through the YAML scalar's source span (requires the YAML layer to expose
  the raw scalar text, style, and indentation). Expect a handful of `.out` diffs for block scalars, escaped quotes, and non-ASCII.
- Behavioural fixes that change output (decide per item, keep behind compat if parity is the goal): continue after the first failing
  `${{ }}` in a string (bug 5), include-combination checking (bug 2), the Deref mutation (bug 1, which strictly *adds* correct errors), `jobs.*.result`, correct
  precedence, case-insensitive literal index and fromJSON keys, int range.

### 9.8 Performance targets
- Parse every expression-bearing YAML string once and cache `(span → Ast + Ty)` on the YAML node, then share it with if-cond, runner-label,
  and other rules (this removes the 2–3× re-lexing).
- No per-expression heap work beyond the AST: borrowed tokens, arena nodes, `Rc` clones of shared types, lazy error formatting.
- Benchmark with `testdata/bench/expressions.txt` (lex / parse / sema / sema+untrusted, as in `expr_test.go:35-116`) and the realworld
  workflows. A fuzz target exists upstream (`fuzz/expr.go`). Port it to `cargo-fuzz` and differential-fuzz against the Go binary.
