//! Typed decoding of the YAML tree, as go-yaml's `Unmarshal` decodes YAML
//! into Go values.
//!
//! The reference reads local `action.yml` files, the `on:` section of local
//! reusable workflows, and its config file with go-yaml's typed decoder,
//! not with its workflow parser. That decoder has rules of its own:
//!
//! - A bool accepts the YAML 1.1 forms such as `yes`, `on`, and `off`, also
//!   when quoted.
//! - Exact duplicate keys in a mapping are errors,
//!   `mapping key "x" already defined at line N`.
//! - Keys that a struct has no field for are ignored, and their values are
//!   not read.
//! - A `<<` key merges mappings into structs and maps.
//! - Most errors are collected, and go-yaml returns them together as a
//!   `TypeError`. A few, such as an alias that contains itself, end
//!   decoding at once.
//!
//! A [`Decoder`] is the state of one go-yaml decoder. Each of its decoding
//! methods stands for decoding a node into one Go type, and reports
//! errors with go-yaml's texts and line numbers. Plain scalars are typed by
//! the YAML 1.2 core schema, as everywhere in workflint
//! (docs/differences.md#yaml-core-schema), so a message names the tag the
//! core schema gives.

mod go_value;
mod scalar;
mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{LineCol, SourceFile};
use workflint_yaml::event::{ErrorKind, EventData, EventParser};
use workflint_yaml::{Document, Entry, Node, NodeId, NodeKind, Scalar, ScalarStyle, Tag};

use go_value::{GoValue, Key, KeyError};
pub(crate) use scalar::Resolved;
use scalar::{resolve, yaml_1_1_bool};
use walk::{Output, Request};

/// The text of the error that the reference reports for a local action or
/// reusable workflow whose metadata cannot be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataError {
    message: String,
}

impl MetadataError {
    pub(crate) fn new(message: String) -> Self {
        Self { message }
    }

    /// Returns the text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MetadataError {}

/// One error that does not stop decoding, go-yaml's `UnmarshalError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnmarshalError {
    line: usize,
    message: String,
}

impl fmt::Display for UnmarshalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// An error that ends decoding at once: a go-yaml `failf` panic. The text
/// starts with `yaml: `.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fatal(String);

impl Fatal {
    pub(crate) fn new(message: String) -> Self {
        Self(message)
    }
}

/// The error of a go-yaml `Unmarshal` or `Node.Decode` call, or of an
/// `UnmarshalYAML` method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// A `TypeError`: the errors that did not stop decoding, in order.
    Type(Vec<UnmarshalError>),
    /// Any other error, such as a YAML syntax error or a [`Fatal`] error.
    Other(String),
}

impl fmt::Display for DecodeError {
    /// Formats the error as Go's `err.Error()` does.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Type(errors) => {
                f.write_str("yaml: unmarshal errors:")?;
                for error in errors {
                    write!(f, "\n  {error}")?;
                }
                Ok(())
            }
            Self::Other(message) => f.write_str(message),
        }
    }
}

/// The most errors that one duplicate key check reports. go-yaml reports a
/// pair for every two equal keys, which grows with the square of the
/// number of keys.
const MAX_DUPLICATE_KEY_ERRORS: usize = 1000;

/// A parsed file with an index of its tree as written.
pub(crate) struct Source<'a> {
    file: &'a SourceFile,
    document: Document,
    /// The children of each node as written: the items of a sequence, or
    /// the keys and values of a mapping in turn, with aliases kept as alias
    /// nodes. Indexed by node.
    children: Vec<Vec<NodeId>>,
    /// The anchored node of each alias, also of a recursive alias, whose
    /// anchored node encloses it.
    alias_targets: BTreeMap<NodeId, NodeId>,
}

