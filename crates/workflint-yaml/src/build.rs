//! Builds the span tree from the event stream.

use std::collections::BTreeMap;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Diagnostic, FileId, SourceFile, Span};

use crate::attach::{CommentPlace, Role, after_indicators, attach};
use crate::document::{
    Alias, Anchor, BlockHeader, Chomping, CollectionStyle, Comment, Document, Entry, Mapping, Node,
    NodeId, NodeKind, Scalar, ScalarStyle, Sequence, Tag,
};
use crate::help::syntax_error_help;
use crate::libyaml::{
    self, Error, Event, EventData, MappingStyle, Mark, NodeMarks, Parser, SequenceStyle,
};
use crate::resolve;
use crate::rules::{RECURSIVE_ALIAS, UNKNOWN_ANCHOR, UNUSED_ANCHOR, YAML_SYNTAX_ERROR};

/// The context of the scanner errors for a quoted scalar without its closing
/// quote.
const UNCLOSED_QUOTED_SCALAR: &str = "while scanning a quoted scalar";

/// The result of [`parse`].
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The tree of the first document, or `None` if the file has a YAML
    /// syntax error or an alias to an unknown anchor.
    pub document: Option<Document>,
    /// The YAML-level diagnostics, in no particular order. Sort them with
    /// [`sort_and_dedup`](workflint_diagnostics::sort_and_dedup).
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses the first YAML document of `file` into a span tree.
///
/// A YAML syntax error or an alias to an unknown anchor gives exactly one
/// diagnostic and no tree, as in actionlint. Otherwise the tree comes with a
/// diagnostic for each recursive alias and each unused anchor. Later
/// documents are not parsed, as in go-yaml. go-yaml's scanner reads a few
/// tokens past the end of the first document, so a syntax error in those
/// tokens is reported, as in `a: 1\n--- @\n`, and one further on is not.
#[must_use]
pub fn parse(file: &SourceFile) -> Parsed {
    match Builder::new(file).run() {
        Ok((document, diagnostics)) => Parsed {
            document: Some(document),
            diagnostics,
        },
        Err(diagnostic) => Parsed {
            document: None,
            diagnostics: vec![diagnostic],
        },
    }
}

/// The marks of an event, kept apart from its data.
#[derive(Clone, Copy)]
struct EventMarks {
    start_mark: Mark,
    end_mark: Mark,
    node_marks: NodeMarks,
}

/// A collection whose end event has not been seen yet.
struct Frame {
    node: NodeId,
    mapping: bool,
    block: bool,
    /// The scanner's indentation inside the innermost block collection that
    /// is open at this frame, this one included, or -1 if there is none.
    /// It is computed when the frame is pushed, so a scalar finds it without
    /// walking the frames.
    block_indent: i64,
    /// Where the collection's children start in `Builder::scratch`.
    scratch_start: usize,
    /// The end of the last child as written, for the end of a block
    /// collection.
    last_end: Option<u32>,
}

