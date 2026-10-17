//! The dump of the `workflint-yaml` tree, in the format of
//! `xtask/goyaml-dump`.

use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::event::{EventData, EventParser};
use workflint_yaml::{Document, Node, NodeId, NodeKind, ScalarStyle, decode, parse};

/// Dumps the tree that `workflint-yaml` builds from `bytes`.
///
/// The format is the one `xtask/goyaml-dump/main.go` documents: one line per
/// node of the tree before alias resolution, in pre-order, then `empty` for
/// a file without a root and `error: ` for a file that `workflint-yaml`
/// rejects. The bytes are decoded as go-yaml decodes them, with [`decode`].
/// Positions come from the line index of [`SourceFile`]. A node's tag is
/// printed when the node has an explicit tag, which [`explicit_tags`] finds.
pub(crate) fn dump(bytes: &[u8]) -> String {
    let text = match decode(bytes) {
        Ok(text) => text,
        Err(error) => return format!("error: {}\n", error.problem()),
    };
    let file = match SourceFile::new(FileId::new(0), "", text) {
        Ok(file) => file,
        Err(error) => return format!("error: {error}\n"),
    };
    let parsed = parse(&file);
    let Some(document) = parsed.document else {
        let mut out = String::new();
        for diagnostic in &parsed.diagnostics {
            let pos = file.line_col(diagnostic.span.start);
            let _ = writeln!(
                out,
                "error: {}:{}: {}",
                pos.line,
                pos.column,
                diagnostic.message.replace('\n', "\\n")
            );
        }
        if out.is_empty() {
            out.push_str("error: no tree and no diagnostic\n");
        }
        return out;
    };
    let Some(root) = document.root() else {
        return "empty\n".to_owned();
    };
    let mut dumper = Dumper {
        file: &file,
        document: &document,
        nodes: document.nodes().collect(),
        explicit_tags: explicit_tags(file.text()),
        next: 0,
        out: String::new(),
    };
    dumper.node(root, 0);
    dumper.out
}

/// Returns whether each node of the first document of `text` has an
/// explicit tag, indexed by node ID.
///
/// The tree keeps a node's tag, but not whether the tag was written or
/// resolved, so this reads the events that the tree is built from. Each
/// scalar, collection start, and alias event of the first document makes
/// one node, in the order of the node IDs. The non-specific tag `!` is not
/// an explicit tag, as in go-yaml.
fn explicit_tags(text: &str) -> Vec<bool> {
    let mut explicit = Vec::new();
    for event in EventParser::new(text) {
        let Ok(event) = event else { break };
        let tag = match event.data {
            EventData::Scalar { tag, .. }
            | EventData::SequenceStart { tag, .. }
            | EventData::MappingStart { tag, .. } => tag,
            EventData::Alias { .. } => None,
            EventData::DocumentEnd { .. } => break,
            _ => continue,
        };
        explicit.push(tag.is_some_and(|tag| !tag.is_empty() && tag != "!"));
    }
    explicit
}

/// The style of a scalar as a word after a space, or nothing for a plain
/// scalar, as `xtask/goyaml-dump` prints it.
fn style(style: ScalarStyle) -> &'static str {
    match style {
        ScalarStyle::Plain => "",
        ScalarStyle::SingleQuoted => " single",
        ScalarStyle::DoubleQuoted => " double",
        ScalarStyle::Literal(_) => " literal",
        ScalarStyle::Folded(_) => " folded",
    }
}

/// Walks the tree with resolved aliases and prints the tree as written.
///
/// Node IDs follow the order in which the nodes start, which is the
/// pre-order of the tree as written, and a resolved alias keeps its own ID.
/// So the next ID in the walk is either the node in the parent's slot, or
/// the alias that resolution replaced with that node.
struct Dumper<'a> {
    file: &'a SourceFile,
    document: &'a Document,
    nodes: Vec<(NodeId, &'a Node)>,
    /// Whether each node has an explicit tag, indexed by node ID.
    explicit_tags: Vec<bool>,
    next: usize,
    out: String,
}

