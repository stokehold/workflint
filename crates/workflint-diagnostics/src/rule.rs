/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// A problem that makes the workflow fail or behave wrongly.
    Error,
    /// A likely problem.
    Warning,
    /// A suggestion.
    Info,
}

impl Severity {
    /// Returns the name used in config and output: `error`, `warning`, or `info`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// The rule set a rule belongs to. Only [`Category::Correctness`] rules are on by default (ADR 0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// Everything actionlint checks, plus mistakes that make a workflow fail or misbehave on
    /// GitHub. On by default.
    Correctness,
    /// `pedantic`. Off by default.
    Pedantic,
    /// `style`. Off by default.
    Style,
    /// `security-strict`. Off by default.
    SecurityStrict,
    /// `policy`. Off by default.
    Policy,
}

impl Category {
    /// Returns the name used in config and output, such as `security-strict`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::Pedantic => "pedantic",
            Self::Style => "style",
            Self::SecurityStrict => "security-strict",
            Self::Policy => "policy",
        }
    }
}

/// The static description of a rule.
///
/// Each crate that emits diagnostics defines a `static` value of this type per rule, and the rule
/// registry refers to the same values.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RuleMeta {
    /// The kebab-case rule ID, such as `undefined-property`.
    pub id: &'static str,
    /// The actionlint error kind the rule maps to, such as `expression` or `syntax-check`.
    pub kind: &'static str,
    /// The rule set the rule belongs to.
    pub category: Category,
    /// The severity of the rule's diagnostics unless config overrides it.
    pub default_severity: Severity,
    /// The URL of the rule's documentation page.
    pub docs_url: &'static str,
}