struct Builder<'a> {
    file: &'a SourceFile,
    id: FileId,
    text: &'a str,
    nodes: Vec<Node>,
    /// The 0-based line where each node starts.
    lines: Vec<u64>,
    roles: Vec<Role>,
    /// Whether each node's end event has been seen.
    closed: Vec<bool>,
    /// Whether an alias refers to each node.
    used: Vec<bool>,
    /// The latest node with each anchor name.
    anchors: BTreeMap<String, NodeId>,
    frames: Vec<Frame>,
    /// The children of the open collections, innermost last.
    /// The children of the open collections: what each parent refers to,
    /// and the node as written.
    scratch: Vec<(NodeId, NodeId)>,
    items: Vec<NodeId>,
    entries: Vec<Entry>,
    written_items: Vec<NodeId>,
    written_entries: Vec<Entry>,
    comments: Vec<Comment>,
    comment_places: Vec<CommentPlace>,
    root: Option<NodeId>,
    /// The offset of the first document's start event.
    start: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Builder<'a> {
    fn new(file: &'a SourceFile) -> Self {
        Self {
            file,
            id: file.id(),
            text: file.text(),
            nodes: Vec::new(),
            lines: Vec::new(),
            roles: Vec::new(),
            closed: Vec::new(),
            used: Vec::new(),
            anchors: BTreeMap::new(),
            frames: Vec::new(),
            scratch: Vec::new(),
            items: Vec::new(),
            entries: Vec::new(),
            written_items: Vec::new(),
            written_entries: Vec::new(),
            comments: Vec::new(),
            comment_places: Vec::new(),
            root: None,
            start: 0,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> Result<(Document, Vec<Diagnostic>), Diagnostic> {
        // go-yaml stops reading events after the end of the first document.
        // The scanner's lookahead raises an error in the tokens right after
        // it before that end event, so the error is reported. Errors that
        // come later are not. The loop reads on only to collect the comments
        // right after the first document.
        let mut first_document_done = false;
        for event in Parser::new(self.text) {
            let event = match event {
                Ok(event) => event,
                Err(_) if first_document_done => break,
                Err(err) => return Err(self.syntax_error(&err)),
            };
            let Event {
                data,
                start_mark,
                end_mark,
                node_marks,
            } = event;
            let event = &EventMarks {
                start_mark,
                end_mark,
                node_marks,
            };
            if let EventData::Comment { kind } = data {
                self.comment(kind, event);
                continue;
            }
            if first_document_done {
                break;
            }
            match data {
                EventData::Scalar {
                    anchor,
                    tag,
                    value,
                    style,
                    ..
                } => self.scalar(event, anchor, tag.as_deref(), value, style),
                EventData::SequenceStart {
                    anchor, tag, style, ..
                } => self.open(
                    event,
                    anchor,
                    tag.as_deref(),
                    false,
                    style == SequenceStyle::Block,
                ),
                EventData::MappingStart {
                    anchor, tag, style, ..
                } => self.open(
                    event,
                    anchor,
                    tag.as_deref(),
                    true,
                    style == MappingStyle::Block,
                ),
                EventData::SequenceEnd | EventData::MappingEnd => self.close(event.end_mark),
                EventData::Alias { anchor } => self.alias(event, anchor)?,
                EventData::DocumentStart { .. } => self.start = Self::offset(event.start_mark),
                EventData::DocumentEnd { .. } => first_document_done = true,
                EventData::StreamEnd => break,
                EventData::StreamStart { .. } | EventData::Comment { .. } => {}
            }
        }
        Ok(self.finish())
    }

    fn offset(mark: Mark) -> u32 {
        u32::try_from(mark.index).unwrap_or(u32::MAX)
    }

    fn span(&self, start: u32, end: u32) -> Span {
        Span::new(self.id, start, end)
    }

    /// The diagnostic for a syntax error, at the problem position. An
    /// unclosed quoted scalar is reported at its opening quote, the context
    /// mark, as in go-yaml, because its problem position is the end of the
    /// stream or the next document marker.
    fn syntax_error(&self, err: &Error) -> Diagnostic {
        let len = self.text.len();
        let position = match (err.context(), err.context_mark()) {
            (Some(UNCLOSED_QUOTED_SCALAR), Some(mark)) => mark.index,
            _ => err.problem_offset(),
        };
        let offset = usize::try_from(position).unwrap_or(len).min(len);
        let offset = self.text.floor_char_boundary(offset);
        let end = self
            .text
            .get(offset..)
            .and_then(|rest| rest.chars().next())
            .map_or(offset, |ch| offset + ch.len_utf8());
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Diagnostic::new(
            &YAML_SYNTAX_ERROR,
            self.span(to_u32(offset), to_u32(end)),
            format!("could not parse as YAML: {}", err.problem()),
        )
        .with_help(syntax_error_help(self.text, err))
    }

    fn comment(&mut self, kind: libyaml::CommentKind, event: &EventMarks) {
        let start = Self::offset(event.start_mark);
        let after_indicators = match kind {
            libyaml::CommentKind::Trailing => after_indicators(self.text, start),
            libyaml::CommentKind::OwnLine => None,
        };
        self.comments.push(Comment {
            kind,
            span: self.span(start, Self::offset(event.end_mark)),
            node: None,
        });
        self.comment_places.push(CommentPlace {
            line: event.start_mark.line,
            after_indicators,
        });
    }

    /// Adds a node to the arena and to the innermost open collection, or
    /// makes it the root. `child` is what the parent refers to, which differs
    /// from the new node for a resolved alias.
    fn add(&mut self, node: Node, line: u64, child: Option<NodeId>) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        let role = match self.frames.last() {
            None => Role::Root,
            Some(frame) if frame.mapping => {
                if (self.scratch.len() - frame.scratch_start).is_multiple_of(2) {
                    Role::Key
                } else {
                    Role::Value
                }
            }
            Some(_) => Role::Item,
        };
        if let Some(anchor) = &node.anchor {
            self.anchors.insert(anchor.name.clone(), id);
        }
        self.nodes.push(node);
        self.lines.push(line);
        self.roles.push(role);
        self.closed.push(false);
        self.used.push(false);
        let child = child.unwrap_or(id);
        if self.frames.is_empty() {
            self.root = Some(child);
        } else {
            self.scratch.push((child, id));
        }
        id
    }

    /// The scanner's indentation inside the innermost open block collection,
    /// or -1 outside every block collection.
    fn block_indent(&self) -> i64 {
        self.frames.last().map_or(-1, |frame| frame.block_indent)
    }

    /// Records that a child of the innermost open collection ended at `end`.
    fn child_ended(&mut self, end: u32) {
        if let Some(frame) = self.frames.last_mut() {
            frame.last_end = Some(end);
        }
    }

    fn set_closed(&mut self, id: NodeId) {
        if let Some(closed) = usize::try_from(id.0)
            .ok()
            .and_then(|i| self.closed.get_mut(i))
        {
            *closed = true;
        }
    }

    fn anchor(&self, name: Option<String>, event: &EventMarks, start: u32) -> Option<Box<Anchor>> {
        let name = name?;
        let span = match event.node_marks.anchor {
            Some((anchor_start, anchor_end)) => {
                self.span(Self::offset(anchor_start), Self::offset(anchor_end))
            }
            None => self.span(start, start),
        };
        Some(Box::new(Anchor { name, span }))
    }

    fn scalar(
        &mut self,
        event: &EventMarks,
        anchor: Option<String>,
        tag: Option<&str>,
        value: String,
        style: libyaml::ScalarStyle,
    ) {
        let start = Self::offset(event.start_mark);
        let end = Self::offset(event.end_mark);
        let content = event.node_marks.content.map_or(start, Self::offset);
        let (style, content_start) = match style {
            libyaml::ScalarStyle::Plain => (ScalarStyle::Plain, content),
            libyaml::ScalarStyle::SingleQuoted => (ScalarStyle::SingleQuoted, content),
            libyaml::ScalarStyle::DoubleQuoted => (ScalarStyle::DoubleQuoted, content),
            libyaml::ScalarStyle::Literal => {
                let (header, body) = self.block_header(content, end);
                (ScalarStyle::Literal(header), body)
            }
            libyaml::ScalarStyle::Folded => {
                let (header, body) = self.block_header(content, end);
                (ScalarStyle::Folded(header), body)
            }
        };
        let tag = resolve::explicit(tag).unwrap_or_else(|| match style {
            ScalarStyle::Plain => resolve::plain(&value),
            _ => Tag::Str,
        });
        let parent_indent_plus_one =
            u32::try_from(self.block_indent().saturating_add(1)).unwrap_or(u32::MAX);
        let node = Node {
            span: self.span(start, end),
            content_start,
            anchor: self.anchor(anchor, event, start),
            kind: NodeKind::Scalar(Scalar {
                value: value.into_boxed_str(),
                style,
                tag,
                parent_indent_plus_one,
            }),
        };
        let id = self.add(node, event.start_mark.line, None);
        self.set_closed(id);
        self.child_ended(end);
    }

    /// Reads the indicators of the block scalar header at `at`, the `|` or
    /// `>`, and returns them with the start of the line after the header.
    fn block_header(&self, at: u32, end: u32) -> (BlockHeader, u32) {
        let mut header = BlockHeader {
            chomping: Chomping::Clip,
            indent: None,
        };
        let at = usize::try_from(at).unwrap_or(usize::MAX);
        let Some(rest) = self.text.get(at..) else {
            return (header, end);
        };
        for ch in rest.chars().skip(1).take(2) {
            match ch {
                '-' => header.chomping = Chomping::Strip,
                '+' => header.chomping = Chomping::Keep,
                '1'..='9' => header.indent = u8::try_from(u32::from(ch) - u32::from('0')).ok(),
                _ => break,
            }
        }
        let body = rest
            .char_indices()
            .find(|&(_, ch)| matches!(ch, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            .map(|(i, ch)| {
                let crlf = ch == '\r' && rest.as_bytes().get(i + 1) == Some(&b'\n');
                at + i + if crlf { 2 } else { ch.len_utf8() }
            });
        let body = body
            .and_then(|body| u32::try_from(body).ok())
            .map_or(end, |body| body.min(end));
        (header, body)
    }

    fn open(
        &mut self,
        event: &EventMarks,
        anchor: Option<String>,
        tag: Option<&str>,
        mapping: bool,
        block: bool,
    ) {
        let start = Self::offset(event.start_mark);
        let style = if block {
            CollectionStyle::Block
        } else {
            CollectionStyle::Flow
        };
        let kind = if mapping {
            NodeKind::Mapping(Mapping {
                style,
                tag: resolve::explicit(tag).unwrap_or(Tag::Map),
                entries: 0..0,
            })
        } else {
            NodeKind::Sequence(Sequence {
                style,
                tag: resolve::explicit(tag).unwrap_or(Tag::Seq),
                items: 0..0,
            })
        };
        let content = event.node_marks.content.unwrap_or(event.start_mark);
        let node = Node {
            span: self.span(start, Self::offset(event.end_mark)),
            content_start: Self::offset(content),
            anchor: self.anchor(anchor, event, start),
            kind,
        };
        let id = self.add(node, event.start_mark.line, None);
        // The column where a block collection's content starts is the
        // scanner's indentation inside it.
        let block_indent = if block {
            i64::try_from(content.column).unwrap_or(i64::MAX)
        } else {
            self.block_indent()
        };
        self.frames.push(Frame {
            node: id,
            mapping,
            block,
            block_indent,
            scratch_start: self.scratch.len(),
            last_end: None,
        });
    }

    fn close(&mut self, end_mark: Mark) {
        let Some(frame) = self.frames.pop() else {
            return;
        };
        let scratch_start = frame.scratch_start.min(self.scratch.len());
        let end = match frame.last_end {
            Some(last_end) if frame.block => last_end,
            _ => Self::offset(end_mark),
        };
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let Some(node) = usize::try_from(frame.node.0)
            .ok()
            .and_then(|i| self.nodes.get_mut(i))
        else {
            self.scratch.truncate(scratch_start);
            return;
        };
        node.span.end = end.max(node.span.start);
        let children = self.scratch.get(scratch_start..).unwrap_or_default();
        match &mut node.kind {
            NodeKind::Mapping(mapping) => {
                let first = to_u32(self.entries.len());
                let (pairs, _) = children.as_chunks::<2>();
                for &[(key, written_key), (value, written_value)] in pairs {
                    self.entries.push(Entry { key, value });
                    self.written_entries.push(Entry {
                        key: written_key,
                        value: written_value,
                    });
                }
                mapping.entries = first..to_u32(self.entries.len());
            }
            NodeKind::Sequence(sequence) => {
                let first = to_u32(self.items.len());
                for &(item, written) in children {
                    self.items.push(item);
                    self.written_items.push(written);
                }
                sequence.items = first..to_u32(self.items.len());
            }
            NodeKind::Scalar(_) | NodeKind::Alias(_) => {}
        }
        self.scratch.truncate(scratch_start);
        self.set_closed(frame.node);
        self.child_ended(end);
    }

    fn alias(&mut self, event: &EventMarks, name: String) -> Result<(), Diagnostic> {
        let start = Self::offset(event.start_mark);
        let end = Self::offset(event.end_mark);
        let span = self.span(start, end);
        let Some(&target) = self.anchors.get(&name) else {
            return Err(Diagnostic::new(
                &UNKNOWN_ANCHOR,
                span,
                format!("could not parse as YAML: unknown anchor '{name}' referenced"),
            ));
        };
        let target_index = usize::try_from(target.0).unwrap_or(usize::MAX);
        if let Some(used) = self.used.get_mut(target_index) {
            *used = true;
        }
        let resolved = self.closed.get(target_index).copied().unwrap_or(false);
        if !resolved {
            let declared = self
                .nodes
                .get(target_index)
                .map_or(start, |node| node.span.start);
            let pos = self.file.line_col(declared);
            self.diagnostics.push(Diagnostic::new(
                &RECURSIVE_ALIAS,
                span,
                format!(
                    "recursive alias {} is found. anchor was declared at line:{}, column:{}",
                    quote(&name),
                    pos.line,
                    pos.column
                ),
            ));
        }
        let node = Node {
            span,
            content_start: start,
            anchor: None,
            kind: NodeKind::Alias(Alias {
                name: name.into_boxed_str(),
                target: resolved.then_some(target),
            }),
        };
        self.add(node, event.start_mark.line, resolved.then_some(target));
        self.child_ended(end);
        Ok(())
    }

    fn finish(mut self) -> (Document, Vec<Diagnostic>) {
        for (node, used) in self.nodes.iter().zip(&self.used) {
            let Some(anchor) = node.anchor.as_ref().filter(|_| !used) else {
                continue;
            };
            // The span starts where go-yaml reports the node, which is the
            // tag if the tag comes before the anchor.
            let span = self.span(node.span.start, anchor.span.end.max(node.span.start));
            self.diagnostics.push(Diagnostic::new(
                &UNUSED_ANCHOR,
                span,
                format!("anchor {} is defined but not used", quote(&anchor.name)),
            ));
        }
        let node_comments = attach(
            &self.nodes,
            &self.lines,
            &self.roles,
            &mut self.comments,
            &self.comment_places,
        );
        // The vectors grew by doubling. The document lives while the
        // workflow is parsed and checked, so give back the spare capacity.
        for list in [&mut self.items, &mut self.written_items] {
            list.shrink_to_fit();
        }
        for list in [&mut self.entries, &mut self.written_entries] {
            list.shrink_to_fit();
        }
        self.nodes.shrink_to_fit();
        self.comments.shrink_to_fit();
        let document = Document {
            file: self.id,
            root: self.root,
            start: self.start,
            nodes: self.nodes,
            items: self.items,
            entries: self.entries,
            written_items: self.written_items,
            written_entries: self.written_entries,
            comments: self.comments,
            node_comments,
        };
        (document, self.diagnostics)
    }
}
