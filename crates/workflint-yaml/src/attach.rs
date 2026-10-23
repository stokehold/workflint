//! Attaches comments to nodes (ADR 0009).

use crate::CommentKind;
use crate::document::{Comment, Node, NodeId};

/// Where a node sits in its parent as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Root,
    Key,
    Value,
    Item,
}

/// Where a comment is, besides its span.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommentPlace {
    /// The 0-based line of the comment.
    pub(crate) line: u32,
    /// For a trailing comment that only `-` and `?` indicators precede on
    /// its line, the offset after the last indicator. See
    /// [`after_indicators`].
    pub(crate) after_indicators: Option<u32>,
}

/// If only `-` and `?` indicators and blanks precede `offset` on its line in
/// `text`, returns the offset right after the last indicator. Returns `None`
/// if anything else precedes it, or nothing does.
pub(crate) fn after_indicators(text: &str, offset: u32) -> Option<u32> {
    let before = text.get(..usize::try_from(offset).ok()?)?;
    let mut after = None;
    // Walks back from `offset`. Each indicator must have a blank after it,
    // and a blank or the start of the line before it.
    let mut chars = before.char_indices().rev().peekable();
    while let Some((i, ch)) = chars.next() {
        match ch {
            ' ' | '\t' => {}
            '-' | '?' => {
                let preceded_by_blank = match chars.peek() {
                    None | Some(&(0, '\u{feff}')) => true,
                    Some(&(_, prev)) => matches!(prev, ' ' | '\t') || is_line_start(prev),
                };
                if !preceded_by_blank {
                    return None;
                }
                after = after.or_else(|| u32::try_from(i + 1).ok());
            }
            '\u{feff}' if i == 0 => break,
            _ if is_line_start(ch) => break,
            _ => return None,
        }
    }
    after
}

/// Whether `ch` is a line break, so the character after it starts a line.
fn is_line_start(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Returns, for each comment, the 0-based line after it that holds no
/// own-line comment. Only own-line comments use it.
fn next_code_lines(comments: &[Comment], places: &[CommentPlace]) -> Vec<u32> {
    let mut next = vec![0; comments.len()];
    // The line of the own-line comment after the current one, and its next
    // code line.
    let mut later: Option<(u32, u32)> = None;
    for ((comment, place), next) in comments.iter().zip(places).zip(&mut next).rev() {
        if comment.kind != CommentKind::OwnLine {
            continue;
        }
        let line = place.line;
        *next = match later {
            Some((later_line, later_next)) if later_line == line.saturating_add(1) => later_next,
            _ => line.saturating_add(1),
        };
        later = Some((line, *next));
    }
    next
}

/// Sets `Comment::node` for each comment and returns the attached comments
/// as `(node, comment index)` pairs, sorted by node and then by comment.
///
/// `lines` and `roles` hold the 0-based start line and the role of each
/// node, and `places` the place of each comment.
pub(crate) fn attach(
    nodes: &[Node],
    lines: &[u32],
    roles: &[Role],
    comments: &mut [Comment],
    places: &[CommentPlace],
) -> Vec<(NodeId, u32)> {
    let start = |i: u32| {
        usize::try_from(i)
            .ok()
            .and_then(|i| nodes.get(i))
            .map_or(u32::MAX, |node| node.span.start)
    };
    let line = |i: u32| usize::try_from(i).ok().and_then(|i| lines.get(i)).copied();
    let role = |i: u32| usize::try_from(i).ok().and_then(|i| roles.get(i)).copied();

    // Node IDs by start offset. The sort is stable, so a collection comes
    // before a first child that starts at the same offset.
    let mut order: Vec<u32> = (0..u32::try_from(nodes.len()).unwrap_or(u32::MAX)).collect();
    order.sort_by_key(|&i| start(i));

    // The node a comment before the node at `first` in `order` attaches to:
    // the first mapping key or sequence item that starts on the line of that
    // node, at or after it, or else that node.
    let first_on_line = |first: usize| {
        let first_line = order.get(first).and_then(|&i| line(i))?;
        let on_line = order
            .get(first..)
            .unwrap_or_default()
            .iter()
            .copied()
            .take_while(|&i| line(i) == Some(first_line));
        on_line
            .clone()
            .find(|&i| matches!(role(i), Some(Role::Key | Role::Item)))
            .or_else(|| on_line.clone().next())
    };

    let next_lines = next_code_lines(comments, places);
    let mut pairs: Vec<(NodeId, u32)> = Vec::new();
    for (((index, comment), place), next_line) in (0u32..)
        .zip(comments.iter_mut())
        .zip(places)
        .zip(next_lines)
    {
        let node = match (comment.kind, place.after_indicators) {
            (CommentKind::Trailing, Some(after)) => {
                first_on_line(order.partition_point(|&i| start(i) < after))
            }
            (CommentKind::Trailing, None) => {
                let before = order.partition_point(|&i| start(i) < comment.span.start);
                before
                    .checked_sub(1)
                    .and_then(|k| order.get(k))
                    .copied()
                    .filter(|&i| line(i) == Some(place.line))
            }
            (CommentKind::OwnLine, _) => {
                // Start lines grow with start offsets, so `order` is sorted
                // by line too.
                let first = order.partition_point(|&i| line(i).is_none_or(|l| l < next_line));
                first_on_line(first).filter(|&i| line(i) == Some(next_line))
            }
        };
        comment.node = node.map(NodeId);
        if let Some(node) = comment.node {
            pairs.push((node, index));
        }
    }

    // Comments are in source order, and the sort is stable, so each node's
    // comments stay in source order.
    pairs.sort_by_key(|&(node, _)| node);
    pairs
}
