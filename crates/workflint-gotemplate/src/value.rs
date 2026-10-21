//! The values that templates read and functions take and return.

use std::collections::BTreeMap;
use std::sync::Arc;

/// A value that a template can read: the data passed to [`Template::execute`], a literal, a
/// variable, or the result of a function.
///
/// Each value stands for a Go value of some type, and errors name that type as Go would.
/// Scalars have Go's types `bool`, `int`, `float64`, and `string`. Lists, maps, and structs
/// carry the Go type name that the caller gives them, such as `[]*actionlint.ErrorTemplateFields`.
///
/// Compound values share their contents, so cloning a value is cheap.
///
/// [`Template::execute`]: crate::Template::execute
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Go's `nil`: a missing map entry, a field of nothing, or a nil interface.
    Nil,
    /// A Go `bool`.
    Bool(bool),
    /// A Go `int`.
    Int(i64),
    /// A Go `float64`.
    Float(f64),
    /// A Go `string`.
    String(String),
    /// A Go slice.
    List(List),
    /// A Go map with string keys.
    Map(Map),
    /// A Go struct, or a pointer to one when its type name starts with `*`.
    Struct(Struct),
}

impl Value {
    /// A list of type `[]interface {}`.
    #[must_use]
    pub fn list(items: impl IntoIterator<Item = Value>) -> Value {
        Value::List(List::new("[]interface {}", items))
    }

    /// A map of type `map[string]interface {}`.
    #[must_use]
    pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Value)>) -> Value {
        Value::Map(Map::new("map[string]interface {}", entries))
    }

    /// The Go type of the value, as Go's `reflect.Type.String` prints it.
    #[must_use]
    pub fn type_name(&self) -> String {
        match self {
            Value::Nil => "<nil>".to_owned(),
            Value::Bool(_) => "bool".to_owned(),
            Value::Int(_) => "int".to_owned(),
            Value::Float(_) => "float64".to_owned(),
            Value::String(_) => "string".to_owned(),
            Value::List(list) => list.type_name().to_owned(),
            Value::Map(map) => map.type_name().to_owned(),
            Value::Struct(record) => record.type_name().to_owned(),
        }
    }

    /// Whether the value is true in the sense of Go's `template.IsTrue`: not the zero value of
    /// its type. Structs are always true.
    #[must_use]
    pub fn is_true(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(x) => *x != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(list) => !list.is_empty(),
            Value::Map(map) => !map.is_empty(),
            Value::Struct(_) => true,
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Int(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Int(i64::from(value))
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::Int(i64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl From<List> for Value {
    fn from(value: List) -> Self {
        Value::List(value)
    }
}

impl From<Map> for Value {
    fn from(value: Map) -> Self {
        Value::Map(value)
    }
}

impl From<Struct> for Value {
    fn from(value: Struct) -> Self {
        Value::Struct(value)
    }
}

/// A Go slice: its type name and its items.
#[derive(Debug, Clone, PartialEq)]
pub struct List {
    type_name: Arc<str>,
    items: Arc<[Value]>,
}

impl List {
    /// A slice of type `type_name`, such as `[]string`.
    #[must_use]
    pub fn new(type_name: &str, items: impl IntoIterator<Item = Value>) -> List {
        List {
            type_name: Arc::from(type_name),
            items: items.into_iter().collect(),
        }
    }

    /// The Go type of the slice.
    #[must_use]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// The number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the slice has no items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The item at `index`.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.items.get(index)
    }

    /// The items in order.
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.items.iter()
    }

    /// Whether Go types the items as `interface {}`, as in `[]interface {}`.
    pub(crate) fn has_interface_items(&self) -> bool {
        self.type_name.ends_with("]interface {}")
    }

    /// A new slice of the same type with the items in `range`.
    pub(crate) fn slice(&self, start: usize, end: usize) -> List {
        List {
            type_name: Arc::clone(&self.type_name),
            items: self.items.get(start..end).unwrap_or_default().into(),
        }
    }
}

/// A Go map with string keys: its type name and its entries, sorted by key.
#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    type_name: Arc<str>,
    entries: Arc<BTreeMap<String, Value>>,
}

impl Map {
    /// A map of type `type_name`, such as `map[string]int`. A later entry replaces an earlier one
    /// with the same key.
    #[must_use]
    pub fn new<K: Into<String>>(
        type_name: &str,
        entries: impl IntoIterator<Item = (K, Value)>,
    ) -> Map {
        Map {
            type_name: Arc::from(type_name),
            entries: Arc::new(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect(),
            ),
        }
    }

    /// The Go type of the map.
    #[must_use]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value for `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    /// The entries, sorted by key as Go's `fmt` and `range` sort them.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.entries.iter()
    }

    /// Whether Go types the values as `interface {}`, as in `map[string]interface {}`.
    pub(crate) fn has_interface_values(&self) -> bool {
        self.type_name.ends_with("]interface {}")
    }
}

/// A Go struct: its type name and its exported fields, in declaration order.
#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    type_name: Arc<str>,
    fields: Arc<[(String, Value)]>,
}

impl Struct {
    /// A struct of type `type_name`. A type name that starts with `*`, such as
    /// `*actionlint.ErrorTemplateFields`, makes it a pointer to a struct, which `printf`
    /// prints with a leading `&`.
    #[must_use]
    pub fn new<K: Into<String>>(
        type_name: &str,
        fields: impl IntoIterator<Item = (K, Value)>,
    ) -> Struct {
        Struct {
            type_name: Arc::from(type_name),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        }
    }

    /// The Go type of the struct.
    #[must_use]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// The value of the field `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }

    /// Whether both structs share their fields, as two copies of one Go pointer do.
    pub(crate) fn same_fields(&self, other: &Struct) -> bool {
        Arc::ptr_eq(&self.fields, &other.fields)
    }

    /// The fields in declaration order.
    pub fn fields(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.fields
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}
