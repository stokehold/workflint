//! The types of expression values.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `expr_type.go`, except for
//! `typeOfJSONValue`, which is in `json.rs`. A type is an immutable
//! value. Objects and arrays share their parts through `Arc`, so cloning a type is cheap, and
//! nothing changes a type that other expressions use (`docs/differences.md#object-filter-mutation`).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use workflint_data::{Rest as RestDesc, TypeDesc};

use crate::ast::simple_lower;

/// The deepest nesting of objects and arrays that a value from data, such as a `fromJSON()`
/// literal or a matrix value, gets a type for. An object or array nested deeper has type `any`,
/// so that printing, merging, and comparing types, which recurse, cannot overflow the stack
/// (`docs/differences.md#fromjson-nesting`).
pub const MAX_TYPE_DEPTH: usize = 128;

/// The type of an expression value.
///
/// `==` compares the structure of two types, including the flag that an array came from a
/// `.*` filter. [`equivalent`](Self::equivalent) is the reference's `EqualTypes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// A value whose type is unknown. It is assignable to every type, and every type is
    /// assignable to it.
    Any,
    /// `null`.
    Null,
    /// A number. Integers and floats are the same type.
    Number,
    /// `true` or `false`.
    Bool,
    /// A string.
    String,
    /// An object.
    Object(Arc<ObjectTy>),
    /// An array.
    Array(ArrayTy),
}

/// The value that [`ObjectTy::unknown_prop_ty`] returns for a loose object.
static ANY: Ty = Ty::Any;

impl Ty {
    /// A loose object with the given properties. Reading an unknown property gives `any`.
    pub fn loose<K: Into<String>>(props: impl IntoIterator<Item = (K, Ty)>) -> Self {
        ObjectTy::new(props, Rest::Loose).into()
    }

    /// A loose object without known properties.
    #[must_use]
    pub fn empty_loose() -> Self {
        ObjectTy::empty(Rest::Loose).into()
    }

    /// A strict object with the given properties. Reading an unknown property is an error.
    pub fn strict<K: Into<String>>(props: impl IntoIterator<Item = (K, Ty)>) -> Self {
        ObjectTy::new(props, Rest::Strict).into()
    }

    /// A strict object without properties.
    #[must_use]
    pub fn empty_strict() -> Self {
        ObjectTy::empty(Rest::Strict).into()
    }

    /// An object without known properties whose every property has type `elem`, as in `env`.
    /// A map of `any` is a loose object.
    #[must_use]
    pub fn map(elem: Ty) -> Self {
        ObjectTy::empty(Rest::Map(elem)).into()
    }

    /// An array whose elements have type `elem`.
    #[must_use]
    pub fn array(elem: Ty) -> Self {
        Ty::Array(ArrayTy::new(elem, false))
    }

    /// An array whose elements have type `elem`, which a `.*` filter returned.
    #[must_use]
    pub fn filtered_array(elem: Ty) -> Self {
        Ty::Array(ArrayTy::new(elem, true))
    }

    /// Returns whether a value of type `from` can be assigned to a place of this type, such as
    /// a function parameter. This is the reference's `Assignable`, with the receiver as the
    /// target.
    ///
    /// `bool` accepts only `bool` and `any`, while the reference's accepts every type
    /// (`docs/differences.md#bool-assignability`). Where GitHub converts a value to a
    /// boolean, use [`converts_to_bool`](Self::converts_to_bool) instead.
    #[must_use]
    pub fn accepts(&self, from: &Ty) -> bool {
        if matches!(from, Ty::Any) {
            return true;
        }
        match self {
            Ty::Any => true,
            Ty::Null => matches!(from, Ty::Null),
            // The reference has a TODO here: "Is string of numbers coerced into number?"
            Ty::Number => matches!(from, Ty::Number),
            Ty::Bool => matches!(from, Ty::Bool),
            // GitHub also converts bool and null to strings, but the reference rejects them,
            // because such a conversion is almost always a mistake.
            Ty::String => matches!(from, Ty::String | Ty::Number),
            Ty::Object(to) => match from {
                Ty::Object(from) => to.accepts(from),
                _ => false,
            },
            Ty::Array(to) => match from {
                Ty::Array(from) => to.elem().accepts(from.elem()),
                _ => false,
            },
        }
    }

