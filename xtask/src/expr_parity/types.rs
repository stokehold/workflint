//! The context types of a setup, written as the reference prints types.
//!
//! `tests/expr-parity/setups.toml` gives each type as text such as
//! `{os: string; node: number}`. [`parse`] reads it into a [`TypeSpec`], which
//! the Go program receives as JSON and [`TypeSpec::to_ty`] turns into a
//! `workflint-expr` type.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Serialize;
use workflint_expr::{ObjectTy, Rest, Ty};

/// The kind of a [`TypeSpec`]. The names are those that `xtask/expr-check`
/// reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Any,
    Null,
    Number,
    Bool,
    String,
    Array,
    /// An object without other properties.
    Strict,
    /// An object whose other properties are `any`.
    Loose,
    /// An object whose other properties have the type of `elem`.
    Map,
}

/// A type of the expression language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TypeSpec {
    pub(crate) kind: Kind,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) props: BTreeMap<String, TypeSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) elem: Option<Box<TypeSpec>>,
}

impl TypeSpec {
    fn scalar(kind: Kind) -> Self {
        Self {
            kind,
            props: BTreeMap::new(),
            elem: None,
        }
    }

    /// Whether the type is an object, as the contexts of a setup must be.
    pub(crate) fn is_object(&self) -> bool {
        matches!(self.kind, Kind::Strict | Kind::Loose | Kind::Map)
    }

    /// The `workflint-expr` type.
    pub(crate) fn to_ty(&self) -> Ty {
        match self.kind {
            Kind::Any => Ty::Any,
            Kind::Null => Ty::Null,
            Kind::Number => Ty::Number,
            Kind::Bool => Ty::Bool,
            Kind::String => Ty::String,
            Kind::Array => Ty::array(self.elem_ty()),
            Kind::Strict | Kind::Loose | Kind::Map => Ty::Object(self.to_object().into()),
        }
    }

    /// The `workflint-expr` object type. A type that is not an object gives an
    /// empty strict object.
    pub(crate) fn to_object(&self) -> ObjectTy {
        let rest = match self.kind {
            Kind::Loose => Rest::Loose,
            Kind::Map => Rest::Map(self.elem_ty()),
            _ => Rest::Strict,
        };
        let props = self
            .props
            .iter()
            .map(|(name, ty)| (name.clone(), ty.to_ty()));
        ObjectTy::new(props, rest)
    }

    fn elem_ty(&self) -> Ty {
        self.elem.as_deref().map_or(Ty::Any, Self::to_ty)
    }
}

/// Parses a type in the reference's printed form: `any`, `null`, `number`,
/// `bool`, `string`, `object` for an empty loose object, `array<T>`,
/// `{string => T}` for a map, and `{a: T; b: U}` for a strict object.
pub(crate) fn parse(text: &str) -> Result<TypeSpec> {
    let mut parser = Parser { text, pos: 0 };
    let ty = parser.ty()?;
    parser.skip_spaces();
    if parser.pos != text.len() {
        bail!("unexpected {:?} in type {text:?}", &text[parser.pos..]);
    }
    Ok(ty)
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn skip_spaces(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, token: &str) -> bool {
        self.skip_spaces();
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str) -> Result<()> {
        if !self.eat(token) {
            bail!(
                "expected {token:?} at {:?} in type {:?}",
                self.rest(),
                self.text
            );
        }
        Ok(())
    }

    fn ty(&mut self) -> Result<TypeSpec> {
        if self.eat("{") {
            return self.object();
        }
        if self.eat("array<") {
            let elem = self.ty()?;
            self.expect(">")?;
            return Ok(TypeSpec {
                kind: Kind::Array,
                props: BTreeMap::new(),
                elem: Some(Box::new(elem)),
            });
        }
        let word: String = self
            .rest()
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        let kind = match word.as_str() {
            "any" => Kind::Any,
            "null" => Kind::Null,
            "number" => Kind::Number,
            "bool" => Kind::Bool,
            "string" => Kind::String,
            "object" => Kind::Loose,
            _ => bail!("unknown type at {:?} in type {:?}", self.rest(), self.text),
        };
        self.pos += word.len();
        Ok(TypeSpec::scalar(kind))
    }

    /// The rest of an object type after its `{`.
    fn object(&mut self) -> Result<TypeSpec> {
        if self.eat("}") {
            return Ok(TypeSpec::scalar(Kind::Strict));
        }
        let start = self.pos;
        if self.eat("string") && self.eat("=>") {
            let elem = self.ty()?;
            self.expect("}")?;
            return Ok(TypeSpec {
                kind: Kind::Map,
                props: BTreeMap::new(),
                elem: Some(Box::new(elem)),
            });
        }
        self.pos = start;
        let mut props = BTreeMap::new();
        loop {
            self.skip_spaces();
            let Some(colon) = self.rest().find(':') else {
                bail!(
                    "expected a property at {:?} in type {:?}",
                    self.rest(),
                    self.text
                );
            };
            let name = self.rest()[..colon].trim_end().to_owned();
            if name.is_empty() || name.contains(['{', '}', ';']) {
                bail!(
                    "expected a property at {:?} in type {:?}",
                    self.rest(),
                    self.text
                );
            }
            self.pos += colon + 1;
            let ty = self.ty()?;
            if props.insert(name.clone(), ty).is_some() {
                bail!("property {name:?} appears twice in type {:?}", self.text);
            }
            if self.eat("}") {
                break;
            }
            self.expect(";")?;
        }
        Ok(TypeSpec {
            kind: Kind::Strict,
            props,
            elem: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parsed_type_prints_as_its_text() {
        for text in [
            "any",
            "null",
            "number",
            "bool",
            "string",
            "object",
            "array<string>",
            "{string => string}",
            "{}",
            "{os: string}",
            "{build: {conclusion: string; outcome: string; outputs: {string => string}}; job-index: number}",
            "array<{a: array<number>}>",
        ] {
            assert_eq!(parse(text).unwrap().to_ty().to_string(), text);
        }
    }

    #[test]
    fn a_map_of_any_is_a_loose_object() {
        assert_eq!(
            parse("{string => any}").unwrap().to_ty().to_string(),
            "object"
        );
    }

    #[test]
    fn spaces_around_tokens_are_ignored() {
        assert_eq!(
            parse(" { a : array< number > ; b:{string=>bool} } ")
                .unwrap()
                .to_ty()
                .to_string(),
            "{a: array<number>; b: {string => bool}}"
        );
    }

    #[test]
    fn malformed_types_are_errors() {
        for text in [
            "",
            "str",
            "{a: string",
            "{a string}",
            "{a: string;}",
            "{a: string; a: number}",
            "array<string",
            "string extra",
            "{: string}",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn the_go_program_receives_kinds_props_and_elements() {
        let spec = parse("{a: array<string>; b: {string => number}}").unwrap();
        assert_eq!(
            serde_json::to_string(&spec).unwrap(),
            r#"{"kind":"strict","props":{"a":{"kind":"array","elem":{"kind":"string"}},"b":{"kind":"map","elem":{"kind":"number"}}}}"#
        );
    }
}
