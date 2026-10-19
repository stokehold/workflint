//! The checks of local actions: the metadata of each action on its first use in the run, and the
//! `with:` inputs of every use.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `checkLocalAction`,
//! `checkLocalActionMetadata`, and the `runs` checks in `rule_action.go`.

use workflint_data::{is_branding_color, is_branding_icon, is_on_docker_registry};
use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{ActionRuns, ExecAction, StepUses};

use super::{DeclaredInput, check_inputs};
use crate::Context;
use crate::local::{LocalAction, Lookup};

/// A local action's `action.yml` is not valid YAML or cannot be decoded.
pub(crate) static BROKEN_ACTION_METADATA: RuleMeta = RuleMeta {
    id: "broken-action-metadata",
    description: "A local action's metadata file is not valid YAML or cannot be decoded",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/broken-action-metadata.md",
};

/// A local action's metadata has no `name`.
pub(crate) static MISSING_ACTION_NAME: RuleMeta = RuleMeta {
    id: "missing-action-name",
    description: "A local action's metadata has no `name`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-action-name.md",
};

/// A local action's metadata has no `description`.
pub(crate) static MISSING_ACTION_DESCRIPTION: RuleMeta = RuleMeta {
    id: "missing-action-description",
    description: "A local action's metadata has no `description`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-action-description.md",
};

/// A local action's `branding.icon` is not a Feather icon that GitHub supports.
pub(crate) static INVALID_BRANDING_ICON: RuleMeta = RuleMeta {
    id: "invalid-branding-icon",
    description: "A local action's `branding.icon` is not an icon that GitHub supports",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-branding-icon.md",
};

/// A local action's `branding.color` is not a color that GitHub supports.
pub(crate) static INVALID_BRANDING_COLOR: RuleMeta = RuleMeta {
    id: "invalid-branding-color",
    description: "A local action's `branding.color` is not a color that GitHub supports",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-branding-color.md",
};

/// An input of a local action has a `deprecationMessage` that is empty.
pub(crate) static EMPTY_DEPRECATION_MESSAGE: RuleMeta = RuleMeta {
    id: "empty-deprecation-message",
    description: "An input of a local action has an empty `deprecationMessage`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-deprecation-message.md",
};

/// A local action has no `runs.using`.
pub(crate) static MISSING_RUNS_USING: RuleMeta = RuleMeta {
    id: "missing-runs-using",
    description: "A local action has no `runs.using`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-runs-using.md",
};

/// A local action's `runs.using` is not a runtime that GitHub has.
pub(crate) static INVALID_RUNS_USING: RuleMeta = RuleMeta {
    id: "invalid-runs-using",
    description: "A local action's `runs.using` is not `composite`, `docker`, or a Node.js runtime that GitHub has",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-runs-using.md",
};

/// The `runs` section of a local action lacks a key that its kind of action needs.
pub(crate) static MISSING_RUNS_KEY: RuleMeta = RuleMeta {
    id: "missing-runs-key",
    description: "The `runs` section of a local action lacks the key that its kind of action needs",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-runs-key.md",
};

/// The `runs` section of a local action has a key of another kind of action.
pub(crate) static RUNS_KEY_NOT_ALLOWED: RuleMeta = RuleMeta {
    id: "runs-key-not-allowed",
    description: "The `runs` section of a local action has a key that its kind of action does not use",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/runs-key-not-allowed.md",
};

/// A file that the `runs` section of a local action names does not exist.
///
/// A warning, because an earlier step of the job may build the file (rhysd/actionlint#518).
pub(crate) static ACTION_FILE_NOT_FOUND: RuleMeta = RuleMeta {
    id: "action-file-not-found",
    description: "A file that the `runs` section of a local action names does not exist",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Warning,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/action-file-not-found.md",
};

/// A local Docker action's `image` is a local file not named `Dockerfile`.
pub(crate) static INVALID_DOCKERFILE_NAME: RuleMeta = RuleMeta {
    id: "invalid-dockerfile-name",
    description: "A local Docker action's `image` is a file that is not named `Dockerfile`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-dockerfile-name.md",
};

/// A local JavaScript action has `pre-if` without `pre`, or `post-if` without `post`.
pub(crate) static MISSING_PRE_POST_SCRIPT: RuleMeta = RuleMeta {
    id: "missing-pre-post-script",
    description: "A local JavaScript action has `pre-if` without `pre`, or `post-if` without `post`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-pre-post-script.md",
};

