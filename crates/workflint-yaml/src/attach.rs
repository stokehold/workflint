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

    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for ((index, comment), &comment_line) in (0u32..).zip(comments.iter_mut()).zip(comment_lines) {
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
                let next = order.partition_point(|&i| start(i) < comment.span.end);
                order.get(next).copied().map(|first| {
                    let target_line = line(first);
                    order
                        .get(next..)
                        .unwrap_or_default()
                        .iter()
                        .copied()
                        .take_while(|&i| line(i) == target_line)
                        .find(|&i| matches!(role(i), Some(Role::Key | Role::Item)))
                        .unwrap_or(first)
                })
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
