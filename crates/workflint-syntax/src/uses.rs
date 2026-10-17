//! The `uses` values of steps and jobs, classified as the reference's
//! `action` and `workflow-call` rules read them.
//!
//! The classification reports nothing. The rules report the format errors
//! from the variant and the [`InvalidAction`] or [`InvalidWorkflow`]
//! reason.

use workflint_diagnostics::{SourceFile, Span};

use crate::{Str, contains_expression};

/// A part of a `uses` value, such as the owner or the ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsesPart<'a> {
    /// The text of the part, a slice of the decoded value.
    pub value: &'a str,
    /// The span of the part in the source file. A part decoded from an
    /// escape sequence covers the whole sequence.
    pub span: Span,
}

/// `uses` of a step: the action that the step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepUses<'a> {
    /// The scalar of the value.
    pub source: Str<'a>,
    /// The kind of action.
    pub kind: StepUsesKind<'a>,
}

/// The kind of action in a step's `uses`, in the order that the
/// reference's `RuleAction.VisitStep` checks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepUsesKind<'a> {
    /// The value contains a `${{ }}` expression, as
    /// [`contains_expression`] decides it. The reference does not check it.
    Expression,
    /// A local action, `./path`. The path is the whole value, `./`
    /// included, which is the key of the local action cache. An `@` in it
    /// is part of the path.
    Local {
        /// The path to the action's directory.
        path: UsesPart<'a>,
    },
    /// A Docker image, `docker://image[:tag]`.
    Docker {
        /// The image, after `docker://` and up to the first `:`. The
        /// reference's messages show it with `docker://` in front.
        image: UsesPart<'a>,
        /// The text after the first `:`, or `None` without one. It is
        /// empty for `docker://image:`. A registry with a port, as in
        /// `docker://host:5000/app`, makes the port the start of the tag,
        /// as in the reference's `checkDockerAction`.
        tag: Option<UsesPart<'a>>,
    },
    /// An action in a repository, `owner/repo[/path]@ref`. No part is
    /// empty, except the path.
    Repository {
        /// The text before the first `/`.
        owner: UsesPart<'a>,
        /// The text after the owner, up to the next `/` or the `@`.
        repo: UsesPart<'a>,
        /// The text between the `/` after the repo and the `@`, or `None`
        /// without that `/`. It may be empty, as in `owner/repo/@v1`, and
        /// may contain more `/`.
        path: Option<UsesPart<'a>>,
        /// The text after the first `@`. It may contain more `@`.
        git_ref: UsesPart<'a>,
    },
    /// The value is none of the formats above.
    Invalid(InvalidAction),
}

/// Why a step's `uses` value is not a valid repository action, as the
/// reference's `checkRepoAction` decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidAction {
    /// The value has no `@`. An empty value has none either.
    MissingRef,
    /// The text before the first `@` has no `/`.
    MissingOwner,
    /// The owner, the repo, or the ref is empty.
    EmptyPart,
}

impl InvalidAction {
    /// Returns the reason as the reference's `invalidActionFormat` message
    /// writes it, after "because".
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::MissingRef => "ref is missing",
            Self::MissingOwner => "owner is missing",
            Self::EmptyPart => "owner and repo and ref should not be empty",
        }
    }
}

impl<'a> StepUses<'a> {
    /// Classifies `source`, a scalar parsed from `file`.
    ///
    /// If `file` is not the file of `source`, every part's span is the span
    /// of `source`.
    #[must_use]
    pub fn new(source: Str<'a>, file: &SourceFile) -> Self {
        let kind = with_spans(source, file, |span| classify_step(source.value(), span));
        Self { source, kind }
    }

    /// Returns the value as written, decoded.
    #[must_use]
    pub fn value(&self) -> &'a str {
        self.source.value()
    }

    /// Returns the span of the scalar.
    #[must_use]
    pub fn span(&self) -> Span {
        self.source.span()
    }
}

/// `uses` of a job: the reusable workflow that the job calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobUses<'a> {
    /// The scalar of the value.
    pub source: Str<'a>,
    /// The kind of reusable workflow.
    pub kind: JobUsesKind<'a>,
}

