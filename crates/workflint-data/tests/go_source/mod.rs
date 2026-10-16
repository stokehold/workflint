//! Reads tables from the reference's `expr_sema.go`, `availability.go`, and `expr_insecure.go`,
//! vendored by `cargo xtask vendor-testdata`, for the tests that compare them with
//! `workflint-data`.

// Each test file uses part of this module.
#![allow(clippy::panic, dead_code)]

use std::collections::BTreeMap;
use std::fmt;

use workflint_data::{Rest, TypeDesc};

/// The reference's `expr_sema.go`.
const EXPR_SEMA_GO: &str = include_str!("../../../../tests/conformance/actionlint/expr_sema.go");

/// The reference's `availability.go`.
const AVAILABILITY_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/availability.go");

/// The reference's `expr_insecure.go`.
const EXPR_INSECURE_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/expr_insecure.go");

/// An owned expression type, built from the Go source or from a [`TypeDesc`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Any,
    Null,
    Number,
    Bool,
    String,
    Array(Box<Ty>),
    Object(BTreeMap<String, Ty>, OwnedRest),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OwnedRest {
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

impl fmt::Display for Ty {
    /// Writes the type as the reference's `String()` methods do.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any => f.write_str("any"),
            Ty::Null => f.write_str("null"),
            Ty::Number => f.write_str("number"),
            Ty::Bool => f.write_str("bool"),
            Ty::String => f.write_str("string"),
            Ty::Array(elem) => write!(f, "array<{elem}>"),
            Ty::Object(_, OwnedRest::Loose) => f.write_str("object"),
            Ty::Object(_, OwnedRest::Map(elem)) => write!(f, "{{string => {elem}}}"),
            Ty::Object(props, OwnedRest::Strict) => {
                let props: Vec<String> = props
                    .iter()
                    .map(|(name, ty)| format!("{name}: {ty}"))
                    .collect();
                write!(f, "{{{}}}", props.join("; "))
            }
        }
    }
}

/// One overload of the reference's `FuncSignature`.
#[derive(Debug, Default)]
pub(crate) struct FuncSignature {
    pub(crate) name: String,
    pub(crate) ret: Option<Ty>,
    pub(crate) params: Vec<Ty>,
    pub(crate) variable_length_params: bool,
    pub(crate) is_const_func: bool,
}

impl fmt::Display for FuncSignature {
    /// Writes the signature as the reference's `FuncSignature.String()` does, such as
    /// `hashFiles(string...) -> string`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params: Vec<String> = self.params.iter().map(ToString::to_string).collect();
        let ellipsis = if self.variable_length_params {
            "..."
        } else {
            ""
        };
        let ret = self
            .ret
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        write!(f, "{}({}{ellipsis}) -> {ret}", self.name, params.join(", "))
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

/// Parses the subset of Go that the reference's tables are written in.
struct Parser<'a> {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token<'a>>>,
}

impl<'a> Parser<'a> {
    /// A parser of `src` after `declaration`, such as `var Foo = `.
    fn after(src: &'a str, declaration: &str) -> Self {
        let Some(start) = src.find(declaration) else {
            panic!("the Go source has no {declaration:?}");
        };
        Parser {
            tokens: tokenize(&src[start + declaration.len()..])
                .into_iter()
                .peekable(),
        }
    }