    /// Returns whether a value of this type can be converted to a boolean, as GitHub does in
    /// `if:` and for the operands of `!`, `&&`, and `||`. Every type can. The predicates of
    /// `case()` are not converted (`docs/differences.md#case-predicate-type`).
    #[must_use]
    pub fn converts_to_bool(&self) -> bool {
        match self {
            Ty::Any
            | Ty::Null
            | Ty::Number
            | Ty::Bool
            | Ty::String
            | Ty::Object(_)
            | Ty::Array(_) => true,
        }
    }

    /// Returns whether each type accepts the other. This is the reference's `EqualTypes`.
    #[must_use]
    pub fn equivalent(&self, other: &Ty) -> bool {
        self.accepts(other) && other.accepts(self)
    }

    /// Returns the type of a value that has this type or the type `other`, as for the result
    /// of `||`. A conflict gives `any`. This is the reference's `Merge`, with the same
    /// asymmetries: `number` with `bool` is `any`, but either of them with `string` is
    /// `string`.
    #[must_use]
    pub fn merge(&self, other: &Ty) -> Ty {
        let mut merged = self.clone();
        merged.merge_in_place(other);
        merged
    }

    /// Replaces this type with its [`merge`](Self::merge) with `other`. An object or array
    /// whose parts no other type shares changes in place instead of being copied, so merging
    /// many types into one accumulator takes time linear in their total size.
    pub(crate) fn merge_in_place(&mut self, other: &Ty) {
        match (&mut *self, other) {
            (Ty::Null, Ty::Null)
            | (Ty::Number, Ty::Number)
            | (Ty::Bool, Ty::Bool)
            | (Ty::String, Ty::String | Ty::Number | Ty::Bool) => {}
            (Ty::Number | Ty::Bool, Ty::String) => *self = Ty::String,
            (Ty::Object(ty), Ty::Object(other)) => ObjectTy::merge_in_place(ty, other),
            (Ty::Array(ty), Ty::Array(other)) => ty.merge_in_place(other),
            _ => *self = Ty::Any,
        }
    }
}

impl fmt::Display for Ty {
    /// Writes the type as the reference's `String()` does: `{a: T; b: U}` for a strict object,
    /// `object` for a loose one, `{string => T}` for a map, `array<T>`, and the name of each
    /// unit type.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any => f.write_str("any"),
            Ty::Null => f.write_str("null"),
            Ty::Number => f.write_str("number"),
            Ty::Bool => f.write_str("bool"),
            Ty::String => f.write_str("string"),
            Ty::Object(object) => object.fmt(f),
            Ty::Array(array) => write!(f, "array<{}>", array.elem()),
        }
    }
}

impl From<ObjectTy> for Ty {
    fn from(object: ObjectTy) -> Self {
        Ty::Object(Arc::new(object))
    }
}

impl From<ArrayTy> for Ty {
    fn from(array: ArrayTy) -> Self {
        Ty::Array(array)
    }
}

impl From<&TypeDesc> for Ty {
    /// Converts a type from `workflint-data`. Arrays are not filtered.
    fn from(desc: &TypeDesc) -> Self {
        match desc {
            TypeDesc::Any => Ty::Any,
            TypeDesc::Null => Ty::Null,
            TypeDesc::Number => Ty::Number,
            TypeDesc::Bool => Ty::Bool,
            TypeDesc::String => Ty::String,
            TypeDesc::Array(elem) => Ty::array(Ty::from(*elem)),
            TypeDesc::Object(object) => {
                let props = object
                    .props
                    .iter()
                    .map(|prop| (prop.name, Ty::from(&prop.ty)));
                let rest = match object.rest {
                    RestDesc::Strict => Rest::Strict,
                    RestDesc::Loose => Rest::Loose,
                    RestDesc::Map(elem) => Rest::Map(Ty::from(elem)),
                };
                ObjectTy::new(props, rest).into()
            }
        }
    }
}

