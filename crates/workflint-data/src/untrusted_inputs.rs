//! The context properties that hold untrusted input.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `BuiltinUntrustedInputs` in
//! `expr_insecure.go`, lines 86-141. Until M7 replaces it with a generated file (ADR 0011), keep
//! it equal to that table. The test `untrusted_inputs_match_the_reference_table` compares them.

/// A node in a tree of property paths whose values someone outside the repository can set,
/// such as `github.event.issue.title`.
///
/// A root is a context, and each child is a property of its parent. A node without children is
/// untrusted. The name `*` stands for the elements of an array, as in
/// `github.event.commits.*.message`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UntrustedInput {
    /// The context or property name in lower case, or `*` for array elements.
    pub name: &'static str,
    /// The properties below this one. Empty for an untrusted value.
    pub children: &'static [UntrustedInput],
}

impl UntrustedInput {
    /// A node with `children`. A node without children is an untrusted value.
    #[must_use]
    pub const fn new(name: &'static str, children: &'static [Self]) -> Self {
        Self { name, children }
    }

    /// Returns whether the node is an untrusted value rather than an object that contains one.
    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Returns the child called `name`, which is lower case or `*`.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&'static Self> {
        self.children.iter().find(|child| child.name == name)
    }
}

const fn leaf(name: &'static str) -> UntrustedInput {
    UntrustedInput::new(name, &[])
}

/// The functions whose arguments the check of scripts skips, sorted and in lower case. Each
/// returns a boolean, which cannot inject code, so `contains(github.event.issue.title, 'x')`
/// is safe in a script.
///
/// Ported from the reference's `isSafeFuncCall` in `expr_insecure.go`. The test
/// `safe_functions_match_the_reference_is_safe_func_call` compares them.
pub static SAFE_FUNCTIONS: &[&str] = &["contains", "endswith", "startswith"];

const fn node(name: &'static str, children: &'static [UntrustedInput]) -> UntrustedInput {
    UntrustedInput::new(name, children)
}

/// The untrusted inputs that the check of scripts reports, as trees rooted at their contexts.
///
/// The reference takes the paths from these pages:
///
/// - <https://securitylab.github.com/research/github-actions-untrusted-input/>
/// - <https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks>
pub static UNTRUSTED_INPUTS: &[UntrustedInput] = &[node(
    "github",
    &[
        node(
            "event",
            &[
                node("issue", &[leaf("title"), leaf("body")]),
                node(
                    "pull_request",
                    &[
                        leaf("title"),
                        leaf("body"),
                        node(
                            "head",
                            &[
                                leaf("ref"),
                                leaf("label"),
                                node("repo", &[leaf("default_branch")]),
                            ],
                        ),
                    ],
                ),
                node("comment", &[leaf("body")]),
                node("review", &[leaf("body")]),
                node("review_comment", &[leaf("body")]),
                node("pages", &[node("*", &[leaf("page_name")])]),
                node(
                    "commits",
                    &[node(
                        "*",
                        &[
                            leaf("message"),
                            node("author", &[leaf("email"), leaf("name")]),
                        ],
                    )],
                ),
                node(
                    "head_commit",
                    &[
                        leaf("message"),
                        node("author", &[leaf("email"), leaf("name")]),
                    ],
                ),
                node("discussion", &[leaf("title"), leaf("body")]),
            ],
        ),
        leaf("head_ref"),
    ],
)];
