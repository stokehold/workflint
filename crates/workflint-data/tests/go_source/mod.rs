//! Reads tables from the reference's Go files, vendored by `cargo xtask vendor-testdata`, for
//! the tests that compare them with `workflint-data`.

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

/// The reference's `all_webhooks.go`.
const ALL_WEBHOOKS_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/all_webhooks.go");

/// The reference's `rule_action.go`.
const RULE_ACTION_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/rule_action.go");

/// The reference's `rule_deprecated_commands.go`.
const RULE_DEPRECATED_COMMANDS_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/rule_deprecated_commands.go");

/// The reference's `rule_events.go`.
const RULE_EVENTS_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/rule_events.go");

/// The reference's `rule_permissions.go`.
const RULE_PERMISSIONS_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/rule_permissions.go");

/// The reference's `rule_runner_label.go`.
const RULE_RUNNER_LABEL_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/rule_runner_label.go");

/// The reference's `rule_shell_name.go`.
const RULE_SHELL_NAME_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/rule_shell_name.go");

/// The reference's `cmd/actionlint/main.go`.
pub(crate) const MAIN_GO: &str =
    include_str!("../../../../tests/conformance/actionlint/cmd/actionlint/main.go");

/// The time zone names of the Go that builds the reference, written by
/// `cargo xtask vendor-testdata`.
const TIME_ZONES_TXT: &str = include_str!("../../../../tests/conformance/go/time-zones.txt");

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
    /// An interpreted string literal with its escapes kept, or a raw string literal.
    Str(&'a str),
    /// A rune literal, such as `'@'`, with its escapes kept.
    Rune(&'a str),
    Punct(char),
}

/// The length of the quoted literal at the start of `rest`, quotes included. A backslash
/// escapes the next character unless the literal is raw.
fn literal_len(rest: &str, quote: char, raw: bool) -> usize {
    let mut chars = rest.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        if c == quote {
            return i + 1;
        }
        if c == '\\' && !raw {
            chars.next();
        }
    }
    panic!("unterminated literal in {rest:.40}");
}

