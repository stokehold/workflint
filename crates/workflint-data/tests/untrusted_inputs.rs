//! Checks the untrusted input paths against the reference's `BuiltinUntrustedInputs` in the
//! vendored `expr_insecure.go`, and ports `TestExprInsecureBuiltinUntrustedInputs` from
//! `expr_insecure_test.go`.

#![allow(clippy::panic)]

mod go_source;

use go_source::builtin_untrusted_inputs;
use workflint_data::{UNTRUSTED_INPUTS, UntrustedInput};

/// The reference's `testAllUntrustedInputs`.
const ALL_UNTRUSTED_INPUTS: [&str; 20] = [
    "github.event.issue.title",
    "github.event.issue.body",
    "github.event.pull_request.title",
    "github.event.pull_request.body",
    "github.event.comment.body",
    "github.event.review.body",
    "github.event.review_comment.body",
    "github.event.pages.*.page_name",
    "github.event.commits.*.message",
    "github.event.head_commit.message",
    "github.event.head_commit.author.email",
    "github.event.head_commit.author.name",
    "github.event.commits.*.author.email",
    "github.event.commits.*.author.name",
    "github.event.pull_request.head.ref",
    "github.event.pull_request.head.label",
    "github.event.pull_request.head.repo.default_branch",
    "github.event.discussion.title",
    "github.event.discussion.body",
    "github.head_ref",
];

/// Pushes the path of each leaf below `node` to `paths`, in table order.
fn leaf_paths(node: &UntrustedInput, prefix: &str, paths: &mut Vec<String>) {
    let path = if prefix.is_empty() {
        node.name.to_owned()
    } else {
        format!("{prefix}.{}", node.name)
    };
    if node.is_leaf() {
        paths.push(path);
        return;
    }
    for child in node.children {
        leaf_paths(child, &path, paths);
    }
}

fn workflint_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for root in UNTRUSTED_INPUTS {
        leaf_paths(root, "", &mut paths);
    }
    paths
}

#[test]
fn untrusted_inputs_match_the_reference_table() {
    assert_eq!(workflint_paths(), builtin_untrusted_inputs());
}

#[test]
fn the_reference_table_has_the_twenty_paths_of_its_tests() {
    let mut reference = builtin_untrusted_inputs();
    reference.sort();
    let mut tested = ALL_UNTRUSTED_INPUTS.map(str::to_owned).to_vec();
    tested.sort();
    assert_eq!(reference, tested);
}

/// The first half of `TestExprInsecureBuiltinUntrustedInputs`: each path leads to a leaf.
#[test]
fn every_untrusted_path_ends_at_a_leaf() {
    for input in ALL_UNTRUSTED_INPUTS {
        let mut names = input.split('.');
        let root = names.next().unwrap_or_default();
        let Some(mut node) = UNTRUSTED_INPUTS.iter().find(|node| node.name == root) else {
            panic!("{root} in {input} is not a root");
        };
        for name in names {
            let Some(child) = node.child(name) else {
                panic!("{name} in {input} is not in the table below {}", node.name);
            };
            node = child;
        }
        assert!(node.is_leaf(), "{input} does not end at a leaf");
    }
}

/// The second half of `TestExprInsecureBuiltinUntrustedInputs`: names are lower case, and `*`
/// has no siblings. Also checks that no name appears twice among siblings.
#[test]
fn names_are_lower_case_and_star_is_an_only_child() {
    fn check(siblings: &[UntrustedInput], path: &str) {
        for (i, node) in siblings.iter().enumerate() {
            let here = format!("{path}.{}", node.name);
            if node.name == "*" {
                assert_eq!(siblings.len(), 1, "{here} has siblings");
            } else {
                assert!(
                    !node.name.is_empty()
                        && node
                            .name
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b == b'_'),
                    "{here} does not match ^[a-z_]+$"
                );
            }
            assert!(
                siblings[..i].iter().all(|other| other.name != node.name),
                "{here} appears twice"
            );
            check(node.children, &here);
        }
    }
    check(UNTRUSTED_INPUTS, "");
}
