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

/// Returns, for each comment, the 0-based line after it that holds no
/// own-line comment. Only own-line comments use it.
fn next_code_lines(comments: &[Comment], comment_lines: &[u64]) -> Vec<u64> {
    let mut next = vec![0; comments.len()];
    // The line of the own-line comment after the current one, and its next
    // code line.
    let mut later: Option<(u64, u64)> = None;
    for ((comment, &line), next) in comments.iter().zip(comment_lines).zip(&mut next).rev() {
        if comment.kind != CommentKind::OwnLine {
            continue;
        }
        *next = match later {
            Some((later_line, later_next)) if later_line == line.saturating_add(1) => later_next,
            _ => line.saturating_add(1),
        };
        later = Some((line, *next));
    }
    next
}

/// Sets `Comment::node` for each comment and `Node::comments` for each node,
/// and returns the comment indexes grouped by node.
///
/// `lines` and `roles` hold the 0-based start line and the role of each
/// node, and `comment_lines` the line of each comment.
pub(crate) fn attach(
    nodes: &mut [Node],
    lines: &[u64],
    roles: &[Role],
    comments: &mut [Comment],
    comment_lines: &[u64],
) -> Vec<u32> {
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

    let next_lines = next_code_lines(comments, comment_lines);
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for (((index, comment), &comment_line), next_line) in (0u32..)
        .zip(comments.iter_mut())
        .zip(comment_lines)
        .zip(next_lines)
    {
        let node = match comment.kind {
            CommentKind::Trailing => {
                let before = order.partition_point(|&i| start(i) < comment.span.start);
                before
                    .checked_sub(1)
                    .and_then(|k| order.get(k))
                    .copied()
                    .filter(|&i| line(i) == Some(comment_line))
            }
            CommentKind::OwnLine => {
                // Start lines grow with start offsets, so `order` is sorted
                // by line too.
                let first = order.partition_point(|&i| line(i).is_none_or(|l| l < next_line));
                let on_line = order
                    .get(first..)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .take_while(|&i| line(i) == Some(next_line));
                on_line
                    .clone()
                    .find(|&i| matches!(role(i), Some(Role::Key | Role::Item)))
                    .or_else(|| on_line.clone().next())
            }
        };
        comment.node = node.map(NodeId);
        if let Some(node) = node {
            pairs.push((node, index));
        }
    }

    // Comments are in source order, and the sort is stable, so each node's
    // comments stay in source order.
    pairs.sort_by_key(|&(node, _)| node);
    let mut node_comments = Vec::with_capacity(pairs.len());
    for group in pairs.chunk_by(|a, b| a.0 == b.0) {
        let first = u32::try_from(node_comments.len()).unwrap_or(u32::MAX);
        node_comments.extend(group.iter().map(|&(_, comment)| comment));
        let last = u32::try_from(node_comments.len()).unwrap_or(u32::MAX);
        let node = group.first().map(|&(node, _)| node);
        if let Some(node) = node
            .and_then(|node| usize::try_from(node).ok())
            .and_then(|node| nodes.get_mut(node))
        {
            node.comments = first..last;
        }
    }
    node_comments
}