/// Splits Go source into identifiers, string and rune literals, and punctuation, and drops
/// whitespace and `//` comments. This is enough for the tables and the code around them.
fn tokenize(src: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut rest = src;
    while let Some(c) = rest.chars().next() {
        if c.is_whitespace() {
            rest = &rest[c.len_utf8()..];
        } else if let Some(comment) = rest.strip_prefix("//") {
            rest = comment.find('\n').map_or("", |end| &comment[end..]);
        } else if matches!(c, '"' | '`' | '\'') {
            let len = literal_len(rest, c, c == '`');
            let inner = &rest[1..len - 1];
            tokens.push(if c == '\'' {
                Token::Rune(inner)
            } else {
                Token::Str(inner)
            });
            rest = &rest[len..];
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

/// The secrets that the reference's `UpdateSecrets` adds to the declared ones, with their
/// types.
pub(crate) fn update_secrets_built_ins() -> BTreeMap<String, Ty> {
    Parser::after(EXPR_SEMA_GO, "copied := NewStrictObjectType(").type_map()
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

/// The functions that the reference's `isSafeFuncCall` accepts, in source order. Its body is
/// `return c == "a" || c == "b" ...` after lowercasing the callee into `c`.
pub(crate) fn safe_func_calls() -> Vec<String> {
    let mut parser = Parser::after(EXPR_INSECURE_GO, "c := strings.ToLower(call.Callee)");
    parser.expect(&Token::Ident("return"));
    let mut names = Vec::new();
    loop {
        parser.expect_all(&[Token::Ident("c"), Token::Punct('='), Token::Punct('=')]);
        names.push(parser.str().to_owned());
        if !parser.eat(&Token::Punct('|')) {
            parser.expect(&Token::Punct('}'));
            return names;
        }
        parser.expect(&Token::Punct('|'));
    }
}

impl Parser<'_> {
    /// `{"a", "b", ...}`, a `[]string` literal whose type is elided.
    fn elided_strings(&mut self) -> Vec<String> {
        self.expect(&Token::Punct('{'));
        let mut strings = Vec::new();
        while !self.eat(&Token::Punct('}')) {
            strings.push(self.str().to_owned());
            self.eat(&Token::Punct(','));
        }
        strings
    }

    /// `map[string]<value>{"name": <value>, ...}` in source order, with `value` reading each
    /// value. `value_type` is the tokens of the value type, such as `[Ident("runnerOSCompat")]`.
    fn ordered_map<T>(
        &mut self,
        value_type: &[Token<'_>],
        mut value: impl FnMut(&mut Self) -> T,
    ) -> Vec<(String, T)> {
        self.expect_all(&[
            Token::Ident("map"),
            Token::Punct('['),
            Token::Ident("string"),
            Token::Punct(']'),
        ]);
        self.expect_all(value_type);
        self.expect(&Token::Punct('{'));
        let mut entries: Vec<(String, T)> = Vec::new();
        while !self.eat(&Token::Punct('}')) {
            let name = self.str();
            assert!(
                entries.iter().all(|(other, _)| other != name),
                "{name:?} appears twice"
            );
            self.expect(&Token::Punct(':'));
            entries.push((name.to_owned(), value(self)));
            self.eat(&Token::Punct(','));
        }
        entries
    }

    /// The strings of `case "a", "b":` labels up to `default` or the `}` that ends the switch,
    /// skipping the statements between them. Fails on a nested block that has its own `case`.
    fn case_strings(&mut self) -> Vec<String> {
        let mut strings = Vec::new();
        let mut depth = 0_usize;
        loop {
            match self.next() {
                Token::Ident("case") if depth == 0 => loop {
                    strings.push(self.str().to_owned());
                    if !self.eat(&Token::Punct(',')) {
                        self.expect(&Token::Punct(':'));
                        break;
                    }
                },
                Token::Ident("case") => panic!("a nested switch is not supported"),
                Token::Ident("default") | Token::Punct('}') if depth == 0 => return strings,
                Token::Punct('{') => depth += 1,
                Token::Punct('}') => depth -= 1,
                _ => {}
            }
        }
    }
}

/// The reference's `AllWebhookTypes` in source order. `None` stands for Go's `nil`, which
/// means any activity type.
pub(crate) fn all_webhook_types() -> Vec<(String, Option<Vec<String>>)> {
    Parser::after(ALL_WEBHOOKS_GO, "var AllWebhookTypes = ").ordered_map(
        &[Token::Punct('['), Token::Punct(']'), Token::Ident("string")],
        |parser| {
            if parser.eat(&Token::Ident("nil")) {
                None
            } else {
                Some(parser.elided_strings())
            }
        },
    )
}

/// The reference's `allPermissionScopes` in source order.
pub(crate) fn all_permission_scopes() -> Vec<(String, Vec<String>)> {
    Parser::after(RULE_PERMISSIONS_GO, "var allPermissionScopes = ").ordered_map(
        &[Token::Punct('['), Token::Punct(']'), Token::Ident("string")],
        Parser::elided_strings,
    )
}

/// The values that the reference's `checkPermissions` accepts for all the scopes at once.
pub(crate) fn permissions_for_all_scopes() -> Vec<String> {
    Parser::after(RULE_PERMISSIONS_GO, "switch p.All.Value {").case_strings()
}

/// The names of the reference's `runnerOSCompat` bits, in declaration order. `compatInvalid`,
/// which is 0, is not a bit.
pub(crate) fn runner_os_compat_bits() -> Vec<String> {
    let start = RULE_RUNNER_LABEL_GO
        .find("const (")
        .unwrap_or_else(|| panic!("rule_runner_label.go has no const block"));
    let block = &RULE_RUNNER_LABEL_GO[start + "const (".len()..];
    let end = block
        .find(')')
        .unwrap_or_else(|| panic!("the const block of rule_runner_label.go is not closed"));
    block[..end]
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "compatInvalid")
        .map(str::to_owned)
        .collect()
}

/// The reference's `allGitHubHostedRunnerLabels`.
pub(crate) fn all_github_hosted_runner_labels() -> Vec<String> {
    Parser::after(RULE_RUNNER_LABEL_GO, "var allGitHubHostedRunnerLabels = ").strings()
}

/// The reference's `selfHostedRunnerPresetOSLabels`.
pub(crate) fn self_hosted_runner_preset_os_labels() -> Vec<String> {
    Parser::after(
        RULE_RUNNER_LABEL_GO,
        "var selfHostedRunnerPresetOSLabels = ",
    )
    .strings()
}

/// The reference's `selfHostedRunnerPresetOtherLabels`.
pub(crate) fn self_hosted_runner_preset_other_labels() -> Vec<String> {
    Parser::after(
        RULE_RUNNER_LABEL_GO,
        "var selfHostedRunnerPresetOtherLabels = ",
    )
    .strings()
}

/// The reference's `defaultRunnerOSCompats` in source order, with the names of the bits that
/// each label's value ORs together.
pub(crate) fn default_runner_os_compats() -> Vec<(String, Vec<String>)> {
    Parser::after(RULE_RUNNER_LABEL_GO, "var defaultRunnerOSCompats = ").ordered_map(
        &[Token::Ident("runnerOSCompat")],
        |parser| {
            let mut bits = Vec::new();
            loop {
                match parser.next() {
                    Token::Ident(bit) => bits.push(bit.to_owned()),
                    token => panic!("expected the name of a bit, got {token:?}"),
                }
                if !parser.eat(&Token::Punct('|')) {
                    return bits;
                }
            }
        },
    )
}

/// The arms of the reference's `getAvailableShellNames`, keyed by the platform constant, such
/// as `platformKindAny`.
pub(crate) fn available_shell_names() -> Vec<(String, Vec<String>)> {
    let mut parser = Parser::after(
        RULE_SHELL_NAME_GO,
        "func getAvailableShellNames(kind platformKind) []string {",
    );
    parser.expect_all(&[
        Token::Ident("switch"),
        Token::Ident("kind"),
        Token::Punct('{'),
    ]);
    let mut arms = Vec::new();
    while parser.eat(&Token::Ident("case")) {
        let Token::Ident(platform) = parser.next() else {
            panic!("expected a platform constant");
        };
        parser.expect_all(&[Token::Punct(':'), Token::Ident("return")]);
        arms.push((platform.to_owned(), parser.strings()));
    }
    parser.expect(&Token::Ident("default"));
    arms
}

/// The calls of `checkExclusiveFilters` in the reference's `checkWebhookEvent`: the field of the
/// filter, such as `Paths`, and the events that may have it, in source order.
pub(crate) fn exclusive_filters() -> Vec<(String, Vec<String>)> {
    let call = "rule.checkExclusiveFilters(";
    let mut calls = Vec::new();
    let mut rest = RULE_EVENTS_GO;
    while let Some(start) = rest.find(call) {
        rest = &rest[start + call.len()..];
        if rest.starts_with("filter, ") {
            // The declaration, `func (rule *RuleEvents) checkExclusiveFilters(filter, ...`.
            continue;
        }
        let mut parser = Parser::after(rest, "");
        let field = |parser: &mut Parser<'_>| {
            parser.expect_all(&[Token::Ident("event"), Token::Punct('.')]);
            match parser.next() {
                Token::Ident(field) => field.to_owned(),
                token => panic!("expected a field of the event, got {token:?}"),
            }
        };
        let filter = field(&mut parser);
        parser.expect(&Token::Punct(','));
        let ignore = field(&mut parser);
        assert_eq!(ignore, format!("{filter}Ignore"));
        parser.expect_all(&[Token::Punct(','), Token::Ident("hook"), Token::Punct(',')]);
        calls.push((filter, parser.strings()));
    }
    calls
}

/// The number in the reference's `len(event.Inputs) > N` check of `workflow_dispatch` inputs.
pub(crate) fn max_workflow_dispatch_inputs() -> usize {
    let mut parser = Parser::after(RULE_EVENTS_GO, "if len(event.Inputs) > ");
    match parser.next() {
        Token::Ident(number) => number
            .parse()
            .unwrap_or_else(|_| panic!("{number:?} is not a number")),
        token => panic!("expected a number, got {token:?}"),
    }
}

/// The seconds in the reference's `diff < 60.0*N` check of cron intervals.
pub(crate) fn min_schedule_interval_seconds() -> u64 {
    let mut parser = Parser::after(RULE_EVENTS_GO, "if diff < ");
    parser.expect_all(&[
        Token::Ident("60"),
        Token::Punct('.'),
        Token::Ident("0"),
        Token::Punct('*'),
    ]);
    match parser.next() {
        Token::Ident(minutes) => {
            60 * minutes
                .parse::<u64>()
                .unwrap_or_else(|_| panic!("{minutes:?} is not a number"))
        }
        token => panic!("expected a number, got {token:?}"),
    }
}

/// The reference's `deprecatedCommandsPattern`.
pub(crate) fn deprecated_commands_pattern() -> String {
    let mut parser = Parser::after(
        RULE_DEPRECATED_COMMANDS_GO,
        "var deprecatedCommandsPattern = regexp.MustCompile(",
    );
    parser.str().to_owned()
}

/// The replacement that the reference's `VisitStep` of `deprecated-commands` names for each
/// command, in source order.
pub(crate) fn deprecated_command_replacements() -> Vec<(String, String)> {
    let mut parser = Parser::after(RULE_DEPRECATED_COMMANDS_GO, "switch c {");
    let mut arms = Vec::new();
    while parser.eat(&Token::Ident("case")) {
        let command = parser.str().to_owned();
        parser.expect_all(&[Token::Punct(':'), Token::Ident("a"), Token::Punct('=')]);
        arms.push((command, parser.str().to_owned()));
    }
    parser.expect(&Token::Ident("default"));
    arms
}

/// `map[string]struct{}{"name": {}, ...}` after `declaration` in `rule_action.go`, in source
/// order.
fn rule_action_set(declaration: &str) -> Vec<String> {
    Parser::after(RULE_ACTION_GO, declaration)
        .ordered_map(
            &[Token::Ident("struct"), Token::Punct('{'), Token::Punct('}')],
            |parser| parser.expect_all(&[Token::Punct('{'), Token::Punct('}')]),
        )
        .into_iter()
        .map(|(name, ())| name)
        .collect()
}

/// The reference's `BrandingColors` in source order.
pub(crate) fn branding_colors() -> Vec<String> {
    rule_action_set("var BrandingColors = ")
}

/// The reference's `BrandingIcons` in source order.
pub(crate) fn branding_icons() -> Vec<String> {
    rule_action_set("var BrandingIcons = ")
}

/// The prefixes in the reference's `isImageOnDockerRegistry`, in source order.
pub(crate) fn docker_registry_prefixes() -> Vec<String> {
    let mut parser = Parser::after(
        RULE_ACTION_GO,
        "func isImageOnDockerRegistry(image string) bool {",
    );
    parser.expect(&Token::Ident("return"));
    let mut prefixes = Vec::new();
    loop {
        parser.expect_all(&[
            Token::Ident("strings"),
            Token::Punct('.'),
            Token::Ident("HasPrefix"),
            Token::Punct('('),
            Token::Ident("image"),
            Token::Punct(','),
        ]);
        prefixes.push(parser.str().to_owned());
        parser.expect(&Token::Punct(')'));
        if !parser.eat(&Token::Punct('|')) {
            parser.expect(&Token::Punct('}'));
            return prefixes;
        }
        parser.expect(&Token::Punct('|'));
    }
}

/// The `runs.using` values that the reference's `checkLocalActionRuns` checks, without the
/// empty string that stands for a missing value.
pub(crate) fn runs_using_values() -> Vec<String> {
    let mut values =
        Parser::after(RULE_ACTION_GO, "switch r := &meta.Runs; r.Using {").case_strings();
    values.retain(|value| !value.is_empty());
    values
}

/// The message that the reference's `checkLocalActionRuns` reports for an invalid
/// `runs.using`.
pub(crate) fn invalid_runs_using_message() -> String {
    let switch = "switch r := &meta.Runs; r.Using {";
    let Some(start) = RULE_ACTION_GO.find(switch) else {
        panic!("rule_action.go has no {switch:?}");
    };
    let mut parser = Parser::after(&RULE_ACTION_GO[start..], "default:");
    parser.expect_all(&[
        Token::Ident("rule"),
        Token::Punct('.'),
        Token::Ident("Errorf"),
        Token::Punct('('),
        Token::Ident("pos"),
        Token::Punct(','),
    ]);
    parser.str().to_owned()
}

/// The time zones that `cargo xtask vendor-testdata` listed, and the tzdata version that its
/// first line names.
pub(crate) fn go_time_zones() -> (String, Vec<String>) {
    let mut lines = TIME_ZONES_TXT.lines();
    let first = lines.next().unwrap_or_default();
    let Some((_, version)) = first.rsplit_once(", tzdata ") else {
        panic!("the first line of time-zones.txt names no tzdata version: {first}");
    };
    let version = version.trim_end_matches('.').to_owned();
    let names = lines
        .filter(|line| !line.starts_with('#'))
        .map(str::to_owned)
        .collect();
    (version, names)
}