    fn next(&mut self) -> Token<'a> {
        self.tokens
            .next()
            .unwrap_or_else(|| panic!("unexpected end of the Go source"))
    }

    /// Consumes the next token if it is `want`.
    fn eat(&mut self, want: &Token<'_>) -> bool {
        let found = self.tokens.peek() == Some(want);
        if found {
            self.tokens.next();
        }
        found
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
    fn type_map(&mut self) -> BTreeMap<String, Ty> {
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
        if self.eat(&Token::Punct('&')) {
            // `&ArrayType{Elem: type}`
            self.expect_all(&[
                Token::Ident("ArrayType"),
                Token::Punct('{'),
                Token::Ident("Elem"),
                Token::Punct(':'),
            ]);
            let elem = self.ty();
            self.eat(&Token::Punct(','));
            self.expect(&Token::Punct('}'));
            return Ty::Array(Box::new(elem));
        }
        let Token::Ident(name) = self.next() else {
            panic!("expected a type");
        };
        let unit = |parser: &mut Self, ty| {
            parser.expect_all(&[Token::Punct('{'), Token::Punct('}')]);
            ty
        };
        let object = |parser: &mut Self, rest| {
            parser.expect(&Token::Punct('('));
            let props = parser.type_map();
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
                self.eat(&Token::Punct(','));
                self.expect(&Token::Punct(')'));
                Ty::Object(BTreeMap::new(), OwnedRest::Map(Box::new(elem)))
            }
            other => panic!("the parser does not know the Go expression {other}"),
        }
    }

    /// `{Name: "name", Ret: type, Params: []ExprType{...}, ...}`
    fn func_signature(&mut self) -> FuncSignature {
        self.expect(&Token::Punct('{'));
        let mut sig = FuncSignature::default();
        loop {
            let field = match self.next() {
                Token::Punct('}') => return sig,
                Token::Ident(field) => field,
                token => panic!("expected a field or `}}`, got {token:?}"),
            };
            self.expect(&Token::Punct(':'));
            match field {
                "Name" => {
                    let Token::Str(name) = self.next() else {
                        panic!("expected the name of a function");
                    };
                    name.clone_into(&mut sig.name);
                }
                "Ret" => sig.ret = Some(self.ty()),
                "Params" => {
                    self.expect_all(&[
                        Token::Punct('['),
                        Token::Punct(']'),
                        Token::Ident("ExprType"),
                        Token::Punct('{'),
                    ]);
                    while !self.eat(&Token::Punct('}')) {
                        sig.params.push(self.ty());
                        self.eat(&Token::Punct(','));
                    }
                }
                "VariableLengthParams" => sig.variable_length_params = self.bool(),
                "IsConstFunc" => sig.is_const_func = self.bool(),
                other => panic!("the parser does not know the field {other}"),
            }
            self.expect(&Token::Punct(','));
        }
    }

    fn str(&mut self) -> &'a str {
        match self.next() {
            Token::Str(s) => s,
            token => panic!("expected a string, got {token:?}"),
        }
    }

    /// `[]string{"a", "b", ...}`
    fn strings(&mut self) -> Vec<String> {
        self.expect_all(&[
            Token::Punct('['),
            Token::Punct(']'),
            Token::Ident("string"),
            Token::Punct('{'),
        ]);
        let mut strings = Vec::new();
        while !self.eat(&Token::Punct('}')) {
            strings.push(self.str().to_owned());
            self.eat(&Token::Punct(','));
        }
        strings
    }

    /// `map[string][]string{"name": []string{...}, ...}`
    fn strings_map(&mut self) -> BTreeMap<String, Vec<String>> {
        self.expect_all(&[
            Token::Ident("map"),
            Token::Punct('['),
            Token::Ident("string"),
            Token::Punct(']'),
            Token::Punct('['),
            Token::Punct(']'),
            Token::Ident("string"),
            Token::Punct('{'),
        ]);
        let mut entries = BTreeMap::new();
        while !self.eat(&Token::Punct('}')) {
            let name = self.str();
            self.expect(&Token::Punct(':'));
            let strings = self.strings();
            assert!(
                entries.insert(name.to_owned(), strings).is_none(),
                "{name:?} appears twice"
            );
            self.eat(&Token::Punct(','));
        }
        entries
    }

    /// The arms of `switch key { case "a", "b": return []string{...}, []string{...} ... }` up to
    /// `default`, keyed by each string of each `case`.
    fn availability_switch(&mut self) -> BTreeMap<String, Availability> {
        self.expect_all(&[
            Token::Ident("switch"),
            Token::Ident("key"),
            Token::Punct('{'),
        ]);
        let mut entries = BTreeMap::new();
        while self.eat(&Token::Ident("case")) {
            let mut keys = vec![self.str()];
            while self.eat(&Token::Punct(',')) {
                keys.push(self.str());
            }
            self.expect_all(&[Token::Punct(':'), Token::Ident("return")]);
            let contexts = self.strings();
            self.expect(&Token::Punct(','));
            let special_functions = self.strings();
            for key in keys {
                let availability = Availability {
                    contexts: contexts.clone(),
                    special_functions: special_functions.clone(),
                };
                assert!(
                    entries.insert(key.to_owned(), availability).is_none(),
                    "{key:?} appears twice"
                );
            }
        }
        self.expect(&Token::Ident("default"));
        entries
    }

    fn bool(&mut self) -> bool {
        match self.next() {
            Token::Ident("true") => true,
            Token::Ident("false") => false,
            token => panic!("expected a bool, got {token:?}"),
        }
    }

    /// `NewUntrustedInputMap("name", child, ...)`. Pushes the path of each node without
    /// children, which is an untrusted value, to `paths`.
    fn untrusted_input_map(&mut self, prefix: &str, paths: &mut Vec<String>) {
        self.expect_all(&[Token::Ident("NewUntrustedInputMap"), Token::Punct('(')]);
        let Token::Str(name) = self.next() else {
            panic!("expected the name of an untrusted input");
        };
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}.{name}")
        };
        let mut leaf = true;
        // gofmt adds a comma after the last argument when it ends its own line.
        while self.eat(&Token::Punct(',')) {
            if self.tokens.peek() == Some(&Token::Punct(')')) {
                break;
            }
            self.untrusted_input_map(&path, paths);
            leaf = false;
        }
        self.expect(&Token::Punct(')'));
        if leaf {
            paths.push(path);
        }
    }

    /// `UntrustedInputSearchRoots{"name": NewUntrustedInputMap(...), ...}`
    fn untrusted_input_roots(&mut self) -> Vec<String> {
        self.expect_all(&[Token::Ident("UntrustedInputSearchRoots"), Token::Punct('{')]);
        let mut paths = Vec::new();
        loop {
            match self.next() {
                Token::Punct('}') => return paths,
                Token::Str(_) => {
                    self.expect(&Token::Punct(':'));
                    self.untrusted_input_map("", &mut paths);
                    self.expect(&Token::Punct(','));
                }
                token => panic!("expected a key or `}}`, got {token:?}"),
            }
        }
    }

    /// `map[string][]*FuncSignature{"name": {{...}, ...}, ...}`
    fn signature_map(&mut self) -> BTreeMap<String, Vec<FuncSignature>> {
        self.expect_all(&[
            Token::Ident("map"),
            Token::Punct('['),
            Token::Ident("string"),
            Token::Punct(']'),
            Token::Punct('['),
            Token::Punct(']'),
            Token::Punct('*'),
            Token::Ident("FuncSignature"),
            Token::Punct('{'),
        ]);
        let mut entries = BTreeMap::new();
        loop {
            match self.next() {
                Token::Punct('}') => return entries,
                Token::Str(name) => {
                    self.expect_all(&[Token::Punct(':'), Token::Punct('{')]);
                    let mut overloads = Vec::new();
                    while self.tokens.peek() == Some(&Token::Punct('{')) {
                        overloads.push(self.func_signature());
                        self.eat(&Token::Punct(','));
                    }
                    self.expect_all(&[Token::Punct('}'), Token::Punct(',')]);
                    assert!(
                        entries.insert(name.to_owned(), overloads).is_none(),
                        "{name:?} appears twice"
                    );
                }
                token => panic!("expected a key or `}}`, got {token:?}"),
            }
        }
    }
}