/// The reference's `checkLocalAction`: looks up the action, checks its metadata on the first
/// lookup of the spec in the run, and checks the `with:` inputs at every use.
pub(super) fn check(exec: &ExecAction<'_>, uses: &StepUses<'_>, cx: &mut Context<'_>) {
    let spec = uses.value();
    let action = match cx.local_metadata().action(spec) {
        Lookup::None => return,
        Lookup::Error(err) => {
            cx.report_in(
                uses.template.source,
                &BROKEN_ACTION_METADATA,
                uses.span(),
                err.to_string(),
            );
            return;
        }
        Lookup::Found { metadata, first } => {
            if first {
                Metadata::new(&metadata, uses.span()).check(cx);
            }
            metadata
        }
    };
    let declared: Vec<_> = action
        .metadata
        .inputs
        .iter()
        .map(|(id, input)| DeclaredInput {
            id,
            name: &input.name,
            required: input.required,
            deprecation: input.deprecation_message.as_deref(),
        })
        .collect();
    let describe = format!(
        "{} defined at {}",
        quote(&action.metadata.name),
        quote(spec)
    );
    check_inputs(exec, uses.span(), &declared, &describe, cx);
}

/// The kinds of local action, as the messages name them.
#[derive(Clone, Copy)]
enum ActionKind {
    Docker,
    Composite,
    JavaScript,
}

impl ActionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Composite => "Composite",
            Self::JavaScript => "JavaScript",
        }
    }
}

/// The keys of `runs` that `checkInvalidRunsProps` knows.
#[derive(Clone, Copy)]
enum RunsKey {
    Main,
    Pre,
    PreIf,
    Post,
    PostIf,
    Steps,
    Image,
    PreEntrypoint,
    Entrypoint,
    PostEntrypoint,
    Args,
    Env,
}

impl RunsKey {
    fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Pre => "pre",
            Self::PreIf => "pre-if",
            Self::Post => "post",
            Self::PostIf => "post-if",
            Self::Steps => "steps",
            Self::Image => "image",
            Self::PreEntrypoint => "pre-entrypoint",
            Self::Entrypoint => "entrypoint",
            Self::PostEntrypoint => "post-entrypoint",
            Self::Args => "args",
            Self::Env => "env",
        }
    }

    /// Whether `runs` sets the key, as the reference decides it: a non-empty string, a
    /// non-empty `steps`, or a non-null `args` or `env`.
    fn is_set(self, runs: &ActionRuns) -> bool {
        match self {
            Self::Main => !runs.main.is_empty(),
            Self::Pre => !runs.pre.is_empty(),
            Self::PreIf => !runs.pre_if.is_empty(),
            Self::Post => !runs.post.is_empty(),
            Self::PostIf => !runs.post_if.is_empty(),
            Self::Steps => runs.steps.is_some_and(|items| items > 0),
            Self::Image => !runs.image.is_empty(),
            Self::PreEntrypoint => !runs.pre_entrypoint.is_empty(),
            Self::Entrypoint => !runs.entrypoint.is_empty(),
            Self::PostEntrypoint => !runs.post_entrypoint.is_empty(),
            Self::Args => runs.has_args,
            Self::Env => runs.has_env,
        }
    }
}

/// The metadata of a local action being checked, with the texts that the messages quote.
struct Metadata<'m> {
    action: &'m LocalAction,
    /// The span of the `uses:` value, where every diagnostic goes.
    span: Span,
    /// `name`, quoted.
    name: String,
    /// The directory of the action, quoted: the reference's `Dir()`.
    dir: String,
    /// The path of the metadata file, quoted: the reference's `Path()`.
    path: String,
}

impl<'m> Metadata<'m> {
    fn new(action: &'m LocalAction, span: Span) -> Self {
        Self {
            action,
            span,
            name: quote(&action.metadata.name),
            dir: quote(&action.dir.to_string_lossy()),
            path: quote(&action.file.to_string_lossy()),
        }
    }

