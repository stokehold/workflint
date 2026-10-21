//! The actionlint kinds and their descriptions.
//!
//! A kind is the name of one of actionlint's rules, or `syntax-check` for the errors of its
//! parser. Each [`RuleMeta`](workflint_diagnostics::RuleMeta) maps its rule ID to one of these
//! kinds. The descriptions are byte-identical to the `desc` field of the reference's `NewRule*`
//! constructors and to `error.go:258` for `syntax-check`. The `allKinds` function of `-format`
//! templates returns them, and the SARIF output (M5.15, #253) will print them.

/// An actionlint kind and its description.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Kind {
    /// The kind's name, such as `expression`.
    pub name: &'static str,
    /// The reference's description of the kind.
    pub description: &'static str,
}

/// The errors of the reference's workflow parser (`error.go:258`).
pub static SYNTAX_CHECK: Kind = Kind {
    name: "syntax-check",
    description: "Checks for GitHub Actions workflow syntax",
};

/// `rule_matrix.go`.
pub static MATRIX: Kind = Kind {
    name: "matrix",
    description: "Checks for matrix combinations in \"matrix:\"",
};

/// `rule_credentials.go`.
pub static CREDENTIALS: Kind = Kind {
    name: "credentials",
    description: "Checks for credentials in \"services:\" configuration",
};

/// `rule_shell_name.go`.
pub static SHELL_NAME: Kind = Kind {
    name: "shell-name",
    description: "Checks for shell names used for scripts in \"run:\"",
};

/// `rule_runner_label.go`.
pub static RUNNER_LABEL: Kind = Kind {
    name: "runner-label",
    description: "Checks for GitHub-hosted and preset self-hosted runner labels in \"runs-on:\"",
};

/// `rule_events.go`.
pub static EVENTS: Kind = Kind {
    name: "events",
    description: "Checks for workflow trigger events at \"on:\"",
};

/// `rule_job_needs.go`.
pub static JOB_NEEDS: Kind = Kind {
    name: "job-needs",
    description: "Checks for job IDs in \"needs:\". Undefined IDs and cyclic dependencies are checked",
};

/// `rule_action.go`.
pub static ACTION: Kind = Kind {
    name: "action",
    description: "Checks for popular actions released on GitHub, local actions, and action calls at \"uses:\"",
};

/// `rule_env_var.go`.
pub static ENV_VAR: Kind = Kind {
    name: "env-var",
    description: "Checks for environment variables configuration at \"env:\"",
};

/// `rule_id.go`.
pub static ID: Kind = Kind {
    name: "id",
    description: "Checks for duplication and naming convention of job/step IDs",
};

/// `rule_glob.go`.
pub static GLOB: Kind = Kind {
    name: "glob",
    description: "Checks for glob syntax used in branch names, tags, and paths",
};

/// `rule_permissions.go`.
pub static PERMISSIONS: Kind = Kind {
    name: "permissions",
    description: "Checks for permissions configuration in \"permissions:\". Permission names and permission scopes are checked",
};

/// `rule_workflow_call.go`.
pub static WORKFLOW_CALL: Kind = Kind {
    name: "workflow-call",
    description: "Checks for reusable workflow calls. Inputs and outputs of called reusable workflow are checked",
};

/// `rule_expression.go`.
pub static EXPRESSION: Kind = Kind {
    name: "expression",
    description: "Syntax and semantics checks for expressions embedded with ${{ }} syntax",
};

/// `rule_deprecated_commands.go`.
pub static DEPRECATED_COMMANDS: Kind = Kind {
    name: "deprecated-commands",
    description: "Checks for deprecated \"set-output\", \"save-state\", \"set-env\", and \"add-path\" commands at \"run:\"",
};

/// `rule_if_cond.go`.
pub static IF_COND: Kind = Kind {
    name: "if-cond",
    description: "Checks for if: conditions which are always true/false",
};

/// `rule_shellcheck.go`.
pub static SHELLCHECK: Kind = Kind {
    name: "shellcheck",
    description: "Checks for shell script sources in \"run:\" using shellcheck",
};

/// `rule_pyflakes.go`.
pub static PYFLAKES: Kind = Kind {
    name: "pyflakes",
    description: "Checks for Python script when \"shell: python\" is configured using Pyflakes",
};

/// Every kind, sorted by name, as the reference's `allKinds` lists them.
pub static ALL: [&Kind; 18] = [
    &ACTION,
    &CREDENTIALS,
    &DEPRECATED_COMMANDS,
    &ENV_VAR,
    &EVENTS,
    &EXPRESSION,
    &GLOB,
    &ID,
    &IF_COND,
    &JOB_NEEDS,
    &MATRIX,
    &PERMISSIONS,
    &PYFLAKES,
    &RUNNER_LABEL,
    &SHELL_NAME,
    &SHELLCHECK,
    &SYNTAX_CHECK,
    &WORKFLOW_CALL,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sorted_by_name_without_duplicates() {
        let names: Vec<_> = ALL.iter().map(|kind| kind.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }
}
