//! The rule trait and the context its hooks report through.

use workflint_diagnostics::{Diagnostic, RuleMeta, SourceFile, Span};
use workflint_syntax::{AliasUses, Job, KeyedEntry, Step, Str, Workflow};

use crate::kind::Kind;
use crate::local::{LocalMetadata, NoProject};

/// A rule: the reference's `Rule`, which implements some of the five hooks of its `Pass`.
///
/// Every hook does nothing by default, so a rule implements only the hooks it needs. A rule is
/// created fresh for each file (see [`create_rules`](crate::create_rules)), so it may keep state
/// from one hook to the next. The lifetime `'a` is the lifetime of the workflow, so a rule can
/// keep references into it until [`workflow_post`](Self::workflow_post).
///
/// A rule reports its diagnostics through the [`Context`], each with the [`RuleMeta`] of the rule
/// ID it reports. One rule may report several rule IDs, as the reference's `events` rule does.
pub trait Rule<'a> {
    /// The reference kind of the rule, such as [`ACTION`](crate::kind::ACTION). Its place in
    /// [`CREATION_ORDER`](crate::CREATION_ORDER) decides when the rule is created.
    fn kind(&self) -> &'static Kind;

    /// Runs before any job is visited, the reference's `VisitWorkflowPre`.
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        let _ = (workflow, cx);
    }

    /// Runs for each job before its steps, the reference's `VisitJobPre`. The entry's key is the
    /// job ID.
    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let _ = (job, cx);
    }

    /// Runs for each step of a job, the reference's `VisitStep`.
    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        let _ = (step, cx);
    }

    /// Runs for each job after its steps, the reference's `VisitJobPost`.
    fn job_post(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let _ = (job, cx);
    }

    /// Runs after every job is visited, the reference's `VisitWorkflowPost`.
    fn workflow_post(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        let _ = (workflow, cx);
    }
}

/// What the hooks of a rule get besides the node: the file being linted, the metadata of the
/// project's local actions and local reusable workflows, the YAML aliases through which the
/// parser reached each string of the workflow, and the list of diagnostics that the rules report.
///
/// The config (M5.2) becomes a field of this type, so the signature of the hooks stays the same
/// when it arrives.
#[derive(Debug)]
pub struct Context<'f> {
    file: &'f SourceFile,
    local: &'f dyn LocalMetadata,
    /// The alias paths of the workflow being visited, which [`visit`](crate::visit) sets.
    pub(crate) aliases: AliasUses,
    diagnostics: Vec<Diagnostic>,
}

impl<'f> Context<'f> {
    /// Creates the context for linting `file` outside any project, with no diagnostics. Lookups
    /// of local actions and local reusable workflows find nothing.
    #[must_use]
    pub fn new(file: &'f SourceFile) -> Self {
        Self::with_local_metadata(file, &NoProject)
    }

    /// Creates the context for linting `file`, whose project has the local actions and local
    /// reusable workflows in `local`, with no diagnostics.
    #[must_use]
    pub fn with_local_metadata(file: &'f SourceFile, local: &'f dyn LocalMetadata) -> Self {
        Self {
            file,
            local,
            aliases: AliasUses::default(),
            diagnostics: Vec::new(),
        }
    }

    /// Returns the file being linted. Its text and line index turn spans into positions, and a
    /// node's source map into spans inside a scalar.
    #[must_use]
    pub fn file(&self) -> &'f SourceFile {
        self.file
    }

    /// Returns the metadata of the local actions and local reusable workflows of the project.
    #[must_use]
    pub fn local_metadata(&self) -> &'f dyn LocalMetadata {
        self.local
    }

    /// Reports a diagnostic of `rule` at `span` with the rule's default severity.
    pub fn report(&mut self, rule: &'static RuleMeta, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(rule, span, message));
    }

    /// Reports a diagnostic of `rule` at `span` with the rule's default severity and the help
    /// text `help`.
    pub fn report_with_help(
        &mut self,
        rule: &'static RuleMeta,
        span: Span,
        message: impl Into<String>,
        help: Option<String>,
    ) {
        self.diagnostics
            .push(Diagnostic::new(rule, span, message).with_help(help));
    }

    /// Reports a diagnostic of `rule` at `span`, which lies in `source`, with the rule's default
    /// severity. When the parser reached `source` through YAML aliases, the diagnostic has a label
    /// at each alias, outermost first, so the reader sees which use of an anchor caused it.
    pub fn report_in(
        &mut self,
        source: Str<'_>,
        rule: &'static RuleMeta,
        span: Span,
        message: impl Into<String>,
    ) {
        self.report_in_with_help(source, rule, span, message, None);
    }

    /// Does what [`report_in`](Self::report_in) does, with the help text `help`.
    pub fn report_in_with_help(
        &mut self,
        source: Str<'_>,
        rule: &'static RuleMeta,
        span: Span,
        message: impl Into<String>,
        help: Option<String>,
    ) {
        let mut diagnostic = Diagnostic::new(rule, span, message).with_help(help);
        diagnostic.labels = self.aliases.labels(source.alias_path(), span);
        self.diagnostics.push(diagnostic);
    }

    /// Returns the reported diagnostics in the order they were reported.
    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}