impl<'a> Source<'a> {
    /// Parses `file`.
    ///
    /// # Errors
    ///
    /// Returns go-yaml's error text if the file is not valid YAML or has an
    /// alias to an unknown anchor.
    pub(crate) fn parse(file: &'a SourceFile) -> Result<Self, String> {
        let Some(document) = workflint_yaml::parse(file).document else {
            return Err(parse_error(file));
        };
        let mut children = vec![Vec::new(); document.nodes().count()];
        let mut alias_targets = BTreeMap::new();
        let mut anchors: BTreeMap<&str, NodeId> = BTreeMap::new();
        // The open collections and how many children each still has.
        let mut open: Vec<(NodeId, usize)> = Vec::new();
        for (id, node) in document.nodes() {
            if let Some((parent, remaining)) = open.last_mut() {
                if let Some(list) = children.get_mut(index(*parent)) {
                    list.push(id);
                }
                *remaining = remaining.saturating_sub(1);
            }
            if let Some(anchor) = &node.anchor {
                anchors.insert(&anchor.name, id);
            }
            let len = match &node.kind {
                NodeKind::Sequence(sequence) => document.items(sequence).len(),
                NodeKind::Mapping(mapping) => document.entries(mapping).len() * 2,
                NodeKind::Alias(alias) => {
                    let target = alias.target.or_else(|| anchors.get(&*alias.name).copied());
                    if let Some(target) = target {
                        alias_targets.insert(id, target);
                    }
                    0
                }
                NodeKind::Scalar(_) => 0,
            };
            if len > 0 {
                open.push((id, len));
            }
            while open.last().is_some_and(|&(_, remaining)| remaining == 0) {
                open.pop();
            }
        }
        Ok(Self {
            file,
            document,
            children,
            alias_targets,
        })
    }

