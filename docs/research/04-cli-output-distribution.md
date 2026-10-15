# 04 — CLI, linter driver, output, tooling, release & distribution, test infrastructure

Scope: everything around the checks, not the checks themselves. That covers the `actionlint` command, the `Linter` driver, error rendering and `-format`, the playground/WASM build, the test corpus, and how releases are built and distributed. It ends with recommendations for the Rust port (`workflint`).

Upstream snapshot: `.reference/actionlint` (v1.7.12 + HEAD `011a6d15`; `go.mod` requires Go 1.25). Every path below is relative to that directory unless stated otherwise. Statements marked **[probed]** were checked by running a locally built upstream binary (`go build -o <scratch dir>/actionlint ./cmd/actionlint`, plus `.reference/bin/actionlint`) on macOS arm64 with 8 CPUs and shellcheck 0.11.0. pyflakes was not installed.

---

## 1. CLI surface (`command.go`, `cmd/actionlint/main.go`, `man/actionlint.1.ronn`)

### 1.1 Entry point

- `cmd/actionlint/main.go:11-18` builds `actionlint.Command{Stdin, Stdout, Stderr}` and calls `os.Exit(cmd.Main(os.Args))`.
- `main.go:8` has `_ "time/tzdata"`, which embeds the IANA timezone DB in the binary. `rule_events.go:87` needs it for `time.LoadLocation` when validating `on.schedule[].timezone`. **Rust:** bundle a tz database (`jiff` with the `tzdb-bundle-always` feature, or `chrono-tz`). Do not rely on the host's `/usr/share/zoneinfo`, because Windows and scratch Docker images don't have it.
- Flags are parsed by Go's standard `flag` package (`command.go:134`), with `flag.ContinueOnError` and output going to stderr.

### 1.2 Flags (full list, `command.go:136-148`)