/// The type of an object: its known properties and what it allows beyond them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTy {
    props: BTreeMap<String, Ty>,
    rest: Rest,
}

/// The properties an object type allows besides its known ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rest {
    /// None. Reading an unknown property is an error.
    Strict,
    /// Any property, of type `any`.
    Loose,
    /// Any property, of the given type, as in `env`. [`ObjectTy`] stores a map of `any` as
    /// [`Loose`](Self::Loose).
    Map(Ty),
}

impl ObjectTy {
    /// An object type with the given properties. A name given twice keeps its last type.
    pub fn new<K: Into<String>>(props: impl IntoIterator<Item = (K, Ty)>, rest: Rest) -> Self {
        let props = props
            .into_iter()
            .map(|(name, ty)| (name.into(), ty))
            .collect();
        Self::from_parts(props, rest)
    }

    fn empty(rest: Rest) -> Self {
        Self::from_parts(BTreeMap::new(), rest)
    }

    fn from_parts(props: BTreeMap<String, Ty>, rest: Rest) -> Self {
        let rest = match rest {
            Rest::Map(Ty::Any) => Rest::Loose,
            rest => rest,
        };
        Self { props, rest }
    }

    /// The known properties, sorted by name in byte order.
    #[must_use]
    pub fn props(&self) -> &BTreeMap<String, Ty> {
        &self.props
    }

    /// Returns the type of the known property `name`. Names are compared exactly, so the
    /// caller lowercases the name that an expression reads.
    #[must_use]
    pub fn prop(&self, name: &str) -> Option<&Ty> {
        self.props.get(name)
    }

    /// Returns the type of the known property `name`, which is in lower case, as an
    /// expression reads it. The exact name wins. Otherwise the first name in byte order that
    /// lowercases to `name` matches, because the keys of a `fromJSON()` literal keep their
    /// case (`docs/differences.md#fromjson-key-case`).
    #[must_use]
    pub fn prop_ignore_case(&self, name: &str) -> Option<&Ty> {
        self.props.get(name).or_else(|| {
            self.props
                .iter()
                .find(|(prop, _)| prop.chars().map(simple_lower).eq(name.chars()))
                .map(|(_, ty)| ty)
        })
    }

    /// The properties the object allows besides the known ones.
    #[must_use]
    pub fn rest(&self) -> &Rest {
        &self.rest
    }

    /// Returns the type of a property that is not known: `None` for a strict object, `any` for
    /// a loose one, and the element type for a map. This is the reference's `Mapped`.
    #[must_use]
    pub fn unknown_prop_ty(&self) -> Option<&Ty> {
        match &self.rest {
            Rest::Strict => None,
            Rest::Loose => Some(&ANY),
            Rest::Map(elem) => Some(elem),
        }
    }

    /// Returns whether the object allows no unknown properties.
    #[must_use]
    pub fn is_strict(&self) -> bool {
        matches!(self.rest, Rest::Strict)
    }

    /// Returns whether the object allows unknown properties of any type.
    #[must_use]
    pub fn is_loose(&self) -> bool {
        matches!(self.rest, Rest::Loose)
    }

    /// Returns a copy with the given rest. This replaces the reference's `Strict()` and
    /// `Loose()`, which change the object in place.
    #[must_use]
    pub fn with_rest(&self, rest: Rest) -> Self {
        Self::from_parts(self.props.clone(), rest)
    }