    /// The reference's `checkLocalActionMetadata`.
    fn check(&self, cx: &mut Context<'_>) {
        let metadata = &self.action.metadata;
        let (name, path) = (&self.name, &self.path);
        if metadata.name.is_empty() {
            cx.report(
                &MISSING_ACTION_NAME,
                self.span,
                format!("name is required in action metadata {path}"),
            );
        }
        if metadata.description.is_empty() {
            cx.report(
                &MISSING_ACTION_DESCRIPTION,
                self.span,
                format!("description is required in metadata of {name} action at {path}"),
            );
        }
        let icon = &metadata.branding.icon;
        if !icon.is_empty() && !is_branding_icon(icon) {
            cx.report(
                &INVALID_BRANDING_ICON,
                self.span,
                format!(
                    "incorrect icon name {} at branding.icon in metadata of {name} action at {path}. see the official document to know the exhaustive list of supported icons: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingicon",
                    quote(icon),
                ),
            );
        }
        // The reference says `branding.icon` here
        // (docs/differences.md#branding-color-message).
        let color = &metadata.branding.color;
        if !color.is_empty() && !is_branding_color(color) {
            cx.report(
                &INVALID_BRANDING_COLOR,
                self.span,
                format!(
                    "incorrect color {} at branding.color in metadata of {name} action at {path}. see the official document to know the exhaustive list of supported colors: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingcolor",
                    quote(color),
                ),
            );
        }
        for input in metadata.inputs.values() {
            if input.deprecation_message.as_deref() == Some("") {
                cx.report(
                    &EMPTY_DEPRECATION_MESSAGE,
                    self.span,
                    format!(
                        "input {} is deprecated but \"deprecationMessage\" is empty in metadata of {name} action at {path}",
                        quote(&input.name),
                    ),
                );
            }
        }
        self.check_runs(cx);
    }

    /// The reference's `checkLocalActionRuns`.
    fn check_runs(&self, cx: &mut Context<'_>) {
        let (name, dir) = (&self.name, &self.dir);
        let using = self.action.metadata.runs.using.as_str();
        match using {
            "" => cx.report(
                &MISSING_RUNS_USING,
                self.span,
                format!("\"runs.using\" is missing in local action {name} defined at {dir}"),
            ),
            "docker" => self.check_docker(cx),
            "composite" => self.check_composite(cx),
            "node20" | "node24" => self.check_javascript(cx),
            _ => {
                cx.report(
                    &INVALID_RUNS_USING,
                    self.span,
                    format!(
                        "invalid runner name {} at runs.using in {name} action defined at {dir}. valid runners are \"composite\", \"docker\", \"node20\", and \"node24\". see https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runs",
                        quote(using),
                    ),
                );
                // Likely an old Node.js runtime: check it as a JavaScript action to find as
                // many errors as possible, as the reference does.
                if using.starts_with("node") {
                    self.check_javascript(cx);
                }
            }
        }
    }

    /// The reference's `checkLocalDockerActionRuns`.
    fn check_docker(&self, cx: &mut Context<'_>) {
        let runs = &self.action.metadata.runs;
        if runs.image.is_empty() {
            self.missing_key(RunsKey::Image, ActionKind::Docker, cx);
        } else if !is_on_docker_registry(&runs.image) {
            self.check_file_exists(&runs.image, RunsKey::Image, cx);
            if go_base(&runs.image) != "Dockerfile" {
                cx.report(
                    &INVALID_DOCKERFILE_NAME,
                    self.span,
                    format!(
                        "the local file {} referenced from \"image\" key must be named \"Dockerfile\" in {} action. the action is defined at {}",
                        quote(&runs.image),
                        self.name,
                        self.dir,
                    ),
                );
            }
        }
        self.check_file_exists(&runs.pre_entrypoint, RunsKey::PreEntrypoint, cx);
        self.check_file_exists(&runs.entrypoint, RunsKey::Entrypoint, cx);
        self.check_file_exists(&runs.post_entrypoint, RunsKey::PostEntrypoint, cx);
        self.check_keys_not_allowed(
            ActionKind::Docker,
            &[
                RunsKey::Main,
                RunsKey::Pre,
                RunsKey::PreIf,
                RunsKey::Post,
                RunsKey::PostIf,
                RunsKey::Steps,
            ],
            cx,
        );
    }

    /// The reference's `checkLocalCompositeActionRuns`.
    fn check_composite(&self, cx: &mut Context<'_>) {
        if self.action.metadata.runs.steps.is_none() {
            self.missing_key(RunsKey::Steps, ActionKind::Composite, cx);
        }
        self.check_keys_not_allowed(
            ActionKind::Composite,
            &[
                RunsKey::Main,
                RunsKey::Pre,
                RunsKey::PreIf,
                RunsKey::Post,
                RunsKey::PostIf,
                RunsKey::Image,
                RunsKey::PreEntrypoint,
                RunsKey::Entrypoint,
                RunsKey::PostEntrypoint,
                RunsKey::Args,
                RunsKey::Env,
            ],
            cx,
        );
    }

