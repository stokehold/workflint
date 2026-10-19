//! The parser labels a diagnostic inside an anchored subtree with the aliases through which it
//! reached the subtree.

#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{FileId, SourceFile, sort_and_dedup};

/// Parses `text` and returns each sorted and deduplicated diagnostic of the parser as its
/// position, its rule ID, and the position and note of each label.
fn labelled(text: &str) -> Vec<(String, &'static str, Vec<String>)> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let mut diagnostics = workflint_syntax::parse(&file, &document).diagnostics;
    sort_and_dedup(&file, &mut diagnostics);
    let at = |offset: u32| {
        let pos = file.line_col(offset);
        format!("{}:{}", pos.line, pos.column)
    };
    diagnostics
        .iter()
        .map(|d| {
            let labels = d
                .labels
                .iter()
                .map(|label| format!("{} {}", at(label.span.start), label.note))
                .collect();
            (at(d.span.start), d.rule.id, labels)
        })
        .collect()
}

// The step of the reference's `err/errors_in_anchor` fixture.
#[test]
fn an_error_in_a_step_used_through_two_aliases_has_a_label_at_each() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - &invalid_step
        run: echo
        with:
          neovim: true
      - *invalid_step
      - *invalid_step
";
    assert_eq!(
        labelled(text),
        [(
            "8:9".to_owned(),
            "unexpected-key",
            vec![
                "10:9 reached through alias *invalid_step".to_owned(),
                "11:9 reached through alias *invalid_step".to_owned(),
            ],
        )]
    );
}

#[test]
fn an_error_at_a_key_has_no_label_for_the_alias_of_its_value() {
    let text = "\
on: push
env: &env
  A: x
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        env: *env
        env: *env
";
    assert_eq!(
        labelled(text),
        [("10:9".to_owned(), "duplicate-key", Vec::<String>::new())]
    );
}

#[test]
fn an_error_reached_through_nested_aliases_has_a_label_at_each_alias() {
    let text = "\
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - &inner
        run: echo
        shell: ''
  b:
    runs-on: ubuntu-latest
    steps: &steps
      - *inner
  c:
    runs-on: ubuntu-latest
    steps: *steps
";
    assert_eq!(
        labelled(text),
        [(
            "8:16".to_owned(),
            "empty-string",
            vec![
                "12:9 reached through alias *inner".to_owned(),
                "15:12 reached through alias *steps".to_owned(),
            ],
        )]
    );
}