/// The kind of reusable workflow in a job's `uses`, in the order that the
/// reference's `RuleWorkflowCall.VisitJobPre` checks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobUsesKind<'a> {
    /// The value contains a `${{ }}` expression, as
    /// [`contains_expression`] decides it. The reference does not check it.
    Expression,
    /// A local workflow, `./path`, as the reference's
    /// `isWorkflowCallUsesLocalFormat` decides it: the path after `./` is
    /// not empty and has no `@` after its first character. The path is the
    /// whole value, `./` included, which is the key of the local workflow
    /// cache.
    Local {
        /// The path to the workflow file.
        path: UsesPart<'a>,
    },
    /// A workflow in a repository, `owner/repo/path@ref`, as the
    /// reference's `isWorkflowCallUsesRepoFormat` decides it. No part is
    /// empty, and the value does not start with `.`.
    Repository {
        /// The text before the first `/`.
        owner: UsesPart<'a>,
        /// The text after the owner, up to the next `/`.
        repo: UsesPart<'a>,
        /// The text after the repo, up to the next `@`. It may contain more
        /// `/`.
        path: UsesPart<'a>,
        /// The text after that `@`. It may contain more `@`.
        git_ref: UsesPart<'a>,
    },
    /// The value is none of the formats above.
    ///
    /// The reference skips an empty value, which its parser reports, so the
    /// parser here never classifies one.
    Invalid(InvalidWorkflow),
}

/// Why a job's `uses` value is neither a local nor a repository workflow.
///
/// The reference's `workflow-call` message gives no reason. This is the
/// first check of `isWorkflowCallUsesLocalFormat` or
/// `isWorkflowCallUsesRepoFormat` that fails, with the local format's
/// reason for a value that starts with `./`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidWorkflow {
    /// The value starts with `./` and has an `@` after the character that
    /// follows `./`. A local workflow cannot have a ref.
    LocalWithRef,
    /// The value is `./`.
    LocalWithoutPath,
    /// The value starts with `.` but not with `./`, as in
    /// `.github/workflows/ci.yml` or `../ci.yml`. A repository workflow
    /// cannot start with `.`.
    DotPrefix,
    /// The value has no `/`, or nothing before the first one. An empty
    /// value has no `/`.
    MissingOwner,
    /// Nothing is between the first and the second `/`.
    MissingRepo,
    /// The value has no second `/`, or nothing between it and the next
    /// `@`. With one `/`, the text after it is read as the repo, so
    /// `owner/workflow.yml@v1` is missing its path.
    MissingPath,
    /// No `@` follows the second `/`, or nothing follows that `@`.
    MissingRef,
}

impl<'a> JobUses<'a> {
    /// Classifies `source`, a scalar parsed from `file`.
    ///
    /// If `file` is not the file of `source`, every part's span is the span
    /// of `source`.
    #[must_use]
    pub fn new(source: Str<'a>, file: &SourceFile) -> Self {
        let kind = with_spans(source, file, |span| classify_job(source.value(), span));
        Self { source, kind }
    }

    /// Returns the value as written, decoded.
    #[must_use]
    pub fn value(&self) -> &'a str {
        self.source.value()
    }

    /// Returns the span of the scalar.
    #[must_use]
    pub fn span(&self) -> Span {
        self.source.span()
    }
}

/// Calls `classify` with a function that turns a byte range of the value of
/// `source` into a span in `file`.
fn with_spans<T>(
    source: Str<'_>,
    file: &SourceFile,
    classify: impl FnOnce(&dyn Fn(usize, usize) -> Span) -> T,
) -> T {
    match source.source_map(file) {
        Some(map) => classify(&|start, end| map.source_span(start..end)),
        None => classify(&|_, _| source.span()),
    }
}