    fn accepts(&self, from: &ObjectTy) -> bool {
        match (self.unknown_prop_ty(), from.unknown_prop_ty()) {
            (Some(to), Some(from)) => to.accepts(from),
            (Some(to), None) => from.props.values().all(|ty| to.accepts(ty)),
            (None, Some(from)) => self.props.values().all(|ty| ty.accepts(from)),
            (None, None) => from
                .props
                .iter()
                .all(|(name, from)| self.props.get(name).is_some_and(|to| to.accepts(from))),
        }
    }

    /// The reference's `ObjectType.Merge`. It adds the new properties of `other` to the
    /// element type in byte order of their names, where the reference uses Go map order.
    /// The result can depend on that order (`docs/differences.md#object-merge-order`).
    ///
    /// It changes `ty` in place when no other type shares it, and copies it otherwise.
    fn merge_in_place(ty: &mut Arc<ObjectTy>, other: &Arc<ObjectTy>) {
        // The reference returns one of its inputs in these two cases. The general case below
        // would build an equal type.
        if ty.props.is_empty() && other.is_loose() {
            *ty = Arc::clone(other);
            return;
        }
        if other.props.is_empty() && ty.is_loose() {
            return;
        }

        let object = Arc::make_mut(ty);
        let own_mapped = match std::mem::replace(&mut object.rest, Rest::Strict) {
            Rest::Strict => None,
            Rest::Loose => Some(Ty::Any),
            Rest::Map(elem) => Some(elem),
        };
        let mut mapped = match (own_mapped, other.unknown_prop_ty()) {
            (None, mapped) => mapped.cloned(),
            (Some(mapped), None) => Some(mapped),
            (Some(mut mapped), Some(other)) => {
                mapped.merge_in_place(other);
                Some(mapped)
            }
        };
        for (name, other) in &other.props {
            if let Some(prop) = object.props.get_mut(name) {
                prop.merge_in_place(other);
                continue;
            }
            object.props.insert(name.clone(), other.clone());
            if let Some(ty) = &mut mapped {
                ty.merge_in_place(other);
            }
        }
        object.rest = match mapped {
            None => Rest::Strict,
            Some(Ty::Any) => Rest::Loose,
            Some(elem) => Rest::Map(elem),
        };
    }
}

impl fmt::Display for ObjectTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.rest {
            Rest::Loose => f.write_str("object"),
            Rest::Map(elem) => write!(f, "{{string => {elem}}}"),
            Rest::Strict => {
                f.write_str("{")?;
                for (i, (name, ty)) in self.props.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "{name}: {ty}")?;
                }
                f.write_str("}")
            }
        }
    }
}

/// The type of an array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayTy {
    elem: Arc<Ty>,
    filtered: bool,
}

impl ArrayTy {
    fn new(elem: Ty, filtered: bool) -> Self {
        Self {
            elem: Arc::new(elem),
            filtered,
        }
    }

    /// The type of the elements.
    #[must_use]
    pub fn elem(&self) -> &Ty {
        &self.elem
    }

    /// Returns whether a `.*` filter returned the array. Reading a property of such an array
    /// reads it from each element. This is the reference's `Deref`.
    #[must_use]
    pub fn is_filtered(&self) -> bool {
        self.filtered
    }

    /// Returns the same array type, marked as returned by a `.*` filter. The reference sets
    /// the flag on the receiver in place.
    #[must_use]
    pub fn to_filtered(&self) -> Self {
        Self {
            elem: Arc::clone(&self.elem),
            filtered: true,
        }
    }

    /// The reference's `ArrayType.Merge`. It changes the element type in place when no other
    /// type shares it, and copies it otherwise.
    fn merge_in_place(&mut self, other: &ArrayTy) {
        // The reference returns one of its inputs in these two cases, with its flag.
        if matches!(self.elem(), Ty::Any) {
            return;
        }
        if matches!(other.elem(), Ty::Any) {
            *self = other.clone();
            return;
        }
        Arc::make_mut(&mut self.elem).merge_in_place(other.elem());
        // Merging two filtered arrays breaks the chain of property reads.
        self.filtered = false;
    }
}
