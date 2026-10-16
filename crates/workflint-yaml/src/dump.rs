//! A text dump of the tree for snapshot tests.

use std::fmt::Write as _;

use workflint_diagnostics::{SourceFile, Span};

use crate::CommentKind;
use crate::document::{
    BlockHeader, Chomping, CollectionStyle, Document, Node, NodeId, NodeKind, ScalarStyle,
};

impl Document {
    /// Renders the tree as text, one node per line, indented by depth.
    ///
    /// Each line has the node's role in its parent, its ID, kind, style,
    /// tag, and span, then its content start if that differs from the span
    /// start, its anchor, and a scalar's decoded value. Positions are
    /// `line:column@byte`, with 1-based lines and columns. A node reached
    /// again through a resolved alias is printed once and then referred to
    /// by its ID. The aliases and the comments follow the tree.
    ///
    /// `file` must be the file the document was parsed from. The format is
    /// for tests and may change.
    #[must_use]
    pub fn dump(&self, file: &SourceFile) -> String {
        let mut out = String::new();
        let mut printed = vec![false; self.nodes.len()];
        match self.root {
            Some(root) => self.dump_node(file, &mut out, &mut printed, "root", root, 0),
            None => out.push_str("no root\n"),
        }

        let aliases: Vec<_> = self
            .nodes()
            .filter_map(|(id, node)| match &node.kind {
                NodeKind::Alias(alias) => Some((id, node, alias)),
                _ => None,
            })
            .collect();
        if !aliases.is_empty() {
            out.push_str("aliases:\n");
        }
        for (id, node, alias) in aliases {
            let target = alias
                .target
                .map_or_else(|| "unresolved".to_owned(), |t| format!("#{}", t.0));
            let _ = writeln!(
                out,
                "  #{} *{} {} -> {target}",
                id.0,
                alias.name,
                span(file, node.span)
            );
        }

        if !self.comments.is_empty() {
            out.push_str("comments:\n");
        }
        for comment in &self.comments {
            let kind = match comment.kind {
                CommentKind::Trailing => "trailing",
                CommentKind::OwnLine => "own-line",
            };
            let node = comment
                .node
                .map_or_else(|| "none".to_owned(), |n| format!("#{}", n.0));
            let _ = writeln!(
                out,
                "  {} {kind} {:?} -> {node}",
                span(file, comment.span),
                text(file, comment.span)
            );
        }
        out
    }

    fn dump_node(
        &self,
        file: &SourceFile,
        out: &mut String,
        printed: &mut [bool],
        role: &str,
        id: NodeId,
        depth: usize,
    ) {
        let indent = "  ".repeat(depth);
        let Some(node) = self.node(id) else {
            let _ = writeln!(out, "{indent}{role} #{} missing", id.0);
            return;
        };
        let Some(seen) = usize::try_from(id.0).ok().and_then(|i| printed.get_mut(i)) else {
            return;
        };
        if *seen {
            let _ = writeln!(out, "{indent}{role} #{} (printed above)", id.0);
            return;
        }
        *seen = true;
        let _ = writeln!(out, "{indent}{role} #{} {}", id.0, describe(file, node));
        match &node.kind {
            NodeKind::Sequence(sequence) => {
                for &item in self.items(sequence) {
                    self.dump_node(file, out, printed, "item", item, depth + 1);
                }
            }
            NodeKind::Mapping(mapping) => {
                for entry in self.entries(mapping) {
                    self.dump_node(file, out, printed, "key", entry.key, depth + 1);
                    self.dump_node(file, out, printed, "value", entry.value, depth + 1);
                }
            }
            NodeKind::Scalar(_) | NodeKind::Alias(_) => {}
        }
    }
}

fn describe(file: &SourceFile, node: &Node) -> String {
    let mut line = match &node.kind {
        NodeKind::Scalar(scalar) => {
            format!(
                "scalar {} {}",
                scalar_style(scalar.style),
                scalar.tag.as_str()
            )
        }
        NodeKind::Sequence(sequence) => format!(
            "sequence {} {}",
            collection_style(sequence.style),
            sequence.tag.as_str()
        ),
        NodeKind::Mapping(mapping) => format!(
            "mapping {} {}",
            collection_style(mapping.style),
            mapping.tag.as_str()
        ),
        NodeKind::Alias(alias) => format!("alias *{}", alias.name),
    };
    let _ = write!(line, " {}", span(file, node.span));
    if node.content_start != node.span.start {
        let _ = write!(line, " content {}", position(file, node.content_start));
    }
    if let Some(anchor) = &node.anchor {
        let _ = write!(line, " &{} {}", anchor.name, span(file, anchor.span));
    }
    if let NodeKind::Scalar(scalar) = &node.kind {
        let _ = write!(line, " {:?}", scalar.value);
    }
    line
}

fn scalar_style(style: ScalarStyle) -> String {
    match style {
        ScalarStyle::Plain => "plain".to_owned(),
        ScalarStyle::SingleQuoted => "single-quoted".to_owned(),
        ScalarStyle::DoubleQuoted => "double-quoted".to_owned(),
        ScalarStyle::Literal(header) => format!("literal{}", block_header(header)),
        ScalarStyle::Folded(header) => format!("folded{}", block_header(header)),
    }
}

fn block_header(header: BlockHeader) -> String {
    let chomping = match header.chomping {
        Chomping::Clip => "clip",
        Chomping::Strip => "strip",
        Chomping::Keep => "keep",
    };
    match header.indent {
        Some(indent) => format!("({chomping},indent={indent})"),
        None => format!("({chomping})"),
    }
}

fn collection_style(style: CollectionStyle) -> &'static str {
    match style {
        CollectionStyle::Block => "block",
        CollectionStyle::Flow => "flow",
    }
}

fn position(file: &SourceFile, offset: u32) -> String {
    let pos = file.line_col(offset);
    format!("{}:{}@{offset}", pos.line, pos.column)
}

fn span(file: &SourceFile, span: Span) -> String {
    format!(
        "{}..{}",
        position(file, span.start),
        position(file, span.end)
    )
}

fn text(file: &SourceFile, span: Span) -> &str {
    let (Ok(start), Ok(end)) = (usize::try_from(span.start), usize::try_from(span.end)) else {
        return "";
    };
    file.text().get(start..end).unwrap_or_default()
}
