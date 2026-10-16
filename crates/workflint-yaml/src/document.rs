//! The span tree: one arena of nodes per document.

use std::collections::BTreeMap;
use std::ops::Range;

use workflint_diagnostics::{FileId, Span};

use crate::CommentKind;

/// Identifies one node in a [`Document`].
///
/// IDs follow the order in which the nodes start in the source, so a parent
/// has a lower ID than its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// Returns the position of the node in [`Document::nodes`].
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// The tree of the first YAML document in a file.
///
/// The nodes live in one arena and refer to each other by [`NodeId`]. After
/// alias resolution the tree is a directed acyclic graph: every resolved
/// alias is replaced by its anchored node in the parent's items or entries,
/// so the same subtree is reachable from several parents without being
/// copied.
#[derive(Debug, Clone)]
pub struct Document {
    pub(crate) file: FileId,
    pub(crate) root: Option<NodeId>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) items: Vec<NodeId>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) comments: Vec<Comment>,
    /// Indexes into `comments`, grouped by node. [`Node::comments`] is a
    /// range of this list.
    pub(crate) node_comments: Vec<u32>,
}

impl Document {
    /// Returns the file the document was parsed from.
    #[must_use]
    pub fn file(&self) -> FileId {
        self.file
    }

    /// Returns the root node, or `None` for an empty stream or a file with
    /// only comments.
    #[must_use]
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Returns the node with the ID `id`, or `None` if the ID comes from
    /// another document.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(usize::try_from(id.0).ok()?)
    }

    /// Returns every node in the order the nodes start in the source,
    /// including aliases that resolution replaced.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        (0u32..).zip(&self.nodes).map(|(i, node)| (NodeId(i), node))
    }

    /// Returns the items of a sequence, with resolved aliases replaced by
    /// their anchored nodes.
    #[must_use]
    pub fn items(&self, sequence: &Sequence) -> &[NodeId] {
        slice(&self.items, &sequence.items)
    }

    /// Returns the entries of a mapping in source order, duplicates included,
    /// with resolved aliases replaced by their anchored nodes.
    #[must_use]
    pub fn entries(&self, mapping: &Mapping) -> &[Entry] {
        slice(&self.entries, &mapping.entries)
    }

    /// Returns every comment in the document in source order.
    #[must_use]
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// Returns the comments attached to the node `id` in source order.
    pub fn node_comments(&self, id: NodeId) -> impl Iterator<Item = &Comment> {
        let range = self.node(id).map_or(0..0, |node| node.comments.clone());
        slice(&self.node_comments, &range)
            .iter()
            .filter_map(|&i| self.comments.get(usize::try_from(i).ok()?))
    }

    /// Lists the entries of `mapping` whose scalar key equals the key of an
    /// earlier entry under `comparison`.
    ///
    /// Each duplicate is paired with the first entry that has its key. Only
    /// scalar keys are compared, by their decoded values. The result is in
    /// source order of the duplicates.
    #[must_use]
    pub fn duplicate_keys(
        &self,
        mapping: &Mapping,
        comparison: KeyComparison,
    ) -> Vec<DuplicateKey> {
        let mut first: BTreeMap<String, Entry> = BTreeMap::new();
        let mut duplicates = Vec::new();
        for &entry in self.entries(mapping) {
            let Some(Node {
                kind: NodeKind::Scalar(key),
                ..
            }) = self.node(entry.key)
            else {
                continue;
            };
            let key = match comparison {
                KeyComparison::Exact => key.value.clone(),
                KeyComparison::AsciiCaseInsensitive => key.value.to_ascii_lowercase(),
            };
            match first.get(&key) {
                Some(&first) => duplicates.push(DuplicateKey {
                    first,
                    duplicate: entry,
                }),
                None => {
                    first.insert(key, entry);
                }
            }
        }
        duplicates
    }
}

fn slice<'a, T>(items: &'a [T], range: &Range<u32>) -> &'a [T] {
    let (Ok(start), Ok(end)) = (usize::try_from(range.start), usize::try_from(range.end)) else {
        return &[];
    };
    items.get(start..end).unwrap_or_default()
}

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// From the start of the node's properties (its anchor or tag), or of
    /// its content if it has none, to the end of the node.
    ///
    /// The start is the position go-yaml reports for the node: the `&` or
    /// `!` of the first property, the opening quote of a quoted scalar, the
    /// `|` or `>` of a block scalar, the first key of a block mapping, the
    /// first `-` of a block sequence, and the `[` or `{` of a flow
    /// collection. An implicit null, as in `key:`, starts and ends right
    /// after the colon.
    ///
    /// A block collection ends where its last child ends. A block scalar ends
    /// where libyaml's mark ends it: after its last line break, including
    /// trailing empty lines.
    pub span: Span,
    /// The byte offset where the content starts.
    ///
    /// It differs from `span.start` for a node with an anchor or tag, where
    /// it is the start of the content after the properties, and for a block
    /// scalar, where it is the start of the line after the header.
    pub content_start: u32,
    /// The anchor, if the node has one. Aliases have none.
    pub anchor: Option<Anchor>,
    /// The kind of the node and its data.
    pub kind: NodeKind,
    pub(crate) comments: Range<u32>,
}

