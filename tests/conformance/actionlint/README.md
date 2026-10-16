# Vendored actionlint test data

This directory is a copy of files from [rhysd/actionlint](https://github.com/rhysd/actionlint) at commit `011a6d15e749bb3f2d771eed9c7aa0e7e3e10ee7`, the reference that workflint is compatible with (ADR 0004). Each file keeps its path relative to the root of the actionlint repository:

- `testdata/`, including the hidden `.github` directories and the corpus archive `testdata/realworld/dataset.zip`.
- `docs/checks.md`.
- `scripts/generate-actionlint-matcher/test/`.
- `.github/actionlint-matcher.json`.
- `expr_sema.go`, the expression checker, whose `BuiltinGlobalVariableTypes` and `BuiltinFuncSignatures` tables the tests of `workflint-data` compare with.
- `availability.go`, the contexts and special functions that each workflow key allows, which the tests of `workflint-data` compare with.
- `expr_insecure.go`, the untrusted-input checker, whose `BuiltinUntrustedInputs` table the tests of `workflint-data` compare with.
- `LICENSE.txt`, actionlint's MIT license, which covers every file here except this README.

Do not edit these files. To recreate the copy, run this from the repository root:

```sh
cargo xtask vendor-testdata
```

The command clones `.reference/actionlint` at the commit if it is missing and copies the files that git tracks there. It removes every other file in this directory except this README. A second run changes nothing.

The corpus stays a zip archive. `cargo xtask reference` unpacks its 1,503 workflows into `.reference/realworld/`.

`.gitattributes` turns off line ending conversion for this directory, as upstream does for its `testdata/`, so every checkout has upstream's bytes.