/// The reference's `BuiltinGlobalVariableTypes`.
pub(crate) fn builtin_global_variable_types() -> BTreeMap<String, Ty> {
    Parser::after(EXPR_SEMA_GO, "var BuiltinGlobalVariableTypes = ").type_map()
}

/// The reference's `BuiltinFuncSignatures`.
pub(crate) fn builtin_func_signatures() -> BTreeMap<String, Vec<FuncSignature>> {
    Parser::after(EXPR_SEMA_GO, "var BuiltinFuncSignatures = ").signature_map()
}

/// What one workflow key allows in the reference's `WorkflowKeyAvailability`, with the names in
/// the reference's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Availability {
    pub(crate) contexts: Vec<String>,
    pub(crate) special_functions: Vec<String>,
}

/// The reference's `WorkflowKeyAvailability`, as a map from each workflow key that it knows.
pub(crate) fn workflow_key_availability() -> BTreeMap<String, Availability> {
    Parser::after(
        AVAILABILITY_GO,
        "func WorkflowKeyAvailability(key string) ([]string, []string) {",
    )
    .availability_switch()
}

/// The reference's `SpecialFunctionNames`.
pub(crate) fn special_function_names() -> BTreeMap<String, Vec<String>> {
    Parser::after(AVAILABILITY_GO, "var SpecialFunctionNames = ").strings_map()
}

/// The reference's `AllContexts`.
pub(crate) fn all_contexts() -> BTreeMap<String, Vec<String>> {
    Parser::after(AVAILABILITY_GO, "var AllContexts = ").strings_map()
}

/// The paths of the untrusted values in the reference's `BuiltinUntrustedInputs`, such as
/// `github.event.commits.*.message`, in source order.
pub(crate) fn builtin_untrusted_inputs() -> Vec<String> {
    Parser::after(EXPR_INSECURE_GO, "var BuiltinUntrustedInputs = ").untrusted_input_roots()
}