impl Dumper<'_> {
    /// Prints the node in `slot` and the nodes under it, in pre-order.
    ///
    /// The walk keeps the slots still to print on a heap stack, so a deep
    /// tree cannot overflow the thread's stack.
    fn node(&mut self, slot: NodeId, depth: usize) {
        // The slot and depth of each node still to print, the next one last.
        let mut stack = vec![(slot, depth)];
        while let Some((slot, depth)) = stack.pop() {
            let indent = "  ".repeat(depth);
            let Some(&(id, node)) = self.nodes.get(self.next) else {
                let _ = writeln!(self.out, "{indent}missing node #{}", slot.index());
                continue;
            };
            self.next += 1;
            let pos = self.file.line_col(node.span.start);
            let _ = write!(
                self.out,
                "{indent}{} {}:{}",
                kind(node),
                pos.line,
                pos.column
            );
            if let NodeKind::Scalar(scalar) = &node.kind {
                self.out.push_str(style(scalar.style));
            }
            let explicit = usize::try_from(id.index())
                .ok()
                .and_then(|index| self.explicit_tags.get(index));
            if let (Some(true), Some(tag)) = (explicit, node.tag()) {
                let _ = write!(self.out, " {}", tag.as_str());
            }
            if let Some(anchor) = &node.anchor {
                let _ = write!(self.out, " &{}", anchor.name);
            }
            // A note in parentheses marks a tree that breaks the ID order
            // above. go-yaml's dump never has one, so the file shows up as
            // differing.
            match &node.kind {
                NodeKind::Alias(alias) => {
                    let _ = write!(self.out, " *{}", alias.name);
                    if id != slot && alias.target != Some(slot) {
                        let _ = write!(self.out, " (its parent refers to #{})", slot.index());
                    }
                    self.out.push('\n');
                }
                _ if id != slot => {
                    let _ = writeln!(self.out, " (its parent refers to #{})", slot.index());
                }
                NodeKind::Scalar(scalar) => {
                    let _ = writeln!(self.out, " {}", quote(&scalar.value));
                }
                NodeKind::Sequence(sequence) => {
                    self.out.push('\n');
                    let items = self.document.items(sequence).iter().rev();
                    stack.extend(items.map(|&item| (item, depth + 1)));
                }
                NodeKind::Mapping(mapping) => {
                    self.out.push('\n');
                    for entry in self.document.entries(mapping).iter().rev() {
                        stack.push((entry.value, depth + 1));
                        stack.push((entry.key, depth + 1));
                    }
                }
            }
        }
    }
}