    /// The reference's `checkLocalJavaScriptActionRuns`.
    fn check_javascript(&self, cx: &mut Context<'_>) {
        let runs = &self.action.metadata.runs;
        if runs.main.is_empty() {
            self.missing_key(RunsKey::Main, ActionKind::JavaScript, cx);
        } else {
            self.check_file_exists(&runs.main, RunsKey::Main, cx);
        }
        let hooks = [
            (&runs.pre, &runs.pre_if, RunsKey::Pre, RunsKey::PreIf),
            (&runs.post, &runs.post_if, RunsKey::Post, RunsKey::PostIf),
        ];
        for (script, condition, key, condition_key) in hooks {
            self.check_file_exists(script, key, cx);
            if script.is_empty() && !condition.is_empty() {
                cx.report(
                    &MISSING_PRE_POST_SCRIPT,
                    self.span,
                    format!(
                        "\"{}\" is required when \"{}\" is specified in \"runs\" section in {} action. the action is defined at {}",
                        key.as_str(),
                        condition_key.as_str(),
                        self.name,
                        self.dir,
                    ),
                );
            }
        }
        self.check_keys_not_allowed(
            ActionKind::JavaScript,
            &[
                RunsKey::Steps,
                RunsKey::Image,
                RunsKey::PreEntrypoint,
                RunsKey::Entrypoint,
                RunsKey::PostEntrypoint,
                RunsKey::Args,
                RunsKey::Env,
            ],
            cx,
        );
    }

    /// The reference's `missingRunsProp`.
    fn missing_key(&self, key: RunsKey, kind: ActionKind, cx: &mut Context<'_>) {
        cx.report(
            &MISSING_RUNS_KEY,
            self.span,
            format!(
                "\"{}\" is required in \"runs\" section because {} is a {} action. the action is defined at {}",
                key.as_str(),
                self.name,
                kind.as_str(),
                self.dir,
            ),
        );
    }

    /// The reference's `checkInvalidRunsProps`.
    fn check_keys_not_allowed(&self, kind: ActionKind, keys: &[RunsKey], cx: &mut Context<'_>) {
        let runs = &self.action.metadata.runs;
        for key in keys.iter().filter(|key| key.is_set(runs)) {
            cx.report(
                &RUNS_KEY_NOT_ALLOWED,
                self.span,
                format!(
                    "\"{}\" is not allowed in \"runs\" section because {} is a {} action. the action is defined at {}",
                    key.as_str(),
                    self.name,
                    kind.as_str(),
                    self.dir,
                ),
            );
        }
    }

    /// The reference's `checkRunsFileExists`: reports `file`, a path relative to the action's
    /// directory with `/` separators, if it is set and does not exist.
    fn check_file_exists(&self, file: &str, key: RunsKey, cx: &mut Context<'_>) {
        if file.is_empty() || cx.local_metadata().action_file_exists(self.action, file) {
            return;
        }
        // The reference prints the path with the separators of the host
        // (docs/differences.md#local-action-paths).
        cx.report(
            &ACTION_FILE_NOT_FOUND,
            self.span,
            format!(
                "file {} does not exist in {}. it is specified at \"{}\" key in \"runs\" section in {} action",
                quote(file),
                self.dir,
                key.as_str(),
                self.name,
            ),
        );
    }
}

/// Go's `filepath.Base` on Linux: the last name of a path with `/` separators, after trailing
/// separators are removed. The reference also splits at `\` on Windows
/// (docs/differences.md#local-action-paths).
fn go_base(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() { "." } else { "/" };
    }
    trimmed.rsplit('/').next().map_or(trimmed, |base| base)
}

#[cfg(test)]
mod tests {
    use super::go_base;

    #[test]
    fn base_is_the_last_name_as_go_returns_it() {
        assert_eq!(go_base("Dockerfile"), "Dockerfile");
        assert_eq!(go_base("build/Dockerfile"), "Dockerfile");
        assert_eq!(go_base("./build/Dockerfile/"), "Dockerfile");
        assert_eq!(go_base("build/Dockerfile.dev"), "Dockerfile.dev");
        assert_eq!(go_base("build\\Dockerfile"), "build\\Dockerfile");
        assert_eq!(go_base("//"), "/");
        assert_eq!(go_base(""), ".");
    }
}
