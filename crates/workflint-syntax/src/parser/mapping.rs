//! The mapping walker, the reference's `parseMapping`, and the key tables
//! that describe the keys of a section.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use workflint_diagnostics::Span;
use workflint_diagnostics::quote::{quote, sorted_quotes};
use workflint_yaml::{Node, NodeKind, Tag, simple_lowercase};

use super::{Parser, kind_name};
use crate::Str;
use crate::rules::{
    DUPLICATE_KEY, EMPTY_SECTION, MERGE_KEY_UNSUPPORTED, UNEXPECTED_KEY, UNEXPECTED_NODE_KIND,
};

/// What messages call a mapping, the reference's `where` argument of
/// `parseMapping`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Label<'s> {
    /// A section, printed as `"name" section`. The reference's
    /// `parseSectionMapping`.
    Section(&'s str),
    /// A place printed as written, such as `element of "schedule" section`.
    /// The reference's `parseMappingAt`.
    At(&'s str),
}

impl fmt::Display for Label<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Section(name) => write!(f, "{} section", quote(name)),
            Self::At(place) => f.write_str(place),
        }
    }
}

/// The key of a mapping entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Key<'a> {
    /// The key, or `None` if the key node is not a scalar.
    pub(crate) string: Option<Str<'a>>,
    /// The span of the key node.
    pub(crate) span: Span,
}

impl<'a> Key<'a> {
    /// Returns the key's value. A key that is not a scalar has the value
    /// `""`, as in the reference, which then reports it as an unexpected
    /// key `""` in most sections.
    pub(crate) fn value(&self) -> &'a str {
        self.string.as_ref().map_or("", Str::value)
    }
}

/// An entry that the mapping walker yields.
#[derive(Debug, Clone)]
pub(crate) struct MapEntry<'a> {
    /// The key as written in a case-sensitive section, or folded with
    /// [`simple_lowercase`] in a case-insensitive one. The reference's
    /// `workflowMappingEntry.id`.
    pub(crate) id: Cow<'a, str>,
    /// The key.
    pub(crate) key: Key<'a>,
    /// The value node.
    pub(crate) value: &'a Node,
}

/// A section whose keys a table of [`KeySpec`] values describes.
pub(crate) trait Section: Sized + 'static {
    /// The AST node that the section's keys fill.
    type Ast<'a>;
}

/// One key of a section: its name and the function that parses its value
/// into the section's AST node.
pub(crate) struct KeySpec<S: Section> {
    /// The key.
    pub(crate) name: &'static str,
    /// Parses the value of the entry into the AST node.
    pub(crate) parse: for<'a, 'f> fn(&mut Parser<'a, 'f>, &mut S::Ast<'a>, &MapEntry<'a>),
}