| Flag | Type / default | Semantics |
|---|---|---|
| `-ignore PATTERN` | repeatable string list (`ignorePatternFlags`, `command.go:113-121`) | Go RE2 regex, unanchored, matched against `Error.Message` only. It does not see the kind, path or `[kind]` suffix. A match drops the error (`linter.go:649-673`). An invalid regex is a **fatal error, exit 3**, not exit 2 (`linter.go:152-159`) **[probed]**. |
| `-shellcheck CMD` | string, default `"shellcheck"` | Command name or path. The empty string (`-shellcheck=`) disables the rule. It is resolved with `execabs.LookPath`. If that fails, the value is parsed as a shell command line with `go-shellwords` and the first word is looked up, so `-shellcheck="shellcheck -x"`-style values work (`process.go:123-137`). If resolution fails, the rule is silently disabled and only a verbose log line is emitted (`linter.go:574-583`). |
| `-pyflakes CMD` | string, default `"pyflakes"` | Same as `-shellcheck` (`linter.go:584-593`). |
| `-oneline` | bool | Passes `src=nil` to `PrettyPrint`, so there is no snippet (`linter.go:675-682`). It has no effect when `-format` is set. |
| `-format TEMPLATE` | string, default `""` | Go `text/template`. Must contain `{{`, otherwise **exit 3** with `template to format error messages must contain at least one {{ }} placeholder` (`error.go:253-255`). Backslash escapes `\n \t \r \a \b \f \v \\` are unescaped before parsing (`error.go:202-215, 284`). |
| `-config-file PATH` | string | Explicit config. It takes priority over the repo's `.github/actionlint.y{a,}ml` for **every** file (`linter.go:534-540`). A read or parse error is exit 3. |
| `-init-config` | bool | Writes `.github/actionlint.yaml` in the project that contains the cwd (see 1.6). Positional args are ignored. |
| `-no-color` | bool | Forces color off. Applied after `-color`, so it wins if both are given (`command.go:177-182`). |
| `-color` | bool | Forces color on. **Quirk [probed]:** a non-empty `NO_COLOR` env var still wins, because `fatih/color`'s `color.New()` latches `noColor=true` at package init (`error.go:18-23`, `fatih/color@v1.19.0/color.go:173-184`). That is the opposite of the no-color.org guidance that CLI flags override the env var. |
| `-verbose` | bool | Log level 1. Lines are prefixed `verbose: ` and go to stderr (`linter.go:202-208`). |
| `-debug` | bool | Log level 2. Lines are prefixed `[Linter] ` (plus per-rule/visitor prefixes) and go to stderr. **Bug [probed]:** `-verbose -debug` together gives *verbose* level only, because of `if Verbose {…} else if Debug {…}` (`linter.go:119-124`). |
| `-version` | bool | Prints to stdout and exits 0 (format in 1.5). |
| `-stdin-filename NAME` | string, default `"<stdin>"` | File name used for stdin input (`linter.go:177-180`). If a file exists at that path, project detection runs on it, so the repo config and local actions apply (`linter.go:486-495`; CHANGELOG #283). |
| `-h`, `-help` | built-in | Prints the usage header (`command.go:29-67`) plus `PrintDefaults()` to **stderr** and exits **0** (`command.go:153-157`). |

Go `flag` semantics that a drop-in replacement must reproduce:

- Single and double dash are equivalent (`-oneline` == `--oneline`) **[probed]**.
- Values are given as `-f=v` or `-f v`. Bool flags accept `-f`, `-f=true` and `-f=false`, but not `-f false`.
- An empty value is legal (`-shellcheck=`).
- **Parsing stops at the first positional argument or `--`**, so `actionlint a.yaml -oneline` treats `-oneline` as a file. Today that fails with exit 3: `could not read "-oneline"` **[probed]**.
- An unknown flag prints `flag provided but not defined: -nope` followed by the usage to stderr and exits **2** **[probed]**.
- The usage header embeds doc links that point at the `v<version>` tag when the version matches `^\d+\.\d+\.\d+$`, and at `main` otherwise (`command.go:30-34`).

### 1.3 Positional arguments and stdin

`Command.runLinter` (`command.go:92-111`):

1. `-init-config` → `GenerateDefaultConfig("")`.
2. No args → `LintRepository("")`, which discovers files (1.4).
3. **Exactly one arg equal to `-`** → `LintStdin(cmd.Stdin)`. `-` is only special when it is the sole argument. `actionlint - a.yaml` tries to open a file named `-` and exits 3 **[probed]**.
4. Otherwise → `LintFiles(args, nil)`. Directories are **not** accepted as args. `os.ReadFile` fails on them and the run exits 3.

Stdin is read fully (`io.ReadAll`, `linter.go:474-481`). The literal `"<stdin>"` is hardcoded in `Lint` to skip project detection (`linter.go:487`). Empty input reports `workflow is empty [syntax-check]` with exit 1 **[probed]**.

### 1.4 File discovery with no args (`linter.go:264-311`, `project.go`)

- `Projects.At(cwd)` → `findProject` walks up from the absolute cwd. It looks for a directory that has **both** `.github/workflows/` (a directory) **and** `.git` (a file or directory, so worktrees and submodules work) (`project.go:24-39`). If none is found, exit 3: `no project was found in any parent directories of "…"` **[probed]**.
- `LintDir(<root>/.github/workflows)` runs `filepath.Walk` **recursively**, so it includes subdirectories even though GitHub only runs top-level files. It collects paths whose suffix is `.yml` or `.yaml` (case-sensitive), sorts them with `sort.Strings`, and errors with exit 3 if none are found (`no YAML file was found in …`).
- The project's config (`.github/actionlint.yaml`, then `.yml`) is loaded once when the `Project` is created (`project.go:43-49`, `config.go:120-134`).
- `Projects` caches the projects it has found. Lookup uses `Project.Knows` = `strings.HasPrefix(abs(path), root)`, which has a **sibling-prefix bug**: `/work/repo` "knows" `/work/repo-other/...` (`project.go:64-67`; reported upstream).
- Explicit file args resolve their project per file, from the file's own path (`linter.go:351-360`). A file outside any git repo gets `project=nil`, which means no config and null local-action/reusable-workflow caches.

### 1.5 `-version` output (`command.go:161-171`)

```
<version>
<installedFrom>
built with <go version> compiler for <GOOS>/<GOARCH>
```

- `version` and `installedFrom` are package vars that GoReleaser overrides with `-ldflags -X` (`command.go:12-16`, `.goreleaser.yaml:14`). The defaults are `""` and `"installed by building from source"`.
- If `version` is empty, `debug.ReadBuildInfo().Main.Version` is used. That gives `v1.7.13-0.2026…-011a6d15e749` for `go install`/`go build` inside the module **[probed]**, `(devel)` under `go test`, and `unknown` outside a module (`command.go:69-80`).
- Docker builds set only the version (`Dockerfile:10`).
- Release binaries print `installed by downloading from release page`. `scripts/test-download-actionlint.bash:29,37` greps for that string and uses `head -n 1` as the version. **Keep line 1 as the bare version** if you want existing scripts to keep working.

### 1.6 `-init-config` (`linter.go:225-258`, `config.go:136-165`)

- Requires a project, found the same way as discovery. Error text: `project is not found. check current project is initialized as Git repository and ".github/workflows" directory exists`.
- Refuses to run if `.github/actionlint.yaml` or `.github/actionlint.yml` already exists.
- Writes a commented template (`labels: []`, `config-variables: null`, `paths:` with commented examples) with mode 0644 and prints `Config file was generated at "<path>"` to **stdout**.
- Ignores `-config-file`.

### 1.7 Exit codes (`command.go:18-27, 184-193`; `docs/usage.md:181-190`)

| Code | Meaning | Triggers |
|---|---|---|
| 0 | No problems | Includes `-h`, `-version`, a successful `-init-config`, and runs where every error was filtered by `-ignore` or config |
| 1 | Problems found | `len(errs) > 0` after filtering |
| 2 | Invalid command-line option | Only `flag.Parse` failures |
| 3 | Fatal | Unreadable file or dir, no project, no YAML files, invalid `-ignore` regex, bad/unparseable `-format` template, config read/parse error, `shellcheck` returning non-JSON, a template *execution* error in multi-file mode, and similar. The message goes to stderr via `err.Error()`. |

**Bug [probed]:** template *execution* errors are swallowed for single-file and stdin runs. `LintFile`/`Lint` ignore the return value of `errFmt.PrintErrors` (`linter.go:463-468, 505-509`). `-format '{{.Foo}}' one.yaml` prints nothing and exits 1. With 2+ files the error propagates and the exit code is 3 (`linter.go:415-417`).

A fatal error from any one file aborts the whole run with **no lint output** (errgroup's first error, `linter.go:388-390`).

### 1.8 Streams

- Lint results go to **stdout**.
- Logs, usage and fatal messages go to **stderr**.
- On Windows, stdout is wrapped with `go-colorable` so ANSI escapes work on legacy consoles (`linter.go:132-135`).

### 1.9 Color decision

Color is controlled by the `fatih/color` global `NoColor`. At init it is set to `NO_COLOR != ""` OR `TERM == "dumb"` OR `!isatty(os.Stdout)` (`fatih/color/color.go:22`). The check is always against **os.Stdout**, not the writer actually passed. `-color` sets `NoColor=false` and `-no-color` sets `NoColor=true` (`linter.go:126-131`). `FORCE_COLOR` and `CLICOLOR_FORCE` are **not** honored.

---

## 2. Linter driver (`linter.go`, `process.go`)

### 2.1 Construction (`NewLinter`, `linter.go:118-200`)

`NewLinter` resolves the log level, color, `-config-file` (read once and used as `defaultConfig`), the compiled ignore regexes, the `ErrorFormatter`, the cwd (`opts.WorkingDir` or `os.Getwd`) and the stdin filename. It also creates an empty `Projects` cache, so projects are cached for the lifetime of the `Linter`.

### 2.2 Entry methods

`LintRepository` → `LintDir` → `LintFiles` (sorted list) → `LintFile` when n == 1. `LintStdin` → `Lint(path, bytes, project)`. All of them return `([]*Error, error)` **and also print** to the `out` writer as a side effect.

### 2.3 Concurrency model (`LintFiles`, `linter.go:316-429`)

- **n == 1 takes a different path:** `LintFile` runs serially, with fresh caches and its own `concurrentProcess`.
- **n > 1:**
  - `cpus := runtime.NumCPU()`.
  - One `concurrentProcess` (a weighted semaphore of `cpus`) is shared by **all** external processes (shellcheck and pyflakes) across all files (`linter.go:328-329`).
  - A second semaphore of `cpus` bounds **file reads only**, to avoid EMFILE (issue #3; `linter.go:330, 365-368`).
  - Projects are resolved **serially before spawning**, because `Projects` is not thread-safe (`linter.go:351-360`).
  - **One goroutine per file**, unbounded, via `errgroup.Group` (`linter.go:347-386`). CPU parallelism is whatever GOMAXPROCS gives.
  - After `eg.Wait()` comes `proc.wait()`. The ordering comment at `linter.go:392-398` explains a WaitGroup race.
- Paths are rewritten relative to cwd with `filepath.Rel` when possible (`linter.go:373-377, 447-451`). That is why running from a subdirectory prints `../.github/workflows/x.yaml` **[probed]**.
- External processes (`process.go`):
  - Each `run:` script is a separate `shellcheck --norc -f json -x --shell <sh> -e SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043 -` invocation, with `set -e` (or `set -eo pipefail` for bash) prepended (`rule_shellcheck.go:193-201`).
  - Stdin is passed as an `io.Reader`, after the deadlock fix for #650/#702 (`process.go:27-31`).
  - A non-zero exit with non-empty stdout is treated as "findings". A non-zero exit with empty stdout is a fatal error (`process.go:41-62`).
  - There is **no caching or dedupe of identical scripts**.
- **Measured [probed], 1503-file real-world corpus** (`.reference/realworld/`): `-shellcheck= -pyflakes=` takes **0.27 s wall** (272% CPU) and produces 6836 errors. With shellcheck enabled, a 50-file subset takes **18 s**, versus under 1 s without. The full corpus with shellcheck ran past 5 minutes and was killed. **Spawning shellcheck once per script is by far the biggest cost.**

### 2.4 Caches shared across files

- `LocalActionsCacheFactory` and `LocalReusableWorkflowCacheFactory` create **one cache per project root**, shared by every file of that project within one `LintFiles` call (`linter.go:333-334, 361-362`; `action_metadata.go:305-330`; `reusable_workflow.go:403-414`). Inside, the caches use `sync.RWMutex` (`action_metadata.go:196`, `reusable_workflow.go:170`). The factories themselves are not thread-safe, which is why `GetCache` is called before the goroutine starts. Caches are **not** kept across separate `Lint*` calls.
- `Project.config` is parsed once per project.
- Popular-action metadata, webhooks and availability tables are static globals (generated `.go` files).
- Rules are **instantiated fresh per file** inside `check()` (`linter.go:557-596`). That includes a `LookPath` for shellcheck/pyflakes per file.

### 2.5 Per-file pipeline (`check`, `linter.go:513-647`)

1. Pick the config. `-config-file` wins over `project.Config()` (`linter.go:534-540`).
2. `Parse(content)` returns the workflow plus syntax errors.
3. If the workflow is non-nil, build the rule list: 15 built-in rules, then shellcheck and pyflakes if they resolve, then `OnRulesCreated(rules)` if the hook is set. Then `Visitor.AddPass` per rule, `SetConfig`, `Visit`, and collect `rule.Errs()`. Each rule is also registered with the formatter for `allKinds`, under a mutex (`error.go:309-321`; fix for #370).
4. **Filter:** drop errors matching CLI `-ignore` or any `paths:` entry whose glob matches `cfg.PathConfigs(path)` (`linter.go:632`).
   - **Bug [probed]:** `path` here is the **cwd-relative** display path, not the repo-root-relative path that `docs/config.md:54` documents. Run from a subdirectory, the path becomes `../.github/workflows/t.yaml`, the `paths:` globs never match, and the ignores silently stop working. This is likely the cause of upstream issue #481 ("Unable to ignore errors by `paths` config").
5. Set `err.Filepath = path` on every error.
6. **Sort** with `compareErrors`: by (Filepath, Line, Column, Message). Kind is not part of the key (`error.go:161-172`).
7. **Dedupe adjacent duplicates** with `slices.CompactFunc(equalsErrors)` on (Filepath, Line, Column, Message), again ignoring kind. YAML aliases can otherwise produce duplicate reports (`linter.go:638-639`, `error.go:174-179`).

### 2.6 Output ordering

- All output is **buffered until every file is done**. Nothing streams.
- Files are printed in input order, which is sorted order for `LintDir` and argument order for explicit files. Errors within a file are sorted.
- With `-format`, the template is executed **once** over the flat list of all errors from all files (`linter.go:406-417`).
- Duplicate file arguments are not deduplicated.

### 2.7 Logging

- `log()` writes `"verbose: "` and the message as **two separate writes** with no lock. Concurrent files interleave them, for example `verbose: verbose: Found 1 parse errors…` **[probed]** (`linter.go:202-208`).
- Verbose mode prints per-file timings and reasons a rule was disabled. Debug mode dumps options, config, the AST visit and the shellcheck scripts.

---

## 3. Output

### 3.1 Default human formatter (`error.go:93-159`)

Exact byte layout. `Y` = yellow `\x1b[33m`, `G` = gray `\x1b[90m`, `B` = bold `\x1b[1m`, `Gr` = green `\x1b[32m`, and each colored segment ends with `\x1b[0m`:

```
Y{path} G{:} {line} G{:} {col} G{: } B{message} G{ [kind]\n}
G{<indent>|\n} G{<lnum> | }<raw line>\n
G{<indent>| } Gr{<indicator>\n}
```

- The reset for the gray ` [kind]\n` segment comes **after** the newline, so the next line starts with `\x1b[0m` **[probed with `cat -v`]**. Upstream's problem-matcher fixture `scripts/generate-actionlint-matcher/test/escape.txt` contains these exact bytes.
- `lnum = "%d | "` and `indent = " " * (len(lnum) - 2)`. Line 10 and above widen the gutter (`error_test.go:178-186`).
- A snippet is printed only if all of these hold: `len(source) > 0`, `Line > 0`, the line exists, and `len(line) >= Column-1` in bytes (`error.go:106-112`). For an out-of-range column, only the header is printed.
- Lines come from `bufio.Scanner` (`error.go:123-133`). It strips a trailing `\r`, and **for any line over 64 KiB the scanner fails** and no snippet is printed.
- Indicator (`getIndicator`, `error.go:135-159`):
  - The `^` is preceded by `runewidth.StringWidth(line[:Column-1])` spaces.
  - It is followed by `~` × (the summed `runewidth.RuneWidth` of the non-blank run starting at the column, minus 1). The run ends at space, `\t`, `\n` or `\r`.
  - If `Column <= 0`, the indicator is `""`. The output then still contains a trailing `"  | "` line (`error_test.go:194-202`).
- Width quirks:
  - `go-runewidth` treats `\t` as width **0**, so tabs before the column shift the caret.
  - `go-runewidth` picks the East-Asian-ambiguous width from the locale (`RUNEWIDTH_EASTASIAN`, `LC_*`/`LANG`). Ambiguous characters count as 2 in CJK locales.
- **Correctness bug [probed]: inconsistent column units.**
  - YAML-node positions are **rune-based**. `parse.go:33` copies the yaml.v4 `Node.Column`.
  - Positions inside `${{ }}` expressions are **byte offsets** added to the node column.
  - `getIndicator` slices `line[:Column-1]` by **bytes**.
  - Result: with multibyte characters before a YAML-level error, the caret lands on the wrong token. Example: `steps: [{name: 日本語, run: echo, foo: 1}]` reports col 36 (correct in runes), but the caret sits under `echo,`.
  - With multibyte characters before an expression error, the caret is right, but the reported column is in bytes. `"日本語" ${{ matrix.foo }}` reports col 35, while the character column is 29.
- `-oneline` output is exactly `Error.Error()` + `\n` (`error.go:40-42`): `path:line:col: message [kind]`.

### 3.2 `-format` (Go `text/template`)

- **Input value:** `.` is `[]*ErrorTemplateFields` (`error.go:181-200`). Fields:
  - `Message`
  - `Filepath` (the JSON tag says omitempty, but stdin input still gets `"<stdin>"`)
  - `Line`, `Column`
  - `Kind`
  - `Snippet`: the source line + `"\n"` + the indicator. It is just the line if the indicator can't be built, and empty if there is no source.
  - `EndColumn`: `len(indicator)`, or `Column` when there is no indicator.
  - **Bug:** `EndColumn` is a *display width*, so it doesn't match the unit of `Column` for non-ASCII. The probe gave 37 where the bytes imply 41 and the runes imply 40.
  - JSON key order: `message, filepath, line, column, kind, snippet, end_column`.
- **Custom functions** (`error.go:261-283`; `docs/usage.md:149-162`):
  - `json x`: `encoding/json` **with HTML escaping** (`<`→`<`, `&`→`&`, `>`→`>`, as probed; U+2028/2029 are escaped too). Always ends with `\n`. `{{json .}}` with zero errors prints `[]\n` **[probed]**.
  - `replace s old new [old new…]`: `strings.NewReplacer`.
  - `toPascalCase s`: splits on non-alphanumerics and upper-cases the first ASCII letter of each part (`error.go:217-231`; tests at `error_test.go:625-659`).
  - `allKinds`: the rule `{Name, Description}` objects **registered so far, sorted by name**. Always includes `syntax-check`. It only contains rules that actually ran, so `shellcheck` and `pyflakes` appear only when enabled **[probed]**. That makes the SARIF `rules` array environment-dependent. The JSON of a kind object uses Go field names (`{"Name":…,"Description":…}`, no tags).
  - `getVersion`: the same string as `-version` line 1. The stale doc comment at `error.go:309-311` mentions `kindDescription`/`kindIndex`, which don't exist.
- **Go built-ins that real templates use:**
  - `range` in both forms, `{{range $err := .}}` and the **assignment form** `{{range $ = .}}` (`error_test.go:449-459`, `docs/usage.md:154`)
  - variable declaration and **re-assignment** (`{{$first := true}}…{{$first = false}}` in the SARIF template)
  - `if/else/end`, pipelines `|`, `printf "%q"`, `index`, field access on `$`
  - Go-syntax string literals with escapes
  - Everything else in `text/template` (`with`, `len`, `eq`/`ne`/`lt`…, `and`/`or`/`not`, `print`/`println`, `html`/`js`/`urlquery`, `{{- -}}` trim markers, comments, `define`/`template`/`block`) is reachable by users.
- **Documented recipes users copy** (`docs/usage.md:52-120`): JSON (`{{json .}}`), JSON Lines (`{{range $err := .}}{{json $err}}{{end}}`), Markdown, GitHub `::error file=…` annotations (which escape only `\n`→`%0A`, not `%`, `\r`, `:` or `,`), and SARIF via `testdata/format/sarif_template.txt`.
- **SARIF:** produced entirely by a template. There is no SARIF code in Go. The template (`testdata/format/sarif_template.txt`) emits schema 2.1.0 with tool name `"GitHub Actions lint"`, `version` from `getVersion`, `rules` from `allKinds` (id, PascalCase name, description, `helpUri` → docs/checks.md), and `results` with ruleId, message, `artifactLocation.uri` + `uriBaseId: %SRCROOT%`, `startLine`, `startColumn`, `endColumn` and `snippet.text`. Columns aren't declared as UTF-16 code units, which SARIF's default `columnKind` assumes.

### 3.3 GitHub Actions problem matcher (`.github/actionlint-matcher.json`)

- Owner is `actionlint`. A single-line regex tolerates ANSI SGR codes anywhere: `^(?:\x1b\[\d+m)?(.+?)(?:\x1b\[\d+m)*:(?:\x1b\[\d+m)*(\d+)…: (?:\x1b\[\d+m)*(.+?)(?:\x1b\[\d+m)* \[(.+?)\]$`, with groups file=1, line=2, column=3, message=4, code=5.
- The JSON is generated by `scripts/generate-actionlint-matcher/object.mjs`.
- `test.mjs` checks the regex against `test/escape.txt` (`-color`), `test/no_escape.txt` (`-no-color`) and `test/want.json`, and against every non-regex line of `testdata/{examples,err}/*.out`. All of that is regenerated by `Makefile:93-101` and enforced in CI by `.github/workflows/matcher.yaml`.
- Usage is `echo "::add-matcher::.github/actionlint-matcher.json"` before running `actionlint -color` (`docs/usage.md:328-344`).

### 3.4 What must stay compatible

1. **The default line format** `path:line:col: message [kind]`. Problem matchers, reviewdog's `actionlint` errorformat, editor plugins (ALE, nvim-lint, flycheck/flymake, VS Code linter extension, Pulsar, Nova; `docs/usage.md:382-411`), super-linter and trunk all parse it.
2. **The bytes of the colored output**, as far as the matcher regex is concerned.
3. **`-format '{{json .}}'` and the JSONL recipe:** field names, order and semantics. Many integrations use it.
4. **`-format` with arbitrary Go templates**, at least the subset in the docs and test fixtures: `range` (both forms), variables with `:=` and `=`, `if/else`, pipelines, `printf` (incl. `%q`), `index`, `len`, comparison and logic functions, plus the five custom functions.
5. **Exit codes 0/1/2/3, `-version` line 1, and the flag names with single-dash Go syntax.**

### 3.5 How a Rust port can handle Go templates

| Option | Pros | Cons |
|---|---|---|
| **`gtmpl` crate** (Go-template port) | Exists. Supports range/if/with/pipelines/vars and custom funcs. | Unmaintained-ish. Unclear support for the Go 1.11+ **assignment** (`$x = …`, `range $ = .`), which the SARIF template and the upstream tests require. `printf`/`%q` fidelity is uncertain. **Verify before adopting.** |
| **Own `gotmpl` crate**: port `text/template/parse` (lexer + parser) and a focused `exec` | Full control. Deterministic. Can match Go byte-for-byte for the subset users actually use. ~2–3 kLOC. Go's BSD license is compatible; keep the notice. | Engineering cost. Needs Go-exact `printf` verbs (`%s %d %v %q %x`; `%q` = `strconv.Quote`) and Go-exact printing of values (`{{.}}` of a struct, nil, maps sorted by key). |
| **Built-in named formats only** | Fast, correct, easy to evolve. | Breaks every user with a custom template. |

**Recommendation: do both.**

- (a) Write our own Go `text/template`-compatible engine. Start with a whitelist: lexer and parser complete, exec covering actions, pipelines, variables (including assignment), range/if/with/else/else-if, comments, trim markers, define/template/block, and the builtins `and or not len index slice eq ne lt le gt ge print printf println html js urlquery call`. Test it against cases lifted from Go's `text/template/exec_test.go` and against upstream's format fixtures.
- (b) Add **named formats** to `-format`. Every value without `{{` is currently rejected with exit 3 (`error.go:253`), so `-format json`, `jsonl`, `sarif`, `github` (properly escaped workflow commands), `checkstyle`, `junit`, `gitlab` (codeclimate) and `rdjson` (reviewdog) are a **strictly backward-compatible** extension.
- Keep `{{json}}` byte-compatible: HTML escaping and the trailing `\n`.
- Native SARIF should declare `columnKind` and emit stable rule metadata for all rules, not only the ones that ran.

---

## 4. Public Go library API (`docs/api.md`, `doc.go`, `example*_test.go`)

- The surface is large: `go doc -short .` lists about 145 exported types, 6 functions, 10 vars and 3 consts.
- `docs/api.md:18-47` highlights:
  - `Command`, with `Command.Main(args) int` (`command.go:83-90, 126`)
  - `Linter`, `LinterOptions` and `NewLinter`, plus `LintFile/LintFiles/LintDir/LintRepository/LintStdin/Lint` and `GenerateDefaultConfig`
  - `LinterOptions.OnRulesCreated` for adding or removing rules (`linter.go:89-93`; `example_your_own_rule_test.go`; `linter_test.go:560-647`)
  - `Project`, `Projects`, `Config`, `ParseConfig`, `ReadConfigFile`
  - `Error`, `ErrorFormatter`, `NewErrorFormatter`, `ErrorTemplateFields`
  - the AST nodes (`Workflow`, `Job`, `Step`, …) and `Parse()`
  - `Pass`, `Visitor`, `Rule`, `RuleBase` (`NewRuleBase`, `Error`, `Errorf`, `Debug`) and every `NewRule*` constructor
  - `ExprLexer`, `ExprParser`, `ExprNode`, `ExprType` (and its implementations), `ExprSemanticsChecker`
  - `ValidateRefGlob`, `ValidatePathGlob`, `ActionMetadata`, the `PopularActions` and `AllWebhookTypes` globals, and `WorkflowKeyAvailability()`
- **Explicitly no semver for the library** (`docs/api.md:49-52`, `doc.go:17-21`). Patch releases may break it.
- What external users depend on: mostly the **CLI contract** in 3.4. Go importers (custom rules via `OnRulesCreated`, embedding the parser or expression checker, `ErrorFormatter`) can't be served by a Rust port.
- **Mitigation:**
  - Publish a Rust library crate with an analogous shape: `Linter`, `LinterOptions`, `Diagnostic`, a `Rule` trait with visitor hooks, and `parse()`.
  - Publish `@workflint/wasm`, a JS/TS API.
  - Document a **stable JSON diagnostics schema**, so non-Rust consumers integrate through the CLI.

---

## 5. Playground / WASM

### 5.1 Today (`playground/`)

- **Build:** `GOOS=js GOARCH=wasm go build -o main.wasm` (`playground/Makefile:19-20`), glued with Go's `wasm_exec.js` copied from `$(go env GOROOT)/lib/wasm` (`post-install.bash:20-25`). `deploy.bash:48-51` runs `wasm-opt -O --enable-bulk-memory`. The binary is about **9 MB** (CHANGELOG ~v1.6.x: "9.4MB to 8.9MB").
- **API:** `main.go` sets the global `window.runActionlint(src)`. It calls `Linter.Lint("test.yaml", src, nil)` with default options: no project, so no local actions, reusable workflows or config. Shellcheck and pyflakes are effectively absent because there is no `exec`. It reports results through `window.onCheckCompleted([{message,line,column,kind}])`, with `window.showError` and `window.dismissLoading` callbacks (`playground/main.go:20-62`, `lib.d.ts`).
- **UI:** CodeMirror 5, Bulma, TypeScript compiled by `tsc`, mocha + jsdom tests that load `main.wasm` (`test.ts`), and eslint/prettier/stylelint.
- **Source input** (`index.ts:61-79`): `?s=<raw source>`, `?u=<url to fetch>`, or `#<base64(zlib-deflate(source))>`. The **permalink format** is generated by pako (`index.ts:308-314`), by `scripts/yaml-to-playground-url.js`, and by `scripts/check-checks/main.go:129-154` (zlib BestCompression). **All links in docs/checks.md use it.**
- **Deploy:** `playground/deploy.bash` builds, tests, copies assets plus `man.html` (ronn) to the `gh-pages` branch, and serves locally for a final check. The push is manual. CI only builds, lints and tests it (`.github/workflows/ci.yaml:66-88`).

### 5.2 Rust equivalent

- **Target:** `wasm32-unknown-unknown` + `wasm-bindgen` (`wasm-pack` or `wasm-bindgen-cli`), then `wasm-opt -Oz`. Export something like `lint(source: string, opts?: {fileName?, config?: string, files?: Record<string,string>}) -> Diagnostic[]` via `serde-wasm-bindgen`, and `version()`.
- **Gate behind a cargo feature or `cfg(target_arch="wasm32")`:**
  - `std::process` (no shellcheck or pyflakes)
  - `std::fs` (replace with a virtual-FS trait, which also lets the playground support local actions and reusable workflows from an in-memory file map)
  - rayon (single-threaded)
  - tz database: keep it bundled, it's pure data
- **Size:** expect about **1–3 MB** before compression. The data tables dominate: popular actions are 275 KB of Go source, plus tzdb. That is a large improvement over 9 MB. Use `opt-level="z"`, `lto`, `codegen-units=1`, `panic=abort` in a dedicated profile.
- **Product:** keep the **same permalink hash format** (`#base64(zlib)`) so existing links keep working. Ship the same WASM as the npm package `@workflint/wasm` for Node/browser/VS Code-web consumers.

---

## 6. Testing infrastructure and a conformance plan

### 6.1 Fixture layout and conventions

| Dir | Count | Driver | Convention |
|---|---|---|---|
| `testdata/examples/` | 51 `.yaml` + 51 `.out` (148 expected lines) + `.github/actions/{my-action,my-action-with-output,my-invalid-action}` | `TestLinterLintError` (`linter_test.go:155-216`) | Mirrors every example in `docs/checks.md`. |
| `testdata/err/` | 95 `.yaml` + 95 `.out` (497 lines) + `.github/workflows/called-workflow.yml` | same | One expected error per line. |
| `testdata/ok/` | 84 `.yaml` | `TestLinterLintOK` (`linter_test.go:28-83`) | Must produce zero errors. Shellcheck and pyflakes are **enabled when present**, and tests skip if the file name contains `shellcheck`/`pyflakes` and the tool is missing. |
| `testdata/projects/` | 30 project dirs (107 YAML files) + 30 `.out` (117 lines); `README.md` | `TestLinterLintProject` (`linter_test.go:313-351`) | `WorkingDir` = the project dir. `<proj>/actionlint.yaml`, if present, is passed as `ConfigFile`. `LintDir(<proj>/workflows)`. Note: `workflows/`, **not** `.github/workflows`. Paths in `.out` are relative to the project. An empty `.out` means no errors. |
| `testdata/format/` | `test.yaml` → `test.json`, `test.jsonl`, `test.md` (exact compare) and `test.sarif` (structural JSON compare), via `sarif_template.txt`; `README.md` explains regeneration | `linter_test.go:353-470` | SARIF `version` is `"(devel)"` because of `go test` build info. |
| `testdata/config/` | `ok.yml`, `broken.yml`, `projects/{ok,err,yml,none}` | `config_test.go`, `linter_test.go:649-669` | |
| `testdata/action_metadata/` | 13 action dirs | `action_metadata_test.go` | |
| `testdata/reusable_workflow_metadata/` | 6 files | `reusable_workflow_test.go` | |
| `testdata/find_project/` | 1 workflow + 1 reusable | `TestLintFindProjectFromPath`, `project_test.go` | `.git` is created at test time (`testEnsureDotGitDir`). |
| `testdata/bench/` | `minimal.yaml` (7 lines), `small.yaml` (21), `large.yaml` (344), `many_scripts.yaml` (146), `expressions.txt` (1858 real-world `${{ }}` bodies) | benchmarks + `TestExprSemanticsCheckRealWorld` (`expr_test.go:10-33`) | |
| `testdata/realworld/dataset.zip` | **1503** real workflows from popular repos (2021 snapshot) | **not referenced by any test** | Unzip by hand; `*.y*ml` are gitignored. Already extracted at `.reference/realworld/`. |
| `docs/checks.md` | 52 `Output:` blocks (6 marked `<!-- Skip update output -->`) | `scripts/check-checks` (`-fix` / check mode), run in CI lint | **Full pretty output with snippets** (no color, shellcheck + pyflakes on, `Lint("test.yaml", src, project=repo root)`, abs root replaced by `/path/to/repo`; `scripts/check-checks/main.go:19-52`). |
| `scripts/generate-actionlint-matcher/test/` | `escape.txt`, `no_escape.txt`, `want.json` | `test.mjs` | Byte-exact colored output of `testdata/err/one_error.yaml`. |

**How `.out` files are compared** (`checkErrors`, `linter_test.go:107-153`):

1. Read the file line by line and sort the actual errors with `compareErrors`. The counts must be equal.
2. Normalize paths to forward slashes (for Windows).
3. Each line is either an **exact** match of `Error.Error()` (`test.yaml:LINE:COL: MESSAGE [KIND]`) or, if it starts **and** ends with `/`, a **Go regex** (unanchored) applied to that string. 160 lines across err/examples/projects use the regex form, mostly to elide long "available labels/contexts are …" lists and shellcheck text.

**Snippets and indicators are never checked by `.out` files.** Only `docs/checks.md`, `testdata/format/*` and the matcher fixtures check rendering.

Run conditions for err/examples:

- Path `"test.yaml"`.
- `Project{root: <fixture dir>}`.
- `defaultConfig = &Config{}`, so the repo config is ignored.
- shellcheck/pyflakes enabled **only when the file name contains** `shellcheck`/`pyflakes`; the test is skipped if the tool is missing.

`TestLinterLintAllErrorWorkflowsAtOnce` (`linter_test.go:218-272`) runs all examples and err files in one `LintFiles` call to exercise concurrency. It asserts that each file produced at least one error.

Other tests of note:

- `command_test.go` and `ExampleCommand`: end-to-end `Main` with `-ignore` and the exit code.
- `process_test.go`: semaphore, concurrent stdin with a 64 KiB payload as the deadlock regression for #650, shellwords parsing, and the non-zero-exit handling.
- Example tests are golden-output Go examples.
- Fuzzing uses legacy **go-fuzz** (dvyukov) on four targets: `FuzzParse`, `FuzzCheck`, expr and glob (`fuzz/*.go`, `Makefile:79-83`, `CONTRIBUTING.md:130-148`). It is not run in CI.

### 6.2 Benchmarks (`make bench` = `go test -bench Lint -benchmem`, `Makefile:90-91`)

- `BenchmarkLintWorkflowFiles` (`linter_test.go:671-819`): end-to-end `LintFiles` (incl. `NewLinter`) over minimal×{1,5,20,40}, small×{1,3,10}, large×{1,3,10}, small×{1,3,10} with shellcheck, many_scripts×{1,3,10} with shellcheck, many-smalls with a markdown `-format`, and the repo's own `.github/workflows`. It measures throughput and the external-process overhead.
- `BenchmarkLintWorkflowContent` (`linter_test.go:821-864`): `Lint` on in-memory content (no I/O, no external tools) for minimal, small and many_scripts.
- `BenchmarkExamplesLintFiles`: all examples with shellcheck and pyflakes. `BenchmarkLintRepository`: the whole repo.
- `BenchmarkParseWorkflow` (`parse_test.go:9-36`): `Parse` on minimal, small and large. `BenchmarkParseTestData`: Parse over all of examples, ok and err.
- `BenchmarkExprRealWorld` (`expr_test.go:35+`): Lex, LexParse, LexParseSema, Sema-untrust and Sema-trust over `expressions.txt`.
- `BenchmarkInsecureDetectUntrustedInputs` (`expr_insecure_test.go:608`).

### 6.3 Reusing the upstream corpus as a Rust conformance suite

1. **Vendor upstream `testdata/`, `docs/checks.md` and `scripts/generate-actionlint-matcher/test/`** into `tests/upstream/`, pinned to a commit SHA, with a sync script and the MIT notice. Keep `tests/workflint/` for intentional divergences, plus an explicit `divergences.toml` that lists fixture, reason and issue.
2. **A harness that replicates the Go semantics exactly:** use `libtest-mimic` or `datatest-stable` to generate one test per fixture. For each fixture:
   - err/examples: virtual path `test.yaml`, project root = the fixture dir, empty config, tool gating by file name, sorted `Display` strings compared exactly or by regex. The regex lines are Go RE2; the Rust `regex` crate is RE2-compatible for these patterns.
   - ok: zero errors.
   - projects: cwd = the project dir, `actionlint.yaml` passed as an explicit config, recursive `workflows/` scan.
   - format: byte-exact for json/jsonl/md, structural for SARIF, with the version injectable as `(devel)`.
3. **Render goldens:** parse the 46 auto-generated `Output:` blocks of `docs/checks.md` into (input YAML, expected uncolored pretty output) pairs. That is the only upstream oracle for snippet and caret rendering. Add the matcher fixtures to test colored output byte for byte.
4. **Differential testing against the Go binary** (`.reference/bin/actionlint`):
   - Run both on every fixture **and** on the 1503 real-world workflows with `-format '{{json .}}' -no-color`, and with the default formatter.
   - Diff the results, and use `insta` snapshots for the accepted state.
   - Run in CI with a **pinned shellcheck version** (0.11.0 here). Shellcheck messages change between versions, which is why upstream uses regex lines.
   - Also run the differential with `-shellcheck= -pyflakes=` so the comparison is deterministic and fast.
5. **Fuzzing:** use `cargo-fuzz` targets for `parse`, `expr` and `glob`, plus a **differential fuzz** mode that shells out to the Go binary and compares diagnostics (corpus seeded from the fixtures and real-world files).
6. **Benchmarks:** port the Go benchmarks to `divan` or `criterion` on the same `testdata/bench` inputs. Add `hyperfine` end-to-end comparisons, workflint vs actionlint, on the real-world corpus, with and without shellcheck. Baseline today: **0.27 s for 1503 files without external tools**, and **~0.36 s per file with shellcheck**.

---

## 7. Release and distribution today

| Channel | Mechanism | Files |
|---|---|---|
| GitHub Releases | Tag `v*.*.*` → `release.yaml`. Build the man page with ronn, then GoReleaser v2 `release --clean`, then `actions/attest-build-provenance` over the checksums file (since v1.7.11), then a post-release check with the download script, then a bot commit that bumps `version=` in `download-actionlint.bash`. The release is created as **prerelease: true**; notes are written and published by hand. | `.github/workflows/release.yaml:1-53`; `.goreleaser.yaml` |
| Binaries | `CGO_ENABLED=0`; ldflags `-s -w -X …version={{.Version}} -X "…installedFrom=installed by downloading from release page"`. Targets: darwin amd64/arm64; linux 386/arm(v6)/amd64/arm64; windows 386/amd64/arm64; freebsd 386/amd64. | `.goreleaser.yaml:11-34` |
| Archives | `actionlint_<ver>_<os>_<arch>.tar.gz` (`.zip` on Windows; `armv6` for arm), containing README.md, LICENSE.txt, `docs/` and `man/actionlint.1`. `actionlint_<ver>_checksums.txt` (sha256). | `.goreleaser.yaml:36-53`; `download-actionlint.bash:112-114` |
| Homebrew | (1) The **official homebrew-core formula** `brew install actionlint` (built from source by Homebrew). (2) A **cask** in this repo (`Casks/actionlint.rb`, auto-generated by GoReleaser `homebrew_casks`, `depends_on formula: shellcheck`, installs the man page). Install with `brew tap rhysd/actionlint https://github.com/rhysd/actionlint && brew install --cask actionlint`. The legacy tap formula was removed in favor of the cask because GoReleaser deprecated `brews`. Cask installs of the unsigned binary trigger Gatekeeper quarantine. | `.goreleaser.yaml:55-69`; `docs/install.md:63-83`; CHANGELOG:74,185,216 |
| Docker | Multi-stage: `golang` builder → `koalaman/shellcheck-alpine` → `alpine` + `py3-pyflakes`, `USER guest`, ENTRYPOINT actionlint. Pushed to Docker Hub as `rhysd/actionlint:<ver>` and `:latest` for linux/amd64 and arm64 via buildx + QEMU (it compiles inside Docker). CI builds, runs it on the repo and runs hadolint. | `Dockerfile`; `release.yaml:64-92`; `ci.yaml:119-137` |
| Download script | `bash <(curl …/scripts/download-actionlint.bash) [VERSION|latest] [DIR]`. Detects `$OSTYPE`/`uname -m`, pipes curl into tar (unzip on Windows), prints `-version`, and sets the `executable=` step output (or the legacy `::set-output` on GHES). **It does not verify checksums or attestations.** The default version is hardcoded and bumped by CI. Tested on ubuntu, macos and windows by `download.yaml` / `test-download-actionlint.bash`. | `scripts/download-actionlint.bash` |
| winget | `vedantmgoyal9/winget-releaser` job, `rhysd.actionlint` | `release.yaml:54-63` |
| Others (community) | Chocolatey, Scoop (main bucket), Arch (official + AUR `-bin`/`-git`), Nix, asdf plugin, mise | `docs/install.md:6-187` |
| Go | `go install github.com/rhysd/actionlint/cmd/actionlint@latest` | `docs/install.md:189-199` |
| pre-commit | `.pre-commit-hooks.yaml` has 3 hooks, all with `files: ^\.github/workflows/` and `types: [yaml]`: `actionlint` (`language: golang`, builds from the repo at `rev`, needs pre-commit ≥ 3), `actionlint-docker` (`docker_image`, `docker.io/rhysd/actionlint:<ver>`, tag rewritten by `bump-version.bash`), `actionlint-system`. Note: the golang hook doesn't install shellcheck (upstream #477). | `.pre-commit-hooks.yaml`; `docs/usage.md:358-380` |
| GitHub Actions usage | **No official action.** The docs recommend the download script plus `${{ steps.x.outputs.executable }} -color`, or `uses: docker://rhysd/actionlint:latest`, or `reviewdog/action-actionlint`. Upstream issue #117 (28 reactions) asks for a proper action. | `docs/usage.md:192-254, 310-326` |
| Problem matcher | `.github/actionlint-matcher.json`; users copy it into their own repo. | §3.3 |
| Playground | Manual `playground/deploy.bash` to gh-pages | §5 |
| Version bump | `scripts/bump-version.bash X.Y.Z` (must be on a clean `main`). It sed-replaces the version in `.pre-commit-hooks.yaml`, `docs/usage.md`, `playground/index.html`, `README.md` and the man page, then commits, tags and pushes main and the tag. `CHANGELOG.md` is regenerated from GitHub releases with `changelog-from-release`. | `scripts/bump-version.bash`; `Makefile:103-104`; `CONTRIBUTING.md:150-169` |

Other CI (`.github/workflows/`):

- `ci.yaml`: unit tests on ubuntu x64/arm, macOS arm/intel and windows x64/arm × Go 1.25/1.26 with `-race`; a CGO-less build; dogfooding with the matcher; codecov. Plus a wasm job, a lint job (gofmt, staticcheck, govulncheck, `check-checks`, shellcheck on scripts) and a docker job.
- `generate.yaml`: weekly regeneration of popular actions, webhooks and availability, which opens a PR.
- `matcher.yaml`, `download.yaml`, `codeql.yaml`.
- `dependabot.yml`: GitHub Actions only.
- `.git-hooks/pre-push` runs build, test, lint and self-lint.

---

## 8. Recommendations for workflint

### 8.1 Compatibility stance: drop-in by default, better on opt-in

- Accept the **entire actionlint flag set with Go syntax** (`-flag`, `--flag`, `-flag=value`, `-flag value`, `-f=false`, empty values), the same exit codes, `-` for stdin, `-stdin-filename`, the same default output bytes, `-oneline` and `-format` templates.
- **Config:** read `.github/workflint.y{a,}ml` first, then fall back to `.github/actionlint.y{a,}ml` with an identical schema. Allow `-init-config` to write either.
- Keep line 1 of `-version` as the bare version.
- Fix the clear bugs from §1–3 **by default**, because none of them is a behavior anyone depends on, and list each one in a "differences from actionlint" doc:
  - `paths:` matched against the repo-root-relative path
  - `-verbose -debug` → debug
  - `-color` overriding `NO_COLOR`
  - template execution errors → exit 3 on every path
  - `Project.Knows` checking a path-component boundary
  - logs that don't interleave
  - snippets for lines over 64 KiB
- **Gate genuine output changes behind a flag or new format**, for example an opt-in "precise columns" mode. Candidates:
  - consistent column unit (choose runes; SARIF says UTF-16)
  - caret alignment with tabs and multibyte text
  - `EndColumn` in the same unit as `Column`
  - allowing flags after positionals and directories as arguments
  - not recursing into `.github/workflows` subdirectories (GitHub ignores them)
- Do **not** install as `actionlint`. Name collisions on PATH or in Homebrew would be hostile. A documented `alias actionlint=workflint` is enough.

### 8.2 CLI crate

- **clap v4 (derive)** for help, completions (`clap_complete`) and the man page (`clap_mangen`, replacing ronn), behind a **small argv normalizer**. The normalizer rewrites `-name[=v]` to `--name[=v]` for known long names (clap has no single-dash long options), maps `-help` to `--help`, and accepts `--bool=false` through `value_parser!(bool)` with `num_args(0..=1)` and `default_missing_value("true")`.
- Map clap errors to **exit 2** (clap's default) and help to exit 0. Optionally print help to stderr like Go.
- Alternative: hand-roll the parser with `lexopt`. It is simpler for exact Go semantics but gives no completions or man page. clap + normalizer is the better trade-off.
- **New, purely additive flags** (each requested upstream):
  - `--summary` (#691)
  - named `-format` values (§3.5)
  - `--no-config` / `--config` merge and a global XDG config (#675, #676)
  - inline `# workflint-disable` comments (#237, 72 reactions)
  - severity/warnings (#516)
  - an LSP mode later (#229)
  - `--jobs N`
  - `--shellcheck-cache-dir`

### 8.3 Diagnostic rendering

- **Write our own renderer** for the default style (~200 LOC). Byte compatibility (§3.1 and 3.4) rules out miette, ariadne and annotate-snippets as the default.
- Use `unicode-width` (call `width_cjk()` when the locale is CJK if you want to mirror go-runewidth) and handle tabs explicitly. In precise mode, echo a `\t` in the gutter so the caret aligns.
- Use **`anstream` + `anstyle`** for colors. They are the clap stack, handle Windows consoles (replacing go-colorable) and respect `NO_COLOR`/`CLICOLOR(_FORCE)`. Reproduce fatih's exact sequences: `\x1b[33m` / `\x1b[90m` / `\x1b[1m` / `\x1b[32m`, reset `\x1b[0m`, and the reset placed after the newline on the header line.
- Optionally offer `annotate-snippets` (rustc style) as an extra named style, for example `-format pretty`.
- Use miette only for the tool's own fatal errors, if at all. Plain `eprintln!` with the upstream message texts is better for compatibility.

### 8.4 Parallelism and driver

- **rayon** `par_iter` over files, collecting `Vec<FileResult>` in input order.
  - Make `Projects` thread-safe (`DashMap` or a `Mutex<Vec>` keyed by the canonical root, with component-boundary `starts_with` on `Path`).
  - Per-project caches for local actions and reusable workflows: `DashMap<PathBuf, OnceLock<Arc<…>>>`, which also dedupes concurrent loads. Keep them alive for the `Linter`'s lifetime.
  - Static tables via `include!` or `phf` (generated at build time from the upstream generator outputs).
- **External tools:**
  - Use a bounded worker pool (default `available_parallelism()`) with `std::process::Command`. Write stdin from a separate thread, or use `wait_with_output` after spawning a writer, to avoid the #650/#702 deadlock class.
  - **Dedupe identical (shell, script) pairs within a run.**
  - Add an **opt-in persistent cache** keyed by `hash(shellcheck version, args, script)`.
  - Consider **batching** several scripts into one shellcheck invocation (temp files + `-f json1`, which includes file names) to amortize GHC startup.
  - This is where the "faster" claim can be won: it is 18 s vs under 1 s on 50 files today.
- Stream output per file in order for the default and `-oneline` formatters. Templates and SARIF still need all results.
- No async runtime is needed.
- **Logging:** use `tracing` or a mutex-guarded stderr writer, keeping the `verbose: ` and `[Linter] ` prefixes.

### 8.5 Release tooling

**Binaries:** build with `cargo-zigbuild` or `cross` (or native runners) for:

- x86_64/aarch64 apple-darwin
- x86_64/aarch64 unknown-linux-**musl** (static; one binary for every distro and Alpine Docker)
- x86_64/aarch64 pc-windows-msvc
- optional: i686/armv7 linux-musl, i686-windows, x86_64-freebsd

Use `release` profile settings `lto="fat"`, `codegen-units=1`, `strip=true`, `panic="abort"`.

**Naming:** keep goreleaser-style asset names, `workflint_<ver>_<os>_<arch>.tar.gz|zip` with darwin/linux/windows/freebsd and amd64/arm64/386/armv7. Generic installers (mise `ubi`/`github` backends, aqua, asdf plugins, `gh release download --pattern`) and the download-script logic transfer directly.

**Orchestration options:**

- **cargo-dist (`dist`)** gives you a generated release workflow, shell (`curl | sh`) and PowerShell installers, a Homebrew formula published to a tap, checksums, GitHub attestations and an MSI. Its **npm installer downloads the binary from GitHub at install time**, which breaks behind proxies and in offline mirrors and needs network on install. It also prefers target-triple asset names.
- **Custom GitHub Actions workflow:** more YAML, full control.
- **Recommendation:** use custom GitHub Actions, or cargo-dist only for the binary matrix, installers and tap. Do npm yourself with the **biome/oxlint/esbuild pattern**:
  - The main `workflint` package has `bin/workflint` (a JS shim that `require.resolve`s `@workflint/cli-<platform>-<arch>[-musl]`, `spawnSync`s with `stdio: "inherit"` and forwards the exit code and signals). It also sets `WORKFLINT_INSTALLED_FROM=npm`.
  - `optionalDependencies` pins every platform package at the exact version.
  - Each platform package carries `os`, `cpu` and (for linux) `libc` fields plus the binary.
  - **No postinstall scripts**: pnpm and yarn and many CI setups disable them.
  - Publish with npm **trusted publishing (OIDC) + `--provenance`**.
  - Optionally publish `@workflint/wasm`.

**Other channels:**

- **crates.io:**
  - Reserve `workflint` (bin + lib) and possibly `workflint-core`.
  - Use `include`/`exclude` to keep testdata and the corpus out (10 MB crate limit).
  - Add `[package.metadata.binstall]` so `cargo binstall workflint` pulls release assets.
  - `cargo install` builds from source, so all data must be checked in or generated in `build.rs` without network access.
- **Homebrew:**
  - Start with your own **tap** (`<org>/homebrew-tap`, `Formula/workflint.rb`) that installs the release binary. It can be generated by cargo-dist or a small script that updates `url`/`sha256` per arch.
  - Prefer a **formula** over a cask. Formula installs aren't quarantined (cask installs hit the Gatekeeper warning in `docs/install.md:81-83`), the formula can `depends_on "shellcheck"` (recommended), and it installs completions and the man page.
  - Submit to homebrew-core once you meet its notability bar (built from source with `depends_on "rust" => :build`).
- **`install.sh`** (and `install.ps1`):
  - Be compatible with the upstream calling convention `bash <(curl -fsSL …/install.sh) [VERSION|latest] [DIR]`, and also accept `--version`/`--dir` flags.
  - Detect OS, arch and musl.
  - **Verify the sha256 against `checksums.txt`.** Optionally run `gh attestation verify` when `gh` is present.
  - Write `executable=` to `$GITHUB_OUTPUT`.
  - Fall back to the GitHub API to resolve `latest` instead of hardcoding the version (or keep the hardcoded default, auto-bumped).
  - Test it on 3 OSes in CI, as upstream does.
- **GitHub Action:**
  - Ship an official composite action (`<org>/workflint@v1` via `action.yml` in the repo root, or a separate `workflint-action` repo).
  - It downloads the verified binary with tool-cache caching, registers a bundled problem matcher (owner `workflint`, same regex shape), runs with `args`, and optionally writes SARIF and uploads it to code scanning.
  - This addresses upstream #117.
- **pre-commit** (`.pre-commit-hooks.yaml`):
  - `workflint-system` (`language: system`)
  - `workflint-docker` (`language: docker_image`, pinned tag bumped at release)
  - `workflint` via **`language: rust`**: pre-commit runs `cargo install` from the repo, which works but is slow and needs a toolchain.
  - Or, faster: a **PyPI wheel built with maturin `bindings = "bin"`** plus a separate mirror repo `workflint-pre-commit` using `language: python` that depends on `workflint==<ver>` (the ruff/`ruff-pre-commit` pattern). That also gives `pipx`/`uv tool install workflint`.
  - Consider a `language: node` hook as well, through the npm package.
- **Docker:**
  - Do not compile inside Docker. `COPY` the prebuilt musl binaries per `TARGETARCH`.
  - Base: `alpine` + shellcheck (static binary from `koalaman/shellcheck-alpine`) + `py3-pyflakes`, non-root user.
  - Also offer a `-slim` variant `FROM scratch` / distroless static with just workflint.
  - Publish to **ghcr.io** (plus Docker Hub if wanted) for amd64 and arm64, with provenance and SBOM attestations from buildx.
- **Windows, Linux and version-manager channels:**
  - winget via `winget-releaser` as upstream does.
  - Scoop: own bucket or a main-bucket PR with `checkver`/`autoupdate` on GitHub releases.
  - Chocolatey: optional.
  - AUR `workflint-bin`.
  - nixpkgs (`buildRustPackage`).
  - mise/aqua registry entries.

**Version embedding:**

- Version: `env!("CARGO_PKG_VERSION")`.
- Git SHA and date: `build.rs` via `vergen-gitcl` or `built`, falling back cleanly when building from a crates.io tarball.
- `installedFrom`: `option_env!("WORKFLINT_INSTALLED_FROM")` at build time (set by the release CI), overridable at **runtime** by the same env var. The npm shim, Docker image and Homebrew formula wrapper can set it, so a single binary serves every channel.
- Keep the 3-line `-version` format, for example `0.3.0` / `installed by downloading from release page` / `built with rustc 1.xx for darwin/arm64`. Map the triple to Go-style os/arch for familiarity.
- Release automation: `release-plz` or a `bump-version` script that also updates the pre-commit Docker tag, docs snippets, the install-script default and the playground footer (mirroring `scripts/bump-version.bash`).

### 8.6 Test, tooling and CI checklist for the port

- [ ] Vendored upstream corpus plus a harness reproducing `checkErrors` semantics (§6.3).
- [ ] Snapshot tests from `docs/checks.md` and the matcher fixtures.
- [ ] Differential runner against the pinned Go binary over the corpus and the 1503 real-world files.
- [ ] Pinned shellcheck in CI (and pyflakes via `pipx`).
- [ ] OS matrix: linux x64/arm64, macOS arm64/x64, windows x64/arm64.
- [ ] Windows path-separator normalization in tests, as upstream does with `ToSlash`.
- [ ] Problem-matcher regex test, ported from `test.mjs` to a Rust test.
- [ ] WASM job (build plus a Node test of the exported `lint`).
- [ ] Docker job (build, run on the repo, hadolint).
- [ ] Download/install script job.
- [ ] Self-lint (dogfood) of `.github/workflows` with the matcher.
- [ ] `cargo deny` / `cargo audit` (replacing govulncheck), clippy, rustfmt.
- [ ] `cargo-fuzz` targets.
- [ ] `divan` benchmarks plus a hyperfine regression job.
- [ ] Port of the weekly data regeneration (popular actions, webhooks, availability). Generator outputs can be JSON consumed by `build.rs`.