fn kind(node: &Node) -> &'static str {
    match node.kind {
        NodeKind::Scalar(_) => "scalar",
        NodeKind::Sequence(_) => "sequence",
        NodeKind::Mapping(_) => "mapping",
        NodeKind::Alias(_) => "alias",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(lines: &[&str]) -> String {
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }

    #[test]
    fn nodes_print_kind_position_anchor_and_value_in_pre_order() {
        assert_eq!(
            dump(b"on: &a push\njobs:\n  - x\n  - 'y'\n"),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"on\"",
                "  scalar 1:5 &a \"push\"",
                "  scalar 2:1 \"jobs\"",
                "  sequence 3:3",
                "    scalar 3:5 \"x\"",
                "    scalar 4:5 single \"y\"",
            ]),
        );
    }

    #[test]
    fn scalar_styles_and_explicit_tags_are_printed() {
        assert_eq!(
            dump(b"a: !!str x\nb: !custom [\"q\"]\nc: ! y\nd: |\n  l\ne: >-\n  f\nf: !!map {}\n"),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"a\"",
                "  scalar 1:4 !!str \"x\"",
                "  scalar 2:1 \"b\"",
                "  sequence 2:4 !custom",
                "    scalar 2:13 double \"q\"",
                "  scalar 3:1 \"c\"",
                "  scalar 3:4 \"y\"",
                "  scalar 4:1 \"d\"",
                "  scalar 4:4 literal \"l\\n\"",
                "  scalar 6:1 \"e\"",
                "  scalar 6:4 folded \"f\"",
                "  scalar 8:1 \"f\"",
                "  mapping 8:4 !!map",
            ]),
        );
    }

    #[test]
    fn an_explicit_tag_comes_before_the_anchor() {
        assert_eq!(
            dump(b"a: &x !<tag:yaml.org,2002:int> 1\nb: *x\n"),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"a\"",
                "  scalar 1:4 !!int &x \"1\"",
                "  scalar 2:1 \"b\"",
                "  alias 2:4 *x",
            ]),
        );
    }

    #[test]
    fn an_alias_is_printed_where_it_is_written() {
        assert_eq!(
            dump(b"a: &x [1, {k: v}]\nb: *x\nc: [*x, 2]\n"),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"a\"",
                "  sequence 1:4 &x",
                "    scalar 1:8 \"1\"",
                "    mapping 1:11",
                "      scalar 1:12 \"k\"",
                "      scalar 1:15 \"v\"",
                "  scalar 2:1 \"b\"",
                "  alias 2:4 *x",
                "  scalar 3:1 \"c\"",
                "  sequence 3:4",
                "    alias 3:5 *x",
                "    scalar 3:9 \"2\"",
            ]),
        );
    }

    #[test]
    fn a_recursive_alias_is_printed_as_an_alias() {
        assert_eq!(
            dump(b"a: &a [*a]\n"),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"a\"",
                "  sequence 1:4 &a",
                "    alias 1:8 *a",
            ]),
        );
    }

    #[test]
    fn a_tree_at_the_depth_limit_dumps_on_a_two_mebibyte_stack() {
        // go-yaml accepts this depth and rejects one level more.
        const MAX_DEPTH: usize = 10_000;
        let text = format!("{}{}\n", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        let out = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || dump(text.as_bytes()))
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(out.lines().count(), MAX_DEPTH);
        let innermost = format!("{}sequence 1:{MAX_DEPTH}", "  ".repeat(MAX_DEPTH - 1));
        assert_eq!(out.lines().last(), Some(innermost.as_str()));
    }

    #[test]
    fn a_bom_takes_no_column_and_crlf_is_one_line_break() {
        assert_eq!(
            dump(b"\xef\xbb\xbfon: push\r\njobs:\r\n  a: 1\r\n"),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"on\"",
                "  scalar 1:5 \"push\"",
                "  scalar 2:1 \"jobs\"",
                "  mapping 3:3",
                "    scalar 3:3 \"a\"",
                "    scalar 3:6 \"1\"",
            ]),
        );
    }

    #[test]
    fn columns_count_code_points_and_values_are_quoted_like_go() {
        assert_eq!(
            dump("日本: \"語 😀\\t\"\n😀: |-\n  x\n".as_bytes()),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"日本\"",
                "  scalar 1:5 double \"語 😀\\t\"",
                "  scalar 2:1 \"😀\"",
                "  scalar 2:4 literal \"x\"",
            ]),
        );
    }

    #[test]
    fn an_implicit_null_is_right_after_the_colon() {
        assert_eq!(
            dump(b"on:\n"),
            lines(&["mapping 1:1", "  scalar 1:1 \"on\"", "  scalar 1:4 \"\""]),
        );
    }

    #[test]
    fn a_file_without_a_root_prints_empty() {
        assert_eq!(dump(b""), "empty\n");
        assert_eq!(dump(b"# only a comment\n"), "empty\n");
    }

    #[test]
    fn only_the_first_document_is_dumped() {
        assert_eq!(
            dump(b"a: 1\n---\nb: [\n"),
            lines(&["mapping 1:1", "  scalar 1:1 \"a\"", "  scalar 1:4 \"1\""]),
        );
    }

    #[test]
    fn a_rejected_file_prints_the_error_and_its_position() {
        assert_eq!(
            dump(b"a: [\n"),
            "error: 2:1: could not parse as YAML: did not find expected node content\n",
        );

        assert_eq!(
            dump(b"a: *b\n"),
            "error: 1:4: could not parse as YAML: unknown anchor 'b' referenced\n",
        );
    }

    #[test]
    fn invalid_utf8_is_an_error_with_the_go_yaml_message() {
        assert_eq!(dump(b"a: \xff\n"), "error: invalid leading UTF-8 octet\n");
    }

    #[test]
    fn utf16_with_a_byte_order_mark_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend("a: \u{e9}\n".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(
            dump(&bytes),
            lines(&[
                "mapping 1:1",
                "  scalar 1:1 \"a\"",
                "  scalar 1:4 \"\u{e9}\""
            ]),
        );
    }
}
