//! The help text of `yaml-syntax-error` diagnostics, which names the likely
//! cause of a cryptic libyaml error.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::parse;

/// The message and help text of the one syntax error in `text`.
fn error(text: &str) -> (String, Option<String>) {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let parsed = parse(&file);
    assert!(parsed.document.is_none(), "{text:?} parsed");
    let [diagnostic] = parsed.diagnostics.as_slice() else {
        panic!("{text:?} gave {:?}", parsed.diagnostics);
    };
    assert_eq!(diagnostic.rule.id, "yaml-syntax-error");
    (
        diagnostic.message.clone(),
        diagnostic.help.as_deref().map(str::to_owned),
    )
}

/// The help text of the one syntax error in `text`, which must be
/// `did not find expected key`.
fn expected_key_help(text: &str) -> Option<String> {
    let (message, help) = error(text);
    assert_eq!(
        message, "could not parse as YAML: did not find expected key",
        "{text:?}"
    );
    help
}

#[test]
fn list_item_indented_less_than_the_item_before_it_names_both_indentations() {
    let text = "\
jobs:
  test:
    steps:
      - run: a
     - run: b
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "this list item is indented 5 spaces and the item on line 4 is indented 6, \
             but the items of one list need the same indentation"
        ),
    );
}

#[test]
fn list_item_indented_more_than_the_item_before_it_names_both_indentations() {
    let text = "\
steps:
- run: a
  - run: b
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "this list item is indented 2 spaces and the item on line 2 is indented 0, \
             but the items of one list need the same indentation"
        ),
    );
}

#[test]
fn list_item_compares_with_the_list_and_not_with_a_nested_one() {
    let text = "\
jobs:
  test:
    steps:
      - uses: actions/checkout@v4
        with:
          sparse-checkout: |
            - src
      - run: a
     - run: b
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "this list item is indented 5 spaces and the item on line 8 is indented 6, \
             but the items of one list need the same indentation"
        ),
    );
}

#[test]
fn list_item_among_mapping_keys_names_the_mapping() {
    let text = "\
jobs:
  test:
    runs-on: ubuntu-latest
    - run: a
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "the mapping that starts on line 3 expects a key here, not a list item, \
             so move the item under the key whose value is the list"
        ),
    );
}

#[test]
fn key_indented_deeper_than_the_keys_before_it_names_both_indentations() {
    let text = "\
jobs:
  test:
    runs-on: ubuntu-latest
   steps: []
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "this key is indented 3 spaces, but the keys of the mapping that starts on line 2 \
             are indented 2, so align it with the keys of the mapping it belongs to"
        ),
    );
}

#[test]
fn key_indented_under_a_quoted_value_names_both_indentations() {
    let text = "\
name: \"CI\"
  on: push
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "this key is indented 2 spaces, but the keys of the mapping that starts on line 1 \
             are indented 0, so align it with the keys of the mapping it belongs to"
        ),
    );
}

#[test]
fn text_after_a_flow_sequence_says_to_quote_the_value() {
    assert_eq!(
        expected_key_help("name: [Foo] bar\n").as_deref(),
        Some(
            "a flow sequence ends at its \"]\" and YAML allows no more text after it \
             on the line, so quote the whole value if it is a string"
        ),
    );
}

#[test]
fn text_after_a_flow_mapping_says_to_quote_the_value() {
    assert_eq!(
        expected_key_help("name: {a: b} c\n").as_deref(),
        Some(
            "a flow mapping ends at its \"}\" and YAML allows no more text after it \
             on the line, so quote the whole value if it is a string"
        ),
    );
}

#[test]
fn text_after_a_quoted_string_says_to_put_it_inside_the_quotes() {
    for text in ["name: \"Foo\" bar\n", "name: 'Foo' bar\n"] {
        assert_eq!(
            expected_key_help(text).as_deref(),
            Some(
                "a quoted string ends at its closing quote and YAML allows no more text \
                 after it on the line, so put the whole value inside the quotes"
            ),
            "{text:?}",
        );
    }
}

#[test]
fn key_without_a_space_after_its_colon_suggests_the_space() {
    let text = "\
name: \"CI\"
  runs-on:ubuntu-latest
";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "YAML reads \"runs-on:ubuntu-latest\" as one string, because a \":\" after a key \
             needs a space after it; did you mean \"runs-on: ubuntu-latest\"?"
        ),
    );
}

#[test]
fn key_without_a_space_after_its_colon_on_the_last_line_suggests_the_space() {
    let text = "\
jobs:
  test:
    steps: []
    runs-on:ubuntu-latest
";
    let (message, help) = error(text);
    assert_eq!(
        message,
        "could not parse as YAML: could not find expected ':'"
    );
    assert_eq!(
        help.as_deref(),
        Some(
            "YAML reads \"runs-on:ubuntu-latest\" as one string, because a \":\" after a key \
             needs a space after it; did you mean \"runs-on: ubuntu-latest\"?"
        ),
    );
}

#[test]
fn url_is_not_a_key_without_a_space() {
    let text = "\
name: \"CI\"
  https://example.com
";
    assert_eq!(expected_key_help(text), None);
}

#[test]
fn unrelated_expected_key_error_has_no_help() {
    assert_eq!(expected_key_help("on: push\n|\n  text\n"), None);
}

#[test]
fn unrelated_simple_key_error_has_no_help() {
    let (message, help) = error("on: push\n\"jobs\"\n");
    assert_eq!(
        message,
        "could not parse as YAML: could not find expected ':'"
    );
    assert_eq!(help, None);
}

#[test]
fn other_syntax_errors_have_no_help() {
    let (message, help) = error("on: push\n  jobs: {}\n");
    assert_eq!(
        message,
        "could not parse as YAML: mapping values are not allowed in this context"
    );
    assert_eq!(help, None);
}

#[test]
fn crlf_line_breaks_count_lines_like_lf() {
    let text = "steps:\r\n- run: a\r\n  - run: b\r\n";
    assert_eq!(
        expected_key_help(text).as_deref(),
        Some(
            "this list item is indented 2 spaces and the item on line 2 is indented 0, \
             but the items of one list need the same indentation"
        ),
    );
}