impl Node {
    /// Returns the tag of the node, or `None` for an alias.
    #[must_use]
    pub fn tag(&self) -> Option<&Tag> {
        match &self.kind {
            NodeKind::Scalar(scalar) => Some(&scalar.tag),
            NodeKind::Sequence(sequence) => Some(&sequence.tag),
            NodeKind::Mapping(mapping) => Some(&mapping.tag),
            NodeKind::Alias(_) => None,
        }
    }
}

/// The kind of a [`Node`] and its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A scalar.
    Scalar(Scalar),
    /// A sequence.
    Sequence(Sequence),
    /// A mapping.
    Mapping(Mapping),
    /// An alias, such as `*name`.
    Alias(Alias),
}

/// The data of a scalar node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scalar {
    /// The decoded value: unquoted, unescaped, folded, and chomped.
    pub value: String,
    /// How the scalar is written.
    pub style: ScalarStyle,
    /// The explicit tag, or the tag that the YAML 1.2 core schema resolves
    /// for a plain scalar. Quoted and block scalars without a tag are
    /// `!!str`.
    pub tag: Tag,
    /// The column of the innermost enclosing block collection, or -1 at the
    /// top level. This is the scanner's indentation when it reads a block
    /// scalar, which the content indentation depends on.
    pub(crate) parent_indent: i64,
}

/// The data of a sequence node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    /// Whether the sequence is a block or a flow sequence.
    pub style: CollectionStyle,
    /// The explicit tag, or `!!seq`.
    pub tag: Tag,
    pub(crate) items: Range<u32>,
}

/// The data of a mapping node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// Whether the mapping is a block or a flow mapping.
    pub style: CollectionStyle,
    /// The explicit tag, or `!!map`.
    pub tag: Tag,
    pub(crate) entries: Range<u32>,
}

/// The data of an alias node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// The anchor name, without the `*`.
    pub name: String,
    /// The anchored node, or `None` if the alias is recursive.
    ///
    /// A resolved alias is replaced by this node in its parent's items or
    /// entries. A recursive alias, which refers to a node that encloses it,
    /// stays in its parent as an alias node.
    pub target: Option<NodeId>,
}

/// One key and value of a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The key node. Its span is the key's span.
    pub key: NodeId,
    /// The value node.
    pub value: NodeId,
}

/// An entry whose key repeats the key of an earlier entry in its mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateKey {
    /// The first entry with the key.
    pub first: Entry,
    /// A later entry with the same key.
    pub duplicate: Entry,
}

/// How [`Document::duplicate_keys`] compares keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyComparison {
    /// Keys must be equal byte for byte.
    Exact,
    /// Keys are equal if they differ only in the case of ASCII letters.
    AsciiCaseInsensitive,
}

/// A node's anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// The anchor name, without the `&`.
    pub name: String,
    /// The span of the `&name` token.
    pub span: Span,
}

/// A node's tag as messages print it.
///
/// Tags in the `tag:yaml.org,2002:` namespace are shortened to `!!`, as
/// go-yaml does, so `!!int` and `!<tag:yaml.org,2002:int>` are both
/// [`Tag::Int`]. Other tags are kept as the parser expands them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tag {
    /// `!!str`.
    Str,
    /// `!!int`.
    Int,
    /// `!!float`.
    Float,
    /// `!!bool`.
    Bool,
    /// `!!null`.
    Null,
    /// `!!map`.
    Map,
    /// `!!seq`.
    Seq,
    /// Any other tag, such as `!!binary` or `!custom`.
    Other(Box<str>),
}

impl Tag {
    /// Returns the tag as messages print it, such as `!!str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Str => "!!str",
            Self::Int => "!!int",
            Self::Float => "!!float",
            Self::Bool => "!!bool",
            Self::Null => "!!null",
            Self::Map => "!!map",
            Self::Seq => "!!seq",
            Self::Other(tag) => tag,
        }
    }
}

/// How a scalar is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarStyle {
    /// A plain scalar, without quotes.
    Plain,
    /// A `'single-quoted'` scalar.
    SingleQuoted,
    /// A `"double-quoted"` scalar.
    DoubleQuoted,
    /// A literal block scalar, `|`.
    Literal(BlockHeader),
    /// A folded block scalar, `>`.
    Folded(BlockHeader),
}

/// The indicators after the `|` or `>` of a block scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockHeader {
    /// How the final line breaks are kept.
    pub chomping: Chomping,
    /// The indentation indicator, 1 to 9, if the header has one.
    pub indent: Option<u8>,
}

/// The chomping indicator of a block scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chomping {
    /// No indicator: keep one final line break.
    Clip,
    /// `-`: remove the final line breaks.
    Strip,
    /// `+`: keep every final line break.
    Keep,
}

/// Whether a collection is written in block or flow style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollectionStyle {
    /// Indentation, `-` items, and `key: value` lines.
    Block,
    /// `[a, b]` or `{a: b}`.
    Flow,
}

/// A comment, from its `#` to the end of its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    /// Whether code precedes the comment on its line.
    pub kind: CommentKind,
    /// The span of the comment without its line break.
    pub span: Span,
    /// The node the comment is attached to (ADR 0009), if any.
    ///
    /// A trailing comment is attached to the last node that starts before
    /// it on its line. An own-line comment is attached to a node on the
    /// line where the next node starts: the first mapping key or sequence
    /// item that starts there, or else the first node that starts there.
    pub node: Option<NodeId>,
}