impl<'a> Parser<'a, '_> {
    /// Walks the mapping `node` and returns its entries, as the reference's
    /// `parseMapping` yields them.
    ///
    /// The checks come in the reference's order:
    ///
    /// 1. A `!!null` scalar is an empty section, reported unless
    ///    `allow_empty`.
    /// 2. Any other node that is not a mapping is reported as the wrong
    ///    kind.
    /// 3. A key that is not a scalar, or is empty, is reported, and the
    ///    entry is kept with the key `""`.
    /// 4. A `<<` key, quoted or not, is reported as an unsupported merge key
    ///    and skipped.
    /// 5. A key that repeats an earlier one is reported and skipped, so its
    ///    value is never parsed. Keys of a section that is not
    ///    `case_sensitive` are compared after [`simple_lowercase`].
    /// 6. A mapping with no entry left is an empty section, reported unless
    ///    `allow_empty`.
    pub(crate) fn parse_mapping(
        &mut self,
        label: Label<'_>,
        node: &'a Node,
        allow_empty: bool,
        case_sensitive: bool,
    ) -> Vec<MapEntry<'a>> {
        let mapping = match &node.kind {
            NodeKind::Mapping(mapping) => mapping,
            NodeKind::Scalar(scalar) if scalar.tag == Tag::Null => {
                if !allow_empty {
                    self.empty_mapping(label, node);
                }
                return Vec::new();
            }
            NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => {
                self.error(
                    &UNEXPECTED_NODE_KIND,
                    node.span,
                    format!(
                        "{label} is {} node but mapping node is expected",
                        kind_name(node)
                    ),
                );
                return Vec::new();
            }
        };
        let document = self.document;
        // The span of the first key with each ID.
        let mut first: BTreeMap<Cow<'a, str>, Span> = BTreeMap::new();
        let mut entries = Vec::new();
        for entry in document.entries(mapping) {
            let Some(key_node) = self.node(entry.key) else {
                continue;
            };
            let key = Key {
                string: Str::new(key_node),
                span: key_node.span,
            };
            self.check_string(key_node, false);
            if key.value() == "<<" {
                self.error(
                    &MERGE_KEY_UNSUPPORTED,
                    key.span,
                    "GitHub Actions does not support YAML merge key \"<<\"".to_owned(),
                );
                continue;
            }
            let id = if case_sensitive {
                Cow::Borrowed(key.value())
            } else {
                simple_lowercase(key.value())
            };
            if let Some(&previous) = first.get(&id) {
                let suffix = if case_sensitive {
                    ""
                } else {
                    ". note that this key is case insensitive"
                };
                let message = format!(
                    "key {} is duplicated in {label}. previously defined at {}{suffix}",
                    quote(key.value()),
                    self.position(previous),
                );
                self.error(&DUPLICATE_KEY, key.span, message);
                continue;
            }
            let Some(value) = self.node(entry.value) else {
                continue;
            };
            first.insert(id.clone(), key.span);
            entries.push(MapEntry { id, key, value });
        }
        if !allow_empty && entries.is_empty() {
            self.empty_mapping(label, node);
        }
        entries
    }

    fn empty_mapping(&mut self, label: Label<'_>, node: &Node) {
        self.error(
            &EMPTY_SECTION,
            node.span,
            format!("{label} should not be empty. please remove this section if it's unnecessary"),
        );
    }

    /// Parses each entry with the function that `keys` gives for its ID,
    /// and reports any other entry as an unexpected key of `section`.
    pub(crate) fn parse_keys<S: Section>(
        &mut self,
        entries: Vec<MapEntry<'a>>,
        section: &str,
        keys: &[KeySpec<S>],
        ast: &mut S::Ast<'a>,
    ) {
        for entry in entries {
            match keys.iter().find(|spec| spec.name == entry.id) {
                Some(spec) => (spec.parse)(self, ast, &entry),
                None => self.unexpected_key(entry.key, section, keys.iter().map(|spec| spec.name)),
            }
        }
    }

    /// Reports `key` as a key that `section` does not define, as the
    /// reference's `unexpectedKey` does.
    ///
    /// A section name without a space is printed as `"name" section`. With
    /// one valid key the message names it, and with more it lists them
    /// sorted.
    pub(crate) fn unexpected_key<'k>(
        &mut self,
        key: Key<'_>,
        section: &str,
        expected: impl IntoIterator<Item = &'k str>,
    ) {
        let section = if section.contains(' ') {
            Cow::Borrowed(section)
        } else {
            Cow::Owned(format!("{} section", quote(section)))
        };
        let expected: Vec<&str> = expected.into_iter().collect();
        let got = quote(key.value());
        let message = match expected[..] {
            [] => format!("unexpected key {got} for {section}"),
            [only] => format!("expected {} key for {section} but got {got}", quote(only)),
            _ => format!(
                "unexpected key {got} for {section}. expected one of {}",
                sorted_quotes(expected)
            ),
        };
        self.error(&UNEXPECTED_KEY, key.span, message);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::testing::parse_with;

    /// Walks the root of `text` as the section `"test"` and returns the
    /// diagnostics followed by one `id=key` line per entry.
    fn walk(text: &str, allow_empty: bool, case_sensitive: bool) -> Vec<String> {
        parse_with(text, |parser, root| {
            parser
                .parse_mapping(Label::Section("test"), root, allow_empty, case_sensitive)
                .iter()
                .map(|entry| format!("{}={}", entry.id, entry.key.value()))
                .collect()
        })
    }

    #[test]
    fn a_null_value_is_an_empty_section_unless_empty_is_allowed() {
        for text in ["~\n", "null\n", "!!null x\n"] {
            assert_eq!(
                walk(text, false, true),
                [
                    r#"1:1: "test" section should not be empty. please remove this section if it's unnecessary [empty-section]"#
                ],
                "{text:?}"
            );
            assert!(walk(text, true, true).is_empty(), "{text:?}");
        }
    }

    #[test]
    fn a_node_that_is_not_a_mapping_has_the_wrong_kind() {
        assert_eq!(
            walk("[a]\n", true, true),
            [
                r#"1:1: "test" section is sequence node but mapping node is expected [unexpected-node-kind]"#
            ]
        );
        assert_eq!(
            walk("'~'\n", true, true),
            [
                r#"1:1: "test" section is scalar node but mapping node is expected [unexpected-node-kind]"#
            ]
        );
        assert_eq!(
            walk("&a [*a]\n", true, true).last().unwrap(),
            "1:1: \"test\" section is sequence node but mapping node is expected [unexpected-node-kind]"
        );
    }

    #[test]
    fn an_alias_value_that_stays_unresolved_has_the_alias_kind() {
        let found = parse_with("a: &a\n  b: *a\n", |parser, root| {
            let entries = parser.parse_mapping(Label::Section("test"), root, true, true);
            let inner = parser.parse_mapping(Label::Section("a"), entries[0].value, true, true);
            parser.parse_mapping(Label::Section("b"), inner[0].value, true, true);
            Vec::new()
        });
        assert_eq!(
            found,
            [
                r#"2:6: "b" section is alias node but mapping node is expected [unexpected-node-kind]"#
            ]
        );
    }

    #[test]
    fn a_key_that_is_not_a_scalar_is_reported_and_kept_as_an_empty_key() {
        assert_eq!(
            walk("[a]: 1\n? {b: c}\n: 2\nd: 3\n", true, true),
            [
                r#"1:1: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                r#"2:3: key "" is duplicated in "test" section. previously defined at line:1,col:1 [duplicate-key]"#,
                r#"2:3: expected scalar node for string value but found mapping node with "!!map" tag [unexpected-node-kind]"#,
                "=",
                "d=d",
            ]
        );
    }

    #[test]
    fn an_empty_key_is_reported_and_kept() {
        assert_eq!(
            walk("'': 1\n", true, true),
            ["1:1: string should not be empty [empty-string]", "="]
        );
    }

    #[test]
    fn a_merge_key_is_reported_and_skipped_quoted_or_not() {
        assert_eq!(
            walk("a: &a {x: 1}\n<<: *a\n'<<': *a\n\"<<\": 1\n", true, true),
            [
                r#"2:1: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
                r#"3:1: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
                r#"4:1: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
                "a=a",
            ]
        );
    }

    #[test]
    fn a_mapping_with_only_merge_keys_is_empty() {
        assert_eq!(
            walk("<<: {}\n", false, true),
            [
                r#"1:1: "test" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                r#"1:1: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
            ]
        );
    }

    #[test]
    fn a_duplicate_key_is_reported_and_its_value_is_not_parsed() {
        let found = parse_with("a: 1\nb: 2\na: [x]\n", |parser, root| {
            let entries = parser.parse_mapping(Label::Section("test"), root, true, true);
            entries
                .iter()
                .map(|entry| format!("{}={}", entry.id, Str::new(entry.value).unwrap().value()))
                .collect()
        });
        assert_eq!(
            found,
            [
                r#"3:1: key "a" is duplicated in "test" section. previously defined at line:1,col:1 [duplicate-key]"#,
                "a=1",
                "b=2",
            ]
        );
    }

    #[test]
    fn a_case_sensitive_section_keeps_keys_that_differ_in_case() {
        assert_eq!(walk("Foo: 1\nfoo: 2\n", true, true), ["Foo=Foo", "foo=foo"]);
    }

    #[test]
    fn a_case_insensitive_section_folds_keys_and_notes_it_on_duplicates() {
        assert_eq!(
            walk("Foo: 1\nFOO: 2\n\u{130}D: 3\nid: 4\n", true, false),
            [
                r#"2:1: key "FOO" is duplicated in "test" section. previously defined at line:1,col:1. note that this key is case insensitive [duplicate-key]"#,
                r#"4:1: key "id" is duplicated in "test" section. previously defined at line:3,col:1. note that this key is case insensitive [duplicate-key]"#,
                "foo=Foo",
                "id=\u{130}D",
            ]
        );
    }

    #[test]
    fn the_position_of_the_first_key_counts_columns_in_characters() {
        let found = parse_with("x: {日本: 1, 日本: 2}\n", |parser, root| {
            let entries = parser.parse_mapping(Label::Section("test"), root, true, true);
            parser.parse_mapping(Label::Section("x"), entries[0].value, true, true);
            Vec::new()
        });
        assert_eq!(
            found,
            [
                r#"1:12: key "日本" is duplicated in "x" section. previously defined at line:1,col:5 [duplicate-key]"#
            ]
        );
    }

    #[test]
    fn an_empty_mapping_is_reported_unless_empty_is_allowed() {
        assert_eq!(
            walk("{}\n", false, true),
            [
                r#"1:1: "test" section should not be empty. please remove this section if it's unnecessary [empty-section]"#
            ]
        );
        assert_eq!(walk("{}\n", true, true), Vec::<String>::new());
    }

    fn unexpected(expected: &[&str], section: &str) -> Vec<String> {
        parse_with("x: 1\n", |parser, root| {
            let entries = parser.parse_mapping(Label::Section("test"), root, true, true);
            parser.unexpected_key(entries[0].key, section, expected.iter().copied());
            Vec::new()
        })
    }

    #[test]
    fn an_unexpected_key_lists_the_valid_keys_sorted() {
        assert_eq!(
            unexpected(&["b", "c", "a"], "workflow"),
            [
                r#"1:1: unexpected key "x" for "workflow" section. expected one of "a", "b", "c" [unexpected-key]"#
            ]
        );
    }

    #[test]
    fn an_unexpected_key_names_the_only_valid_key() {
        assert_eq!(
            unexpected(&["run"], "defaults"),
            [r#"1:1: expected "run" key for "defaults" section but got "x" [unexpected-key]"#]
        );
    }

    #[test]
    fn an_unexpected_key_with_no_valid_keys_has_no_list() {
        assert_eq!(
            unexpected(&[], "test"),
            [r#"1:1: unexpected key "x" for "test" section [unexpected-key]"#]
        );
    }

    #[test]
    fn a_section_name_with_a_space_is_printed_as_written() {
        assert_eq!(
            unexpected(&["a", "b"], "element of \"steps\" section"),
            [
                r#"1:1: unexpected key "x" for element of "steps" section. expected one of "a", "b" [unexpected-key]"#
            ]
        );
    }
}