/// Classifies a step's `uses` value, as the reference's
/// `RuleAction.VisitStep` and `checkRepoAction` read it. `span` turns a
/// byte range of `value` into a span.
fn classify_step(value: &str, span: impl Fn(usize, usize) -> Span) -> StepUsesKind<'_> {
    if contains_expression(value) {
        return StepUsesKind::Expression;
    }
    if value.starts_with("./") {
        return StepUsesKind::Local {
            path: part(&span, 0, value),
        };
    }
    if let Some(rest) = value.strip_prefix(DOCKER) {
        let start = DOCKER.len();
        return match rest.split_once(':') {
            Some((image, tag)) => StepUsesKind::Docker {
                image: part(&span, start, image),
                tag: Some(part(&span, start + image.len() + 1, tag)),
            },
            None => StepUsesKind::Docker {
                image: part(&span, start, rest),
                tag: None,
            },
        };
    }
    let Some((head, git_ref)) = value.split_once('@') else {
        return StepUsesKind::Invalid(InvalidAction::MissingRef);
    };
    let Some((owner, rest)) = head.split_once('/') else {
        return StepUsesKind::Invalid(InvalidAction::MissingOwner);
    };
    let (repo, path) = match rest.split_once('/') {
        Some((repo, path)) => (repo, Some(path)),
        None => (rest, None),
    };
    if owner.is_empty() || repo.is_empty() || git_ref.is_empty() {
        return StepUsesKind::Invalid(InvalidAction::EmptyPart);
    }
    let repo_start = owner.len() + 1;
    StepUsesKind::Repository {
        owner: part(&span, 0, owner),
        repo: part(&span, repo_start, repo),
        path: path.map(|path| part(&span, repo_start + repo.len() + 1, path)),
        git_ref: part(&span, head.len() + 1, git_ref),
    }
}

/// Classifies a job's `uses` value, as the reference's
/// `RuleWorkflowCall.VisitJobPre` reads it. `span` turns a byte range of
/// `value` into a span.
fn classify_job(value: &str, span: impl Fn(usize, usize) -> Span) -> JobUsesKind<'_> {
    if contains_expression(value) {
        return JobUsesKind::Expression;
    }
    if let Some(path) = value.strip_prefix("./") {
        if path.is_empty() {
            return JobUsesKind::Invalid(InvalidWorkflow::LocalWithoutPath);
        }
        if path.find('@').is_some_and(|at| at > 0) {
            return JobUsesKind::Invalid(InvalidWorkflow::LocalWithRef);
        }
        return JobUsesKind::Local {
            path: part(&span, 0, value),
        };
    }
    if value.starts_with('.') {
        return JobUsesKind::Invalid(InvalidWorkflow::DotPrefix);
    }
    let Some((owner, rest)) = value.split_once('/').filter(|(owner, _)| !owner.is_empty()) else {
        return JobUsesKind::Invalid(InvalidWorkflow::MissingOwner);
    };
    let Some((repo, rest)) = rest.split_once('/') else {
        return JobUsesKind::Invalid(InvalidWorkflow::MissingPath);
    };
    if repo.is_empty() {
        return JobUsesKind::Invalid(InvalidWorkflow::MissingRepo);
    }
    let Some((path, git_ref)) = rest.split_once('@') else {
        return JobUsesKind::Invalid(InvalidWorkflow::MissingRef);
    };
    if path.is_empty() {
        return JobUsesKind::Invalid(InvalidWorkflow::MissingPath);
    }
    if git_ref.is_empty() {
        return JobUsesKind::Invalid(InvalidWorkflow::MissingRef);
    }
    let repo_start = owner.len() + 1;
    let path_start = repo_start + repo.len() + 1;
    JobUsesKind::Repository {
        owner: part(&span, 0, owner),
        repo: part(&span, repo_start, repo),
        path: part(&span, path_start, path),
        git_ref: part(&span, path_start + path.len() + 1, git_ref),
    }
}

/// The prefix of a Docker image action.
const DOCKER: &str = "docker://";

/// Returns the part `text`, which starts at byte `start` of the value.
fn part<'a>(span: &impl Fn(usize, usize) -> Span, start: usize, text: &'a str) -> UsesPart<'a> {
    UsesPart {
        value: text,
        span: span(start, start + text.len()),
    }
}

#[cfg(test)]
mod tests;
