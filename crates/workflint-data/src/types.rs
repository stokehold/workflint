//! A description of expression types that platform data is written in.

/// The type of an expression value, as platform data describes it.
///
/// The variants are the types of actionlint's expression checker. `workflint-expr` converts a
/// `TypeDesc` to its own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeDesc {
    /// Any value. Reading a property of it gives `Any`.
    Any,
    /// `null`.
    Null,
    /// A number. Integers and floats are the same type.
    Number,
    /// `true` or `false`.
    Bool,
    /// A string.
    String,
    /// An array whose elements have the given type.
    Array(&'static TypeDesc),
    /// An object.
    Object(ObjectDesc),
}

impl TypeDesc {
    /// A strict object with the given properties.
    pub(crate) const fn strict(props: &'static [Property]) -> Self {
        Self::Object(ObjectDesc {
            props,
            rest: Rest::Strict,
        })
    }

    /// A loose object with the given properties.
    pub(crate) const fn loose(props: &'static [Property]) -> Self {
        Self::Object(ObjectDesc {
            props,
            rest: Rest::Loose,
        })
    }

    /// An object without known properties whose every property has type `elem`.
    pub(crate) const fn map(elem: &'static TypeDesc) -> Self {
        Self::Object(ObjectDesc {
            props: &[],
            rest: Rest::Map(elem),
        })
    }
}

/// The type of an object: its known properties and what it allows beyond them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectDesc {
    /// The known properties, sorted by name in byte order, with no name twice. Names are lower
    /// case, because expressions compare property names without regard to case.
    pub props: &'static [Property],
    /// The properties the object has besides [`props`](Self::props).
    pub rest: Rest,
}

impl ObjectDesc {
    /// Returns the type of the known property `name`, which must be lower case.
    #[must_use]
    pub fn prop(&self, name: &str) -> Option<&'static TypeDesc> {
        let props = self.props;
        let index = props.binary_search_by(|prop| prop.name.cmp(name)).ok()?;
        props.get(index).map(|prop| &prop.ty)
    }
}

/// The properties an object has besides its known ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rest {
    /// None. Reading an unknown property is an error.
    Strict,
    /// Any property, of type [`TypeDesc::Any`].
    Loose,
    /// Any property, of the given type, as in `env` and `secrets`.
    Map(&'static TypeDesc),
}

/// A known property of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Property {
    /// The name, in lower case.
    pub name: &'static str,
    /// The type of the value.
    pub ty: TypeDesc,
}

impl Property {
    pub(crate) const fn new(name: &'static str, ty: TypeDesc) -> Self {
        Self { name, ty }
    }
}
