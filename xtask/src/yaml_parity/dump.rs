//! The dump of the `workflint-yaml` tree, in the format of
//! `xtask/goyaml-dump`.

use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::{Document, Node, NodeId, NodeKind, decode, parse};

/// Dumps the tree that `workflint-yaml` builds from `bytes`.
///
/// The format is the one `xtask/goyaml-dump/main.go` documents: one line per
/// node of the tree before alias resolution, in pre-order, then `empty` for
/// a file without a root and `error: ` for a file that `workflint-yaml`
/// rejects. The bytes are decoded as go-yaml decodes them, with [`decode`].
/// Positions come from the line index of [`SourceFile`].
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
        next: 0,
        out: String::new(),
    };
    dumper.node(root, 0);
    dumper.out
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
    next: usize,
    out: String,
}

impl Dumper<'_> {
    fn node(&mut self, slot: NodeId, depth: usize) {
        let indent = "  ".repeat(depth);
        let Some(&(id, node)) = self.nodes.get(self.next) else {
            let _ = writeln!(self.out, "{indent}missing node #{}", slot.index());
            return;
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
        if let Some(anchor) = &node.anchor {
            let _ = write!(self.out, " &{}", anchor.name);
        }
        // A note in parentheses marks a tree that breaks the ID order above.
        // go-yaml's dump never has one, so the file shows up as differing.
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
                for &item in self.document.items(sequence) {
                    self.node(item, depth + 1);
                }
            }
            NodeKind::Mapping(mapping) => {
                self.out.push('\n');
                for entry in self.document.entries(mapping) {
                    self.node(entry.key, depth + 1);
                    self.node(entry.value, depth + 1);
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
                "    scalar 4:5 \"y\"",
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
                "  scalar 1:5 \"語 😀\\t\"",
                "  scalar 2:1 \"😀\"",
                "  scalar 2:4 \"x\"",
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
