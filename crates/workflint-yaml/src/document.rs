//! The span tree: one arena of nodes per document.

use std::borrow::Cow;
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
    pub(crate) start: u32,
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

    /// Returns the byte offset where go-yaml reports the document node: the
    /// start of the first directive or of the `---` marker, or else of the
    /// root node, properties included. It is 0 for a file with no document.
    #[must_use]
    pub fn start(&self) -> u32 {
        self.start
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

    /// Returns the items of the sequence `sequence` as written, with each
    /// resolved alias kept as its alias node.
    ///
    /// Comments on an alias are attached to the alias node, which
    /// [`items`](Self::items) replaces, so a walker reaches them through
    /// this list. It is empty if `sequence` is not a sequence. The walk
    /// takes time in proportion to the number of nodes written inside the
    /// sequence.
    #[must_use]
    pub fn written_items(&self, sequence: NodeId) -> Vec<NodeId> {
        match self.node(sequence).map(|node| &node.kind) {
            Some(NodeKind::Sequence(_)) => self.written_children(sequence),
            _ => Vec::new(),
        }
    }

    /// Returns the entries of the mapping `mapping` as written, with each
    /// resolved alias kept as its alias node.
    ///
    /// Comments on an alias are attached to the alias node, which
    /// [`entries`](Self::entries) replaces, so a walker reaches them
    /// through this list. It is empty if `mapping` is not a mapping. The
    /// walk takes time in proportion to the number of nodes written inside
    /// the mapping.
    #[must_use]
    pub fn written_entries(&self, mapping: NodeId) -> Vec<Entry> {
        match self.node(mapping).map(|node| &node.kind) {
            Some(NodeKind::Mapping(_)) => {
                let children = self.written_children(mapping);
                let (pairs, _) = children.as_chunks::<2>();
                pairs
                    .iter()
                    .map(|&[key, value]| Entry { key, value })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Returns the children of the collection `id` as written, keys and
    /// values in turn for a mapping.
    ///
    /// IDs follow the pre-order of the tree as written, where an alias has
    /// no children. So the first child comes right after the collection,
    /// and each later child comes right after the nodes written inside the
    /// child before it.
    fn written_children(&self, id: NodeId) -> Vec<NodeId> {
        let Some(node) = self.node(id) else {
            return Vec::new();
        };
        let count = written_len(node);
        let mut children = Vec::with_capacity(count);
        // The last node visited.
        let mut last = id.0;
        // The nodes still to visit in the current child's subtree.
        let mut pending = 0usize;
        while children.len() < count {
            let Some((next, node)) = last
                .checked_add(1)
                .and_then(|next| Some((next, self.node(NodeId(next))?)))
            else {
                break;
            };
            if pending == 0 {
                children.push(NodeId(next));
                pending = 1;
            }
            last = next;
            pending = pending - 1 + written_len(node);
        }
        children
    }

    /// Returns every comment in the document in source order.
    #[must_use]
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// Returns the comments attached to the node `id` in source order.
    ///
    /// [`Comment::node`] says which node a comment is attached to. A comment
    /// on an alias is attached to the alias node, not to the anchored node,
    /// because several aliases can refer to the anchored node.
    /// [`written_items`](Self::written_items) and
    /// [`written_entries`](Self::written_entries) return the alias nodes.
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
    /// scalar keys are compared, by their decoded values. A `<<` key is
    /// never a duplicate, as in actionlint, which reports merge keys on
    /// their own. The result is in source order of the duplicates.
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
            if key.value == "<<" {
                continue;
            }
            let key = match comparison {
                KeyComparison::Exact => key.value.clone(),
                KeyComparison::CaseInsensitive => simple_lowercase(&key.value).into_owned(),
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

/// Returns the number of children that `node` has as written: none for an
/// alias or a scalar, and a key and a value for each mapping entry.
fn written_len(node: &Node) -> usize {
    let len = |range: &Range<u32>| usize::try_from(range.end.saturating_sub(range.start));
    match &node.kind {
        NodeKind::Sequence(sequence) => len(&sequence.items).unwrap_or(0),
        NodeKind::Mapping(mapping) => len(&mapping.entries).map_or(0, |n| n.saturating_mul(2)),
        NodeKind::Scalar(_) | NodeKind::Alias(_) => 0,
    }
}

/// Lowercases each character of `text` with its Unicode simple case
/// mapping, as Go's `strings.ToLower` does.
///
/// The simple mapping takes one character to one character. Rust's
/// lowercase differs from it only for U+0130, whose lowercase adds a
/// combining dot after the `i`. [`KeyComparison::CaseInsensitive`] says
/// where the Unicode versions of Rust and Go differ. The result borrows
/// `text` when no character changes.
///
/// ```
/// use workflint_yaml::simple_lowercase;
///
/// assert_eq!(simple_lowercase("GITHUB_TOKEN"), "github_token");
/// assert_eq!(simple_lowercase("\u{130}"), "i");
/// ```
#[must_use]
pub fn simple_lowercase(text: &str) -> Cow<'_, str> {
    let lower = |ch: char| {
        let mut lower = ch.to_lowercase();
        match (lower.next(), lower.next()) {
            (Some(lower), None) => lower,
            _ if ch == '\u{130}' => 'i',
            _ => ch,
        }
    };
    if text.chars().all(|ch| lower(ch) == ch) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.chars().map(lower).collect())
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
    /// Keys are equal if they are equal after lowercasing each character
    /// with its Unicode simple case mapping, as actionlint does with Go's
    /// `strings.ToLower`.
    ///
    /// The mapping comes from the Unicode version of the Rust toolchain.
    /// Rust 1.99 has Unicode 17 and Go 1.26 has Unicode 15, so the 55
    /// letters with a lowercase added in Unicode 16 and 17 are lowercased
    /// here and kept by actionlint.
    CaseInsensitive,
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
    /// Returns the tag that the YAML 1.2 core schema resolves for an
    /// untagged plain scalar whose decoded value is `value`.
    ///
    /// ```
    /// use workflint_yaml::Tag;
    ///
    /// assert_eq!(Tag::resolve_plain("1.5"), Tag::Float);
    /// assert_eq!(Tag::resolve_plain("yes"), Tag::Str);
    /// ```
    #[must_use]
    pub fn resolve_plain(value: &str) -> Self {
        crate::resolve::plain(value)
    }

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
    /// next line that is not an own-line comment: the first mapping key or
    /// sequence item that starts there, or else the first node that starts
    /// there. If no node starts on that line, as when the line is empty or
    /// holds only a closing bracket, the comment is attached to no node.
    ///
    /// A trailing comment that only `-` and `?` indicators precede on its
    /// line, as in `- # c`, is attached to the node that follows the last
    /// indicator. If that node starts on a later line, the comment is
    /// attached as if it were an own-line comment before that line. So in
    /// `- # c` followed by `  run: a`, the comment is attached to the
    /// mapping, which is the sequence item, and not to the sequence.
    ///
    /// A comment on an alias is attached to the alias node. See
    /// [`Document::node_comments`].
    pub node: Option<NodeId>,
}
