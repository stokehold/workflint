//! The part of the config that the rules read.

/// The settings of the config file that change what the rules report. `workflint-linter`
/// loads the config and fills this in. The default is what a run without a config file
/// gives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleConfig {
    /// `self-hosted-runner.labels`: the label patterns of self-hosted runners, each in the
    /// syntax of Go's `path.Match`. The `runner-label` rule knows the labels they match.
    pub runner_labels: Vec<String>,
    /// `config-variables`: the configuration variables that `vars.NAME` may read, compared
    /// without regard to case. `None` turns the check off, and an empty list allows none.
    pub config_variables: Option<Vec<String>>,
}

impl RuleConfig {
    /// The config of a run without a config file.
    pub const DEFAULT: Self = Self {
        runner_labels: Vec::new(),
        config_variables: None,
    };
}
