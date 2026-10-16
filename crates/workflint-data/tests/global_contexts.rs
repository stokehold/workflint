//! Checks the global context types against the reference's `BuiltinGlobalVariableTypes` in
//! the vendored `expr_sema.go`, and against the rules of
//! `TestBuiltinGlobalVariableTypesValidation` in `expr_sema_test.go`.

#![allow(clippy::panic)]

use std::collections::BTreeMap;

use workflint_data::{GLOBAL_CONTEXTS, ObjectDesc, Rest, TypeDesc, global_context};

/// The reference's `expr_sema.go`, vendored by `cargo xtask vendor-testdata`.
const EXPR_SEMA_GO: &str = include_str!("../../../tests/conformance/actionlint/expr_sema.go");

/// An owned expression type, built from the Go source or from a [`TypeDesc`].
#[derive(Debug, PartialEq, Eq)]
enum Ty {
    Any,
    Null,
    Number,
    Bool,
    String,
    Array(Box<Ty>),
    Object(BTreeMap<String, Ty>, OwnedRest),
}

#[derive(Debug, PartialEq, Eq)]
enum OwnedRest {
    Strict,
    Loose,
    Map(Box<Ty>),
}

impl From<&TypeDesc> for Ty {
    fn from(ty: &TypeDesc) -> Self {
        match ty {
            TypeDesc::Any => Ty::Any,
            TypeDesc::Null => Ty::Null,
            TypeDesc::Number => Ty::Number,
            TypeDesc::Bool => Ty::Bool,
            TypeDesc::String => Ty::String,
            TypeDesc::Array(elem) => Ty::Array(Box::new(Ty::from(*elem))),
            TypeDesc::Object(object) => {
                let props = object
                    .props
                    .iter()
                    .map(|prop| (prop.name.to_owned(), Ty::from(&prop.ty)))
                    .collect();
                let rest = match object.rest {
                    Rest::Strict => OwnedRest::Strict,
                    Rest::Loose => OwnedRest::Loose,
                    Rest::Map(elem) => OwnedRest::Map(Box::new(Ty::from(elem))),
                };
                Ty::Object(props, rest)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    Ident(&'a str),
    Str(&'a str),
    Punct(char),
}

/// Splits Go source into identifiers, interpreted string literals without escapes, and
/// punctuation, and drops whitespace and `//` comments. This is enough for a map literal.
fn tokenize(src: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut rest = src;
    while let Some(c) = rest.chars().next() {
        if c.is_whitespace() {
            rest = &rest[c.len_utf8()..];
        } else if let Some(comment) = rest.strip_prefix("//") {
            rest = comment.find('\n').map_or("", |end| &comment[end..]);
        } else if c == '"' {
            let Some(end) = rest[1..].find('"') else {
                panic!("unterminated string in {rest:.40}");
            };
            tokens.push(Token::Str(&rest[1..=end]));
            rest = &rest[end + 2..];
        } else if c.is_alphanumeric() || c == '_' {
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            tokens.push(Token::Ident(&rest[..end]));
            rest = &rest[end..];
        } else {
            tokens.push(Token::Punct(c));
            rest = &rest[c.len_utf8()..];
        }
    }
    tokens
}

/// Parses the subset of Go that `BuiltinGlobalVariableTypes` is written in.
struct Parser<'a> {
    tokens: std::vec::IntoIter<Token<'a>>,
}

impl<'a> Parser<'a> {
    fn next(&mut self) -> Token<'a> {
        self.tokens
            .next()
            .unwrap_or_else(|| panic!("unexpected end of the Go source"))
    }

    fn expect(&mut self, want: &Token<'_>) {
        let got = self.next();
        assert_eq!(&got, want, "unexpected token in the Go source");
    }

    fn expect_all(&mut self, want: &[Token<'_>]) {
        for token in want {
            self.expect(token);
        }
    }

    /// `map[string]ExprType{"name": type, ...}`
    fn map_literal(&mut self) -> BTreeMap<String, Ty> {
        self.expect_all(&[
            Token::Ident("map"),
            Token::Punct('['),
            Token::Ident("string"),
            Token::Punct(']'),
            Token::Ident("ExprType"),
            Token::Punct('{'),
        ]);
        let mut entries = BTreeMap::new();
        loop {
            match self.next() {
                Token::Punct('}') => return entries,
                Token::Str(name) => {
                    self.expect(&Token::Punct(':'));
                    let ty = self.ty();
                    assert!(
                        entries.insert(name.to_owned(), ty).is_none(),
                        "{name:?} appears twice"
                    );
                    self.expect(&Token::Punct(','));
                }
                token => panic!("expected a key or `}}`, got {token:?}"),
            }
        }
    }

    fn ty(&mut self) -> Ty {
        let Token::Ident(name) = self.next() else {
            panic!("expected a type");
        };
        let unit = |parser: &mut Self, ty| {
            parser.expect_all(&[Token::Punct('{'), Token::Punct('}')]);
            ty
        };
        let object = |parser: &mut Self, rest| {
            parser.expect(&Token::Punct('('));
            let props = parser.map_literal();
            parser.expect(&Token::Punct(')'));
            Ty::Object(props, rest)
        };
        let empty = |parser: &mut Self, rest| {
            parser.expect_all(&[Token::Punct('('), Token::Punct(')')]);
            Ty::Object(BTreeMap::new(), rest)
        };
        match name {
            "AnyType" => unit(self, Ty::Any),
            "NullType" => unit(self, Ty::Null),
            "NumberType" => unit(self, Ty::Number),
            "BoolType" => unit(self, Ty::Bool),
            "StringType" => unit(self, Ty::String),
            "NewStrictObjectType" => object(self, OwnedRest::Strict),
            "NewObjectType" => object(self, OwnedRest::Loose),
            "NewEmptyStrictObjectType" => empty(self, OwnedRest::Strict),
            "NewEmptyObjectType" => empty(self, OwnedRest::Loose),
            "NewMapObjectType" => {
                self.expect(&Token::Punct('('));
                let elem = self.ty();
                // gofmt adds a trailing comma when the argument ends its own line.
                let mut close = self.next();
                if close == Token::Punct(',') {
                    close = self.next();
                }
                assert_eq!(
                    close,
                    Token::Punct(')'),
                    "unexpected token in the Go source"
                );
                Ty::Object(BTreeMap::new(), OwnedRest::Map(Box::new(elem)))
            }
            other => panic!("the parser does not know the Go expression {other}"),
        }
    }
}

/// The reference's `BuiltinGlobalVariableTypes`, parsed from the vendored Go source.
fn reference_contexts() -> BTreeMap<String, Ty> {
    let declaration = "var BuiltinGlobalVariableTypes = ";
    let Some(start) = EXPR_SEMA_GO.find(declaration) else {
        panic!("expr_sema.go has no BuiltinGlobalVariableTypes");
    };
    let mut parser = Parser {
        tokens: tokenize(&EXPR_SEMA_GO[start + declaration.len()..]).into_iter(),
    };
    parser.map_literal()
}

fn workflint_contexts() -> BTreeMap<String, Ty> {
    GLOBAL_CONTEXTS
        .iter()
        .map(|context| (context.name.to_owned(), Ty::from(&context.ty)))
        .collect()
}

#[test]
fn global_contexts_match_the_reference_table() {
    let reference = reference_contexts();
    let workflint = workflint_contexts();

    assert_eq!(
        workflint.keys().collect::<Vec<_>>(),
        reference.keys().collect::<Vec<_>>(),
        "context names"
    );
    for (name, ty) in &reference {
        assert_eq!(workflint.get(name), Some(ty), "the type of {name}");
    }
}

#[test]
fn global_context_finds_every_context_by_name() {
    for context in GLOBAL_CONTEXTS {
        assert_eq!(
            global_context(context.name),
            Some(&context.ty),
            "{}",
            context.name
        );
    }
}

#[test]
fn jobs_is_not_a_global_context() {
    assert_eq!(global_context("jobs"), None);
}

#[test]
fn global_context_does_not_find_a_name_in_upper_case() {
    assert_eq!(global_context("GITHUB"), None);
}

/// Every name in `ty`, with the path that leads to it.
fn names(ty: &TypeDesc, path: &str, out: &mut Vec<String>) {
    match ty {
        TypeDesc::Array(elem) => names(elem, &format!("{path}.*"), out),
        TypeDesc::Object(object) => {
            for prop in object.props {
                let path = format!("{path}.{}", prop.name);
                out.push(path.clone());
                names(&prop.ty, &path, out);
            }
            if let Rest::Map(elem) = object.rest {
                names(elem, &format!("{path}.<key>"), out);
            }
        }
        TypeDesc::Any | TypeDesc::Null | TypeDesc::Number | TypeDesc::Bool | TypeDesc::String => {}
    }
}

// The reference compares context and property names without regard to case, so its test
// requires lower-case names. This test also looks inside map elements, which the reference's
// test skips.
#[test]
fn context_and_property_names_have_no_upper_case_letters() {
    let mut all = Vec::new();
    for context in GLOBAL_CONTEXTS {
        all.push(context.name.to_owned());
        names(&context.ty, context.name, &mut all);
    }

    let upper: Vec<&String> = all
        .iter()
        .filter(|path| path.bytes().any(|b| b.is_ascii_uppercase()))
        .collect();
    assert!(upper.is_empty(), "names with upper-case letters: {upper:?}");
}

fn assert_sorted_without_duplicates(object: &ObjectDesc, path: &str) {
    for pair in object.props.windows(2) {
        if let [a, b] = pair {
            assert!(
                a.name < b.name,
                "{path}: {:?} comes before {:?}",
                a.name,
                b.name
            );
        }
    }
    for prop in object.props {
        assert_objects_sorted(&prop.ty, &format!("{path}.{}", prop.name));
    }
    if let Rest::Map(elem) = object.rest {
        assert_objects_sorted(elem, &format!("{path}.<key>"));
    }
}

fn assert_objects_sorted(ty: &TypeDesc, path: &str) {
    match ty {
        TypeDesc::Array(elem) => assert_objects_sorted(elem, &format!("{path}.*")),
        TypeDesc::Object(object) => assert_sorted_without_duplicates(object, path),
        TypeDesc::Any | TypeDesc::Null | TypeDesc::Number | TypeDesc::Bool | TypeDesc::String => {}
    }
}

// `global_context` and `ObjectDesc::prop` search by binary search.
#[test]
fn contexts_and_properties_are_sorted_by_name_without_duplicates() {
    for pair in GLOBAL_CONTEXTS.windows(2) {
        if let [a, b] = pair {
            assert!(a.name < b.name, "{:?} comes before {:?}", a.name, b.name);
        }
    }
    for context in GLOBAL_CONTEXTS {
        assert_objects_sorted(&context.ty, context.name);
    }
}

#[test]
fn prop_finds_a_known_property_and_not_an_unknown_one() {
    let Some(TypeDesc::Object(job)) = global_context("job") else {
        panic!("job is not an object");
    };

    assert!(matches!(job.prop("container"), Some(TypeDesc::Object(_))));
    assert_eq!(job.prop("status"), Some(&TypeDesc::String));
    assert_eq!(job.prop("steps"), None);
}

#[test]
fn crate_depends_on_no_workspace_crate() {
    let manifest = include_str!("../Cargo.toml");

    let workspace_deps: Vec<&str> = manifest
        .lines()
        .filter(|line| line.trim_start().starts_with("workflint"))
        .collect();
    assert!(workspace_deps.is_empty(), "{workspace_deps:?}");
}
