#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

use super::Glob;
use crate::{Context, Rule, visit};

/// The note that ends every message, with the space before it.
const NOTE: &str = " note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet";

/// Lints `text` with only the `glob` rule and returns its diagnostics in the one-line format,
/// without the note that ends every message.
fn lint(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Glob)];
    let mut cx = Context::new(&file);
    visit(&parsed.workflow, &mut rules, &mut cx);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &cx.into_diagnostics(), Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| line.replace(NOTE, ""))
        .collect()
}

/// Returns a workflow whose `push` event has `filter` with the YAML scalar `pattern` as its only
/// item, on line 4.
fn push(filter: &str, pattern: &str) -> String {
    format!(
        "on:
  push:
    {filter}:
      - {pattern}
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
"
    )
}

#[test]
fn every_message_ends_with_the_cheat_sheet_note() {
    let file = SourceFile::new(FileId::new(0), "test.yaml", push("branches", "a b")).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Glob)];
    let mut cx = Context::new(&file);
    visit(&parsed.workflow, &mut rules, &mut cx);
    let diagnostics = cx.into_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0].message.ends_with(&format!(
            "note that regular expression is unavailable.{NOTE}"
        )),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn branch_and_tag_filters_are_checked_as_ref_names() {
    for filter in ["branches", "branches-ignore", "tags", "tags-ignore"] {
        assert_eq!(
            lint(&push(filter, "'^foo'")),
            [
                "test.yaml:4:10: character '^' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable. [invalid-ref-name]"
            ],
            "{filter}"
        );
    }
}

#[test]
fn path_filters_are_checked_as_paths() {
    for filter in ["paths", "paths-ignore"] {
        assert_eq!(lint(&push(filter, "'^foo'")), Vec::<String>::new());
        assert_eq!(
            lint(&push(filter, "./foo")),
            ["test.yaml:4:9: '.' and '..' are not allowed in glob path. [glob-path-dot]"],
            "{filter}"
        );
        assert_eq!(
            lint(&push(filter, "'foo '")),
            [
                "test.yaml:4:10: leading and trailing spaces are not allowed in glob path. [glob-path-whitespace]"
            ],
            "{filter}"
        );
    }
}

#[test]
fn syntax_errors_are_reported_as_invalid_glob() {
    assert_eq!(
        lint(&push("tags", "'v*+'")),
        [
            "test.yaml:4:12: invalid glob pattern. unexpected character '+' while checking special character + (one or more). the preceding character must not be special character. [invalid-glob]"
        ]
    );
}

#[test]
fn image_version_versions_and_snapshot_version_are_checked_as_ref_names() {
    let text = "on:
  image_version:
    names: [image]
    versions:
      - 'v[0-'
jobs:
  test:
    runs-on: ubuntu-latest
    snapshot:
      image-name: image
      version: 'a b'
    steps:
      - run: echo
";
    assert_eq!(
        lint(text),
        [
            "test.yaml:5:13: invalid glob pattern. unexpected EOF while checking end of character match []. missing ]. [invalid-glob]",
            "test.yaml:11:18: character ' ' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable. [invalid-ref-name]",
        ]
    );
}

#[test]
fn other_events_and_filters_are_not_checked() {
    let text = "on:
  pull_request:
    types: ['a b']
  workflow_run:
    workflows: ['a b']
  repository_dispatch:
    types: ['a b']
jobs:
  test:
    runs-on: ubuntu-latest
    snapshot: 'a b'
    steps:
      - run: echo
";
    assert_eq!(lint(text), Vec::<String>::new());
}

#[test]
fn empty_patterns_are_skipped() {
    assert_eq!(lint(&push("branches", "''")), Vec::<String>::new());
    assert_eq!(lint(&push("paths", "''")), Vec::<String>::new());
}

#[test]
fn patterns_with_expressions_are_checked() {
    assert_eq!(
        lint(&push("tags", "${{ x }}")),
        [
            "test.yaml:4:12: character ' ' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable. [invalid-ref-name]",
            "test.yaml:4:14: character ' ' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable. [invalid-ref-name]"
        ]
    );
}

#[test]
fn a_problem_after_an_escape_in_a_quoted_pattern_is_at_its_character() {
    // `\t` is two characters in the source and one in the value, so the `]` is at column 20.
    // The reference adds the offset in the value to the scalar's column and reports column 19.
    assert_eq!(
        lint(&push("paths", r#""foo\tbar[x]""#)),
        [
            "test.yaml:4:20: invalid glob pattern. unexpected character ']' while checking character match []. character match with single character is useless. simply use x instead of [x]. [invalid-glob]"
        ]
    );
}

#[test]
fn an_escaped_special_character_in_a_ref_pattern_is_reported_at_that_character() {
    assert_eq!(
        lint(&push("branches", r"'a\[x'")),
        [
            "test.yaml:4:12: character '[' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable. [invalid-ref-name]"
        ]
    );
}

#[test]
fn a_problem_with_the_whole_pattern_is_at_the_first_character_of_its_value() {
    let block = "on:
  push:
    paths:
      - |
        foo.txt
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(
        lint(block),
        [
            "test.yaml:5:9: leading and trailing spaces are not allowed in glob path. [glob-path-whitespace]"
        ]
    );
    assert_eq!(
        lint(&push("paths", "' ./foo'")),
        [
            "test.yaml:4:10: leading and trailing spaces are not allowed in glob path. [glob-path-whitespace]",
            "test.yaml:4:10: '.' and '..' are not allowed in glob path. [glob-path-dot]",
        ]
    );
}
