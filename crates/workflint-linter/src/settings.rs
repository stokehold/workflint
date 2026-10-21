//! What a file is linted with: the defaults, then its config file, then the flags.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use workflint_rules::RuleConfig;

use crate::config::{Config, ConfigError, PathConfig};
use crate::ignore::IgnorePattern;
use crate::tools::ShellcheckOptions;

/// The flags of a run that change how files are linted. The binary fills them from the command
/// line.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    /// `-ignore`: patterns that drop the diagnostics whose message they match.
    pub ignore: Vec<IgnorePattern>,
    /// `-config-file`: the config file for every file of the run, in place of the config file
    /// of each project.
    pub config_file: Option<Arc<Config>>,
    /// `-shellcheck-cache`: every project uses the on-disk cache of shellcheck.
    pub shellcheck_cache: bool,
}

/// What the lint of one file runs with: the defaults, [`Settings::default`], then the config
/// file, then the flags ([`Settings::new`]). A setting that both a config key and a flag set
/// takes the flag's value.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// The path of the config file, if there is one.
    config_path: Option<PathBuf>,
    /// What the rules read.
    rules: RuleConfig,
    /// The `paths` entries of the config file.
    paths: Vec<PathConfig>,
    /// The patterns of `-ignore`.
    ignore: Vec<IgnorePattern>,
    /// How shellcheck runs.
    shellcheck: Arc<ShellcheckOptions>,
}

impl Settings {
    /// The settings of the files of a project with `config`, if it has a config file, and
    /// `flags`. `base` is the project root, or the working directory for files in no project:
    /// `shellcheck.rcfile` is a path from it.
    ///
    /// # Errors
    ///
    /// Fails if `shellcheck.rcfile` cannot be read.
    pub fn new(config: Option<&Config>, base: &Path, flags: &Flags) -> Result<Self, ConfigError> {
        let mut shellcheck = ShellcheckOptions::default();
        let mut settings = Self::default();
        if let Some(config) = config {
            if let Some((path, text)) = config.read_rcfile(base)? {
                shellcheck = shellcheck.with_rcfile(path, text);
            }
            shellcheck = shellcheck
                .with_args(config.shellcheck().args.clone())
                .with_cache(config.shellcheck().cache)
                .with_timeout(config.tool_timeout());
            settings = settings.with_config(config);
        }
        if flags.shellcheck_cache {
            shellcheck = shellcheck.with_cache(true);
        }
        Ok(Self {
            shellcheck: Arc::new(shellcheck),
            ..settings.with_flags(flags)
        })
    }

    /// Applies the keys of `config` that the rules and the ignores read.
    fn with_config(self, config: &Config) -> Self {
        Self {
            config_path: Some(config.path().to_path_buf()),
            rules: config.rules().clone(),
            paths: config.paths_for_all().to_vec(),
            ..self
        }
    }

    /// Applies the `-ignore` patterns of `flags`.
    fn with_flags(self, flags: &Flags) -> Self {
        Self {
            ignore: flags.ignore.clone(),
            ..self
        }
    }

    /// The path of the config file, if there is one.
    #[must_use]
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// What the rules read.
    #[must_use]
    pub fn rules(&self) -> &RuleConfig {
        &self.rules
    }

    /// How shellcheck runs on the scripts of the file.
    #[must_use]
    pub fn shellcheck(&self) -> &Arc<ShellcheckOptions> {
        &self.shellcheck
    }

    /// The patterns that drop diagnostics of the file at `path`, a path from the project root
    /// with `/` separators: those of `-ignore` and those of the `paths` entries whose glob
    /// matches `path`.
    pub(crate) fn ignores_for(&self, path: &str) -> Ignores<'_> {
        Ignores {
            flags: &self.ignore,
            paths: self
                .paths
                .iter()
                .filter(|entry| entry.matches(path))
                .flat_map(PathConfig::ignore)
                .collect(),
        }
    }
}

/// The patterns that drop the diagnostics of one file.
pub(crate) struct Ignores<'s> {
    /// From `-ignore`.
    pub(crate) flags: &'s [IgnorePattern],
    /// From the `paths` entries of the config file that match the file.
    pub(crate) paths: Vec<&'s IgnorePattern>,
}

impl Ignores<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.flags.is_empty() && self.paths.is_empty()
    }
}