    /// Decodes the root node as go-yaml's `Unmarshal` does. `decode` gets
    /// `None` for a file without a root, which go-yaml leaves undecoded.
    pub(crate) fn unmarshal<T>(
        &self,
        decode: impl FnOnce(&mut Decoder<'_>, Option<NodeId>) -> Result<T, Fatal>,
    ) -> Result<T, DecodeError> {
        let mut decoder = Decoder::new(self);
        let root = self.document.root();
        let result = match root {
            // go-yaml decodes the document node, then its content.
            Some(_) => decoder.enter().and_then(|()| decode(&mut decoder, root)),
            None => decode(&mut decoder, None),
        };
        decoder.finish(result)
    }

    /// Decodes `node` with a new decoder, as go-yaml's `Node.Decode` does.
    pub(crate) fn decode<T>(
        &self,
        node: NodeId,
        decode: impl FnOnce(&mut Decoder<'_>, NodeId) -> Result<T, Fatal>,
    ) -> Result<T, DecodeError> {
        let mut decoder = Decoder::new(self);
        let result = decode(&mut decoder, node);
        decoder.finish(result)
    }

    pub(crate) fn node(&self, id: NodeId) -> Option<&Node> {
        self.document.node(id)
    }

    /// Returns the children of `id` as written.
    pub(crate) fn written(&self, id: NodeId) -> &[NodeId] {
        self.children.get(index(id)).map_or(&[], Vec::as_slice)
    }

    /// Returns the entries of the mapping `id` as written.
    pub(crate) fn written_entries(&self, id: NodeId) -> impl Iterator<Item = Entry> + '_ {
        let (pairs, _) = self.written(id).as_chunks::<2>();
        pairs.iter().map(|&[key, value]| Entry { key, value })
    }

    /// Returns the node that the alias `id` refers to, or `id` if it is not
    /// an alias.
    pub(crate) fn resolve_alias(&self, id: NodeId) -> NodeId {
        self.alias_targets.get(&id).copied().unwrap_or(id)
    }

    /// Returns go-yaml's `Value` of a node: the value of a scalar, the
    /// anchor name of an alias, and the empty string for a collection.
    pub(crate) fn go_value(&self, id: NodeId) -> &str {
        match self.node(id).map(|node| &node.kind) {
            Some(NodeKind::Scalar(scalar)) => &scalar.value,
            Some(NodeKind::Alias(alias)) => &alias.name,
            _ => "",
        }
    }

    /// Returns the 1-based line and column where go-yaml places `id`.
    pub(crate) fn position(&self, id: NodeId) -> LineCol {
        let offset = self.node(id).map_or(0, |node| node.span.start);
        self.file.line_col(offset)
    }

    /// Returns whether `id` is a `<<` merge key.
    pub(crate) fn is_merge_key(&self, id: NodeId) -> bool {
        match self.node(id).map(|node| &node.kind) {
            Some(NodeKind::Scalar(scalar)) => {
                &*scalar.value == "<<"
                    && match &scalar.tag {
                        Tag::Str => scalar.style == ScalarStyle::Plain,
                        Tag::Other(tag) => &**tag == "!!merge",
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    /// Returns whether `id`, with aliases resolved, is a mapping.
    pub(crate) fn is_mapping(&self, id: NodeId) -> bool {
        matches!(
            self.node(self.resolve_alias(id)).map(|node| &node.kind),
            Some(NodeKind::Mapping(_))
        )
    }

    /// Returns the error of an `UnmarshalYAML` method that wants a mapping
    /// at `id`, as the reference's `expectedMapping` words it.
    pub(crate) fn expected_mapping(&self, place: &str, id: NodeId) -> DecodeError {
        let kind = match self.node(id).map(|node| &node.kind) {
            Some(NodeKind::Sequence(_)) => "sequence",
            Some(NodeKind::Mapping(_)) => "mapping",
            Some(NodeKind::Alias(_)) => "alias",
            Some(NodeKind::Scalar(_)) | None => "scalar",
        };
        let position = self.position(id);
        DecodeError::Other(format!(
            "yaml: {place} must be mapping node but {kind} node was found at line:{}, col:{}",
            position.line, position.column
        ))
    }
}

fn index(id: NodeId) -> usize {
    usize::try_from(id.index()).unwrap_or(usize::MAX)
}

/// Returns go-yaml's error text for a file that `workflint-yaml` rejects:
/// the first syntax error or alias to an unknown anchor in event order.
fn parse_error(file: &SourceFile) -> String {
    let mut anchors = BTreeSet::new();
    for event in EventParser::new(file.text()) {
        let event = match event {
            Ok(event) => event,
            Err(err) => return syntax_error(&err),
        };
        match event.data {
            EventData::Alias { anchor } if !anchors.contains(&anchor) => {
                return format!("yaml: unknown anchor '{anchor}' referenced");
            }
            EventData::Scalar {
                anchor: Some(anchor),
                ..
            }
            | EventData::SequenceStart {
                anchor: Some(anchor),
                ..
            }
            | EventData::MappingStart {
                anchor: Some(anchor),
                ..
            } => {
                anchors.insert(anchor);
            }
            _ => {}
        }
    }
    "yaml: unknown problem parsing YAML content".to_owned()
}

/// Formats a syntax error as go-yaml's `ParserError` does.
///
/// go-yaml takes the 0-based line of the context mark, or of the problem
/// mark if the context is on the first line, and adds 1 only for scanner
/// errors. A line of 0 is left out.
fn syntax_error(err: &workflint_yaml::event::Error) -> String {
    let scanner = u64::from(err.kind() == ErrorKind::Scanner);
    let line = [err.context_mark(), err.problem_mark()]
        .into_iter()
        .flatten()
        .map(|mark| mark.line)
        .find(|&line| line != 0)
        .map_or(0, |line| line + scanner);
    let problem = match err.problem() {
        "" => "unknown problem parsing YAML content",
        problem => problem,
    };
    if line == 0 {
        format!("yaml: {problem}")
    } else {
        format!("yaml: line {line}: {problem}")
    }
}

/// The state of one go-yaml decoder.
pub(crate) struct Decoder<'s> {
    source: &'s Source<'s>,
    errors: Vec<UnmarshalError>,
    /// The aliases being expanded, innermost last.
    expanding: Vec<NodeId>,
    /// go-yaml's `mergedFields`: while a `<<` value is merged, the keys
    /// already set, which the merged mappings do not override.
    merged: Option<BTreeSet<Key>>,
    decode_count: u64,
    alias_count: u64,
    alias_depth: u32,
}

impl<'s> Decoder<'s> {
    fn new(source: &'s Source<'s>) -> Self {
        Self {
            source,
            errors: Vec::new(),
            expanding: Vec::new(),
            merged: None,
            decode_count: 0,
            alias_count: 0,
            alias_depth: 0,
        }
    }

    fn finish<T>(self, result: Result<T, Fatal>) -> Result<T, DecodeError> {
        match result {
            Err(Fatal(message)) => Err(DecodeError::Other(message)),
            Ok(_) if !self.errors.is_empty() => Err(DecodeError::Type(self.errors)),
            Ok(value) => Ok(value),
        }
    }

    /// Returns the parsed file.
    pub(crate) fn source(&self) -> &'s Source<'s> {
        self.source
    }

    fn line(&self, id: NodeId) -> usize {
        self.source.position(id).line
    }

    fn error(&mut self, id: NodeId, message: String) {
        let line = self.line(id);
        self.errors.push(UnmarshalError { line, message });
    }

    /// Counts one decoding step and ends decoding if aliases make up too
    /// much of the work, as go-yaml does against alias bombs.
    fn enter(&mut self) -> Result<(), Fatal> {
        self.decode_count += 1;
        if self.alias_depth > 0 {
            self.alias_count += 1;
        }
        if self.alias_count > 100 && self.decode_count > 1000 && self.excessive_aliasing() {
            return Err(Fatal::new(
                "yaml: document contains excessive aliasing".to_owned(),
            ));
        }
        Ok(())
    }

    /// Returns whether `alias_count / decode_count` is above go-yaml's
    /// allowed ratio: 0.99 up to 400,000 steps, 0.10 from 4,000,000 steps,
    /// and a straight line between. The comparison is done in integers.
    fn excessive_aliasing(&self) -> bool {
        const LOW: u128 = 400_000;
        const HIGH: u128 = 4_000_000;
        let aliases = u128::from(self.alias_count);
        let decodes = u128::from(self.decode_count);
        // aliases / decodes > ratio, with ratio = 0.99 - 0.89 * (decodes -
        // LOW) / (HIGH - LOW), times 100 * decodes * (HIGH - LOW).
        let range = HIGH - LOW;
        let lhs = aliases * 100 * range;
        let rhs = if decodes <= LOW {
            99 * decodes * range
        } else if decodes >= HIGH {
            10 * decodes * range
        } else {
            99 * decodes * range - 89 * (decodes - LOW) * decodes
        };
        lhs > rhs
    }

    /// Decodes `id` as go-yaml's `d.unmarshal` does before it looks at the
    /// target type: it counts the step and follows an alias. `decode` gets
    /// the node with aliases resolved.
    fn unmarshal<T>(
        &mut self,
        id: NodeId,
        decode: impl FnOnce(&mut Self, NodeId, &'s Node) -> Result<T, Fatal>,
    ) -> Result<T, Fatal> {
        let (id, node, aliases) = self.enter_node(id)?;
        let result = decode(self, id, node);
        self.leave_aliases(aliases);
        result
    }

    /// Counts the decoding step for `id` and follows it if it is an alias,
    /// as `unmarshal` does. Returns the node with aliases resolved and the
    /// number of aliases entered, which the caller leaves with
    /// [`leave_aliases`](Self::leave_aliases) once that node is decoded.
    fn enter_node(&mut self, mut id: NodeId) -> Result<(NodeId, &'s Node, u32), Fatal> {
        let mut aliases = 0;
        loop {
            self.enter()?;
            let Some(node) = self.source.node(id) else {
                return Err(Fatal::new("yaml: node not found".to_owned()));
            };
            let NodeKind::Alias(alias) = &node.kind else {
                return Ok((id, node, aliases));
            };
            if self.expanding.contains(&id) {
                return Err(Fatal::new(format!(
                    "yaml: anchor '{}' value contains itself",
                    alias.name
                )));
            }
            let target = self.source.resolve_alias(id);
            if target == id {
                return Err(Fatal::new(format!(
                    "yaml: unknown anchor '{}' referenced",
                    alias.name
                )));
            }
            self.expanding.push(id);
            self.alias_depth += 1;
            aliases += 1;
            id = target;
        }
    }

    /// Leaves the innermost `count` aliases that
    /// [`enter_node`](Self::enter_node) entered.
    fn leave_aliases(&mut self, count: u32) {
        for _ in 0..count {
            self.alias_depth = self.alias_depth.saturating_sub(1);
            self.expanding.pop();
        }
    }

    /// Records a type error for decoding `id` into the Go type `go_type`:
    /// `cannot unmarshal !!str `foo` into bool`.
    fn type_error(&mut self, id: NodeId, go_type: &str) {
        let Some(node) = self.source.node(id) else {
            return;
        };
        let tag = node.tag().map_or("", Tag::as_str);
        let value = match &node.kind {
            NodeKind::Scalar(scalar) => &scalar.value,
            _ => "",
        };
        let shown = if tag == "!!seq" || tag == "!!map" {
            String::new()
        } else if value.len() > 10 {
            // go-yaml cuts the value at 7 bytes, even inside a character.
            let head = value.as_bytes().get(..7).unwrap_or_default();
            format!(" `{}...`", String::from_utf8_lossy(head))
        } else {
            format!(" `{value}`")
        };
        self.error(id, format!("cannot unmarshal {tag}{shown} into {go_type}"));
    }

    /// Records a type error for decoding the collection `id` into the Go
    /// type `go_type`. go-yaml checks a mapping for duplicate keys before it
    /// looks at the type, so duplicates are reported instead.
    fn collection_mismatch(&mut self, id: NodeId, node: &Node, go_type: &str) {
        if matches!(node.kind, NodeKind::Mapping(_)) && !self.unique_keys(id) {
            return;
        }
        self.type_error(id, go_type);
    }

    /// Decodes a scalar into a Go type that no scalar fits, such as a
    /// struct: null leaves the target unchanged and anything else is a
    /// type error.
    fn scalar_mismatch(
        &mut self,
        id: NodeId,
        scalar: &Scalar,
        go_type: &str,
    ) -> Result<bool, Fatal> {
        if resolve(scalar)? != Resolved::Null {
            self.type_error(id, go_type);
        }
        Ok(false)
    }

    /// Decodes `id` into a `yaml.Node`, which keeps the node as written.
    pub(crate) fn raw(&mut self, id: NodeId) -> Result<NodeId, Fatal> {
        self.enter()?;
        Ok(id)
    }

    /// Decodes `id` into a type with an `UnmarshalYAML` method, which
    /// `unmarshal` implements. go-yaml does not call the method for a null
    /// node, and then this returns `None`. An error of the method is
    /// recorded at `id`, and then this returns `None` too.
    pub(crate) fn custom<T>(
        &mut self,
        id: NodeId,
        unmarshal: impl FnOnce(&mut Self, NodeId) -> Result<T, DecodeError>,
    ) -> Result<Option<T>, Fatal> {
        self.unmarshal(id, |d, id, node| {
            if let NodeKind::Scalar(scalar) = &node.kind
                && scalar.tag == Tag::Null
            {
                resolve(scalar)?;
                return Ok(None);
            }
            match unmarshal(d, id) {
                Ok(value) => Ok(Some(value)),
                Err(DecodeError::Type(errors)) => {
                    d.errors.extend(errors);
                    Ok(None)
                }
                Err(DecodeError::Other(message)) => {
                    d.error(id, message);
                    Ok(None)
                }
            }
        })
    }

    /// Decodes `id` into a `string`. Returns whether `out` was set.
    pub(crate) fn string(&mut self, id: NodeId, out: &mut String) -> Result<bool, Fatal> {
        self.unmarshal(id, |d, id, node| {
            let NodeKind::Scalar(scalar) = &node.kind else {
                d.collection_mismatch(id, node, "string");
                return Ok(false);
            };
            match resolve(scalar)? {
                Resolved::Null => Ok(false),
                Resolved::Str(decoded) if scalar::is_binary(&scalar.tag) => {
                    *out = decoded;
                    Ok(true)
                }
                _ => {
                    out.clear();
                    out.push_str(&scalar.value);
                    Ok(true)
                }
            }
        })
    }

    /// Decodes `id` into a `*string`: null sets `None`, and anything else
    /// sets `Some`, also if it is not a string.
    pub(crate) fn optional_string(
        &mut self,
        id: NodeId,
        out: &mut Option<String>,
    ) -> Result<bool, Fatal> {
        let null = match self
            .source
            .node(self.source.resolve_alias(id))
            .map(|node| &node.kind)
        {
            Some(NodeKind::Scalar(scalar)) => scalar.tag == Tag::Null,
            _ => false,
        };
        if null {
            // go-yaml resolves the null, which fails for `!!null x`.
            let mut ignored = String::new();
            self.string(id, &mut ignored)?;
            *out = None;
            return Ok(true);
        }
        self.string(id, out.get_or_insert_default())
    }

    /// Decodes `id` into a `bool`, which also takes the YAML 1.1 forms.
    /// Returns whether `out` was set.
    pub(crate) fn bool(&mut self, id: NodeId, out: &mut bool) -> Result<bool, Fatal> {
        self.unmarshal(id, |d, id, node| {
            let NodeKind::Scalar(scalar) = &node.kind else {
                d.collection_mismatch(id, node, "bool");
                return Ok(false);
            };
            let value = match resolve(scalar)? {
                Resolved::Null => return Ok(false),
                Resolved::Bool(value) => Some(value),
                Resolved::Str(text) => yaml_1_1_bool(&text),
                _ => None,
            };
            let Some(value) = value else {
                d.type_error(id, "bool");
                return Ok(false);
            };
            *out = value;
            Ok(true)
        })
    }

    /// Decodes `id` into a `[]interface{}`. `non_nil` says whether the
    /// slice is set, which is all the reference reads of one.
    pub(crate) fn any_slice(&mut self, id: NodeId, non_nil: &mut bool) -> Result<bool, Fatal> {
        self.unmarshal(id, |d, id, node| match &node.kind {
            NodeKind::Sequence(_) => {
                *non_nil = true;
                for &item in d.source.written(id) {
                    d.any(item, false)?;
                }
                Ok(true)
            }
            NodeKind::Scalar(scalar) if resolve(scalar)? == Resolved::Null => {
                *non_nil = false;
                Ok(true)
            }
            _ => {
                d.collection_mismatch(id, node, "[]interface {}");
                Ok(false)
            }
        })
    }

    /// Decodes `id` into a `map[string]interface{}`, whose values the
    /// reference never reads. `non_nil` says whether the map is set.
    pub(crate) fn any_map(&mut self, id: NodeId, non_nil: &mut bool) -> Result<bool, Fatal> {
        self.unmarshal(id, |d, id, node| match &node.kind {
            NodeKind::Mapping(_) => {
                if !d.unique_keys(id) {
                    return Ok(false);
                }
                *non_nil = true;
                d.walk(Request::Entries {
                    id,
                    string_keys: true,
                    build: false,
                })?;
                Ok(true)
            }
            NodeKind::Scalar(scalar) if resolve(scalar)? == Resolved::Null => {
                *non_nil = false;
                Ok(true)
            }
            _ => {
                d.collection_mismatch(id, node, "map[string]interface {}");
                Ok(false)
            }
        })
    }

    /// Decodes `id` into an `interface{}`. Builds the value only if
    /// `build`, which is needed for map keys.
    fn any(&mut self, id: NodeId, build: bool) -> Result<Option<GoValue>, Fatal> {
        match self.walk(Request::Any { id, build })? {
            Output::Value(value) => Ok(value),
            Output::Entries(_) => Ok(None),
        }
    }

    /// Returns whether go-yaml decodes the mapping `id` into a
    /// `map[string]interface{}`: every key is a string or a merge key.
    fn is_string_map(&self, id: NodeId) -> bool {
        self.source.written_entries(id).all(|entry| {
            let key = self.source.resolve_alias(entry.key);
            matches!(self.source.node(key).and_then(Node::tag), Some(Tag::Str))
                || self.source.is_merge_key(key)
        })
    }

    /// Runs go-yaml's duplicate key check on the mapping `id`: every two
    /// keys of the same kind with the same `Value`, as written, are an
    /// error. Returns whether there were none.
    fn unique_keys(&mut self, id: NodeId) -> bool {
        let source = self.source;
        let keys: Vec<NodeId> = source.written_entries(id).map(|entry| entry.key).collect();
        let class = |key: NodeId| match source.node(key).map(|node| &node.kind) {
            Some(NodeKind::Scalar(_)) => 0u8,
            Some(NodeKind::Sequence(_)) => 1,
            Some(NodeKind::Mapping(_)) => 2,
            Some(NodeKind::Alias(_)) | None => 3,
        };
        let mut groups: BTreeMap<(u8, &str), Vec<usize>> = BTreeMap::new();
        for (i, &key) in keys.iter().enumerate() {
            groups
                .entry((class(key), source.go_value(key)))
                .or_default()
                .push(i);
        }
        if groups.len() == keys.len() {
            return true;
        }
        // go-yaml compares each key with every later key, in order.
        let mut reported = 0;
        for (i, &key) in keys.iter().enumerate() {
            let Some(group) = groups.get(&(class(key), source.go_value(key))) else {
                continue;
            };
            for &j in group.iter().filter(|&&j| j > i) {
                if reported == MAX_DUPLICATE_KEY_ERRORS {
                    return false;
                }
                let Some(&later) = keys.get(j) else { continue };
                let message = format!(
                    "mapping key {} already defined at line {}",
                    quote(source.go_value(later)),
                    self.line(key)
                );
                self.error(later, message);
                reported += 1;
            }
        }
        false
    }

    /// Decodes `id` into a struct with the fields `fields`, named by their
    /// YAML keys. `decode` decodes the value of the field with the given
    /// index. `go_type` names the struct in messages.
    pub(crate) fn structure(
        &mut self,
        id: NodeId,
        go_type: &str,
        fields: &[&str],
        decode: &mut dyn FnMut(&mut Self, usize, NodeId) -> Result<(), Fatal>,
    ) -> Result<bool, Fatal> {
        self.unmarshal(id, |d, id, node| match &node.kind {
            NodeKind::Mapping(_) => {
                if !d.unique_keys(id) {
                    return Ok(false);
                }
                d.struct_mappings(id, go_type, fields, decode)?;
                Ok(true)
            }
            NodeKind::Scalar(scalar) => d.scalar_mismatch(id, scalar, go_type),
            _ => {
                d.type_error(id, go_type);
                Ok(false)
            }
        })
    }

    /// Decodes the mapping `id` into a struct, and then each mapping that
    /// its `<<` keys merge into the same struct, as go-yaml's
    /// `mappingStruct` and `merge` do. go-yaml recurses for each nested
    /// `<<`. This loops over a stack of the work left instead, so that
    /// merges nested as deep as the YAML layer allows (ADR 0022) cannot
    /// overflow the stack.
    fn struct_mappings(
        &mut self,
        id: NodeId,
        go_type: &str,
        fields: &[&str],
        decode: &mut dyn FnMut(&mut Self, usize, NodeId) -> Result<(), Fatal>,
    ) -> Result<(), Fatal> {
        let mut work = Vec::new();
        self.struct_entries(id, go_type, fields, decode, &mut work)?;
        while let Some(next) = work.pop() {
            match next {
                StructWork::Merge { parent, value } => {
                    let outermost = self.merged.is_none();
                    if outermost {
                        self.merged = Some(self.merge_keys(parent)?);
                    }
                    work.push(StructWork::EndMerge { outermost });
                    let items = self.merge_items(value)?;
                    work.extend(items.into_iter().rev().map(StructWork::Mapping));
                }
                StructWork::Mapping(item) => {
                    if !self.source.is_mapping(item) {
                        return Err(want_map());
                    }
                    let (item, _, aliases) = self.enter_node(item)?;
                    work.push(StructWork::LeaveAliases(aliases));
                    if self.unique_keys(item) {
                        self.struct_entries(item, go_type, fields, decode, &mut work)?;
                    }
                }
                StructWork::LeaveAliases(count) => self.leave_aliases(count),
                StructWork::EndMerge { outermost } => {
                    if outermost {
                        self.merged = None;
                    }
                }
            }
        }
        Ok(())
    }

    /// Decodes the entries of the mapping `id` into a struct, as go-yaml's
    /// `mappingStruct` does, and pushes the merge of its `<<` key, if it has
    /// one, to `work`.
    fn struct_entries(
        &mut self,
        id: NodeId,
        go_type: &str,
        fields: &[&str],
        decode: &mut dyn FnMut(&mut Self, usize, NodeId) -> Result<(), Fatal>,
        work: &mut Vec<StructWork>,
    ) -> Result<(), Fatal> {
        let mut merged = self.merged.take();
        let mut merge = None;
        let mut done = vec![false; fields.len()];
        let source = self.source;
        for entry in source.written_entries(id) {
            if source.is_merge_key(entry.key) {
                merge = Some(entry.value);
                continue;
            }
            let mut name = String::new();
            if !self.string(entry.key, &mut name)? {
                continue;
            }
            if let Some(seen) = &mut merged
                && !seen.insert(Key::Str(name.clone()))
            {
                continue;
            }
            let Some(field) = fields.iter().position(|field| *field == name) else {
                continue;
            };
            if let Some(done) = done.get_mut(field) {
                if *done {
                    self.error(
                        entry.key,
                        format!("field {name} already set in type {go_type}"),
                    );
                    continue;
                }
                *done = true;
            }
            decode(self, field, entry.value)?;
        }
        self.merged = merged;
        if let Some(value) = merge {
            work.push(StructWork::Merge { parent: id, value });
        }
        Ok(())
    }

    /// Returns the keys of the mapping `parent`, which the mappings that its
    /// `<<` key merges in do not override, as go-yaml's `merge` collects
    /// them.
    fn merge_keys(&mut self, parent: NodeId) -> Result<BTreeSet<Key>, Fatal> {
        let mut seen = BTreeSet::new();
        let source = self.source;
        for entry in source.written_entries(parent) {
            let key = self.any(entry.key, true)?;
            add_merged_key(&mut seen, key)?;
        }
        Ok(seen)
    }

    /// Returns the mappings that the `<<` value `merge` names: itself if it
    /// is a mapping or an alias, or else its items if it is a sequence. The
    /// caller checks that each one is a mapping just before it merges it,
    /// as go-yaml does.
    fn merge_items(&self, merge: NodeId) -> Result<Vec<NodeId>, Fatal> {
        match self.source.node(merge).map(|node| &node.kind) {
            Some(NodeKind::Mapping(_) | NodeKind::Alias(_)) => Ok(vec![merge]),
            Some(NodeKind::Sequence(_)) => Ok(self.source.written(merge).to_vec()),
            _ => Err(want_map()),
        }
    }
}

/// The work left in [`Decoder::struct_mappings`], last first.
enum StructWork {
    /// Merge the `<<` value `value` of the mapping `parent`.
    Merge { parent: NodeId, value: NodeId },
    /// Check that a node that a `<<` key merges is a mapping, and decode it.
    Mapping(NodeId),
    /// Leave the aliases that the decoding of a merged mapping entered.
    LeaveAliases(u32),
    /// End a merge. The outermost merge forgets the keys already set.
    EndMerge { outermost: bool },
}

fn want_map() -> Fatal {
    Fatal::new("yaml: map merge requires map or sequence of maps as the value".to_owned())
}

/// Adds the map key `key`, or nil if it is `None`, to the keys that a merge
/// does not override.
fn add_merged_key(seen: &mut BTreeSet<Key>, key: Option<GoValue>) -> Result<(), Fatal> {
    match key.unwrap_or(GoValue::Scalar(Resolved::Null)).key() {
        Ok(key) => {
            seen.insert(key);
            Ok(())
        }
        Err(KeyError::NaN) => Ok(()),
        Err(KeyError::Unhashable(go_type)) => Err(unhashable(go_type)),
    }
}

fn unhashable(go_type: &str) -> Fatal {
    Fatal::new(format!(
        "yaml: runtime error: hash of unhashable type {go_type}"
    ))
}

/// Sets `key` to `value` in a map's entries, as Go's map assignment does.
fn insert(entries: &mut Vec<(GoValue, GoValue)>, key: GoValue, value: GoValue) {
    if let Ok(wanted) = key.key()
        && let Some(entry) = entries
            .iter_mut()
            .find(|(existing, _)| existing.key().ok().as_ref() == Some(&wanted))
    {
        entry.1 = value;
        return;
    }
    entries.push((key, value));
}
