//! The config file (ADR 0010): finding it in a project, reading it, and writing the default one
//! for `-init-config`.
//!
//! A config file is decoded as the reference decodes `.github/actionlint.yaml`, with go-yaml's
//! error texts ([`ConfigDocument`]). Its `paths` globs are checked when it is read, and its
//! `ignore` patterns are compiled then. A key that the reference does not read is a warning in
//! an actionlint config file and an error in a workflint config file.

use std::fmt;
use std::fs;
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_rules::RuleConfig;
use workflint_syntax::{ConfigDocument, UnknownKey};

use crate::doublestar;
use crate::ignore::IgnorePattern;
use crate::project::ProjectRoots;

/// The file that `-init-config` writes: the reference's template.
pub const DEFAULT_CONFIG: &str = "self-hosted-runner:
  # Labels of self-hosted runner in array of strings.
  labels: []

# Configuration variables in array of strings defined in your repository or
# organization. `null` means disabling configuration variables check.
# Empty array means no configuration variable is allowed.
config-variables: null

# Configuration for file paths. The keys are glob patterns to match to file
# paths relative to the repository root. The values are the configurations for
# the file paths. Note that the path separator is always '/'.
# The following configurations are available.
#
# \"ignore\" is an array of regular expression patterns. Matched error messages
# are ignored. This is similar to the \"-ignore\" command line option.
paths:
#  .github/workflows/**/*.yml:
#    ignore: []
";

/// The config files in a project's `.github` directory, in the order they are looked up.
const FILE_NAMES: [&str; 4] = [
    "workflint.yaml",
    "workflint.yml",
    "actionlint.yaml",
    "actionlint.yml",
];

/// The file that `-init-config` writes in a project's `.github` directory.
const DEFAULT_FILE_NAME: &str = "workflint.yaml";

/// Whether a config file is actionlint's or workflint's, which decides what an unknown key
/// does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKind {
    /// An actionlint config file. An unknown key is a warning, because actionlint ignores it.
    Actionlint,
    /// A workflint config file. An unknown key is an error.
    Workflint,
}

impl ConfigKind {
    /// The kind of the config file at `path`: a file named `workflint.yaml` or `workflint.yml`
    /// is a workflint config file, and any other file an actionlint config file.
    #[must_use]
    pub fn of_path(path: &Path) -> Self {
        match path.file_name().and_then(|name| name.to_str()) {
            Some("workflint.yaml" | "workflint.yml") => Self::Workflint,
            _ => Self::Actionlint,
        }
    }
}

/// A config file that cannot be read, decoded, or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

/// A key of an actionlint config file that workflint ignores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWarning {
    path: PathBuf,
    key: UnknownKey,
}

impl fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "config file {}: {} is ignored",
            quote(&self.path.to_string_lossy()),
            self.key
        )
    }
}

/// One entry of `paths`: a glob, validated, and its ignore patterns.
#[derive(Debug, Clone)]
pub(crate) struct PathConfig {
    glob: String,
    ignore: Vec<IgnorePattern>,
}

impl PathConfig {
    /// Whether the glob matches `path`, a path from the project root with `/` separators.
    pub(crate) fn matches(&self, path: &str) -> bool {
        doublestar::is_match(&self.glob, path)
    }

    pub(crate) fn ignore(&self) -> &[IgnorePattern] {
        &self.ignore
    }
}

/// The `shellcheck` keys of a config file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellcheckConfig {
    /// `rcfile`, as written: a path from the project root.
    pub rcfile: Option<String>,
    /// `args`: arguments for shellcheck after workflint's own.
    pub args: Vec<String>,
    /// `cache`: whether results are kept in the on-disk cache.
    pub cache: bool,
    /// Whether the file has a `shellcheck` key that is not null, which asks for shellcheck.
    pub requested: bool,
}

/// A config file that was read.
#[derive(Debug, Clone)]
pub struct Config {
    path: PathBuf,
    kind: ConfigKind,
    rules: RuleConfig,
    paths: Vec<PathConfig>,
    shellcheck: ShellcheckConfig,
    tool_timeout: Option<Duration>,
    warnings: Vec<ConfigWarning>,
}

impl Config {
    /// Decodes `text`, the config file at `path`, as a config file of `kind`.
    ///
    /// # Errors
    ///
    /// Fails if `text` does not decode, a glob of `paths` or an `ignore` pattern is invalid, or
    /// a workflint config file has an unknown key. The message starts with
    /// `could not parse config file "<path>": `.
    pub fn parse(path: &Path, kind: ConfigKind, text: &str) -> Result<Self, ConfigError> {
        let fail = |message: &dyn fmt::Display| {
            ConfigError::new(format!(
                "could not parse config file {}: {message}",
                quote(&path.to_string_lossy())
            ))
        };
        let file = SourceFile::new(FileId::new(0), path.to_string_lossy(), text.to_owned())
            .map_err(|err| fail(&err))?;
        let compile =
            &mut |pattern: &str| IgnorePattern::new(pattern).map_err(|err| err.to_string());
        let document = ConfigDocument::decode(&file, compile).map_err(|err| fail(&err))?;
        let mut paths = Vec::with_capacity(document.paths.len());
        for entry in document.paths {
            if !doublestar::validate(&entry.glob) {
                return Err(fail(&format!(
                    "invalid glob pattern {} in \"paths\"",
                    quote(&entry.glob)
                )));
            }
            paths.push(PathConfig {
                glob: entry.glob,
                ignore: entry.ignore,
            });
        }
        let mut warnings = Vec::new();
        for key in document.unknown_keys {
            match kind {
                ConfigKind::Workflint => {
                    // The first unknown key stops the run. Fixing it shows the next one.
                    return Err(fail(&key));
                }
                ConfigKind::Actionlint => warnings.push(ConfigWarning {
                    path: path.to_path_buf(),
                    key,
                }),
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            kind,
            rules: RuleConfig {
                runner_labels: document.runner_labels.unwrap_or_default(),
                config_variables: document.config_variables,
                python_linter: document.python_linter.unwrap_or_default(),
            },
            paths,
            shellcheck: document
                .shellcheck
                .map(|keys| ShellcheckConfig {
                    rcfile: keys.rcfile,
                    args: keys.args.unwrap_or_default(),
                    cache: keys.cache.unwrap_or_default(),
                    requested: true,
                })
                .unwrap_or_default(),
            tool_timeout: document.tool_timeout.map(Duration::from_secs),
            warnings,
        })
    }

    /// Reads the config file at `path`, whose kind its name decides ([`ConfigKind::of_path`]).
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, with a message that starts with
    /// `could not read config file "<path>": `, or for the reasons of [`parse`](Self::parse).
    pub fn read(path: &Path) -> Result<Self, ConfigError> {
        let bytes = fs::read(path).map_err(|err| read_error(path, &err))?;
        Self::from_bytes(path, &bytes)
    }

    /// Finds and reads the config file of the project at `root`: the first of
    /// `.github/workflint.yaml`, `.github/workflint.yml`, `.github/actionlint.yaml`, and
    /// `.github/actionlint.yml` that exists. Returns `None` if none exists.
    ///
    /// # Errors
    ///
    /// Fails if a file exists but cannot be read, or for the reasons of
    /// [`parse`](Self::parse).
    pub fn find(root: &Path) -> Result<Option<Self>, ConfigError> {
        let dir = root.join(".github");
        for name in FILE_NAMES {
            let path = dir.join(name);
            match fs::read(&path) {
                Ok(bytes) => return Self::from_bytes(&path, &bytes).map(Some),
                Err(err) if err.kind() == ErrorKind::NotFound => {}
                Err(err) => return Err(read_error(&path, &err)),
            }
        }
        Ok(None)
    }

    /// Decodes `bytes`, which may be UTF-8 or UTF-16, as the config file at `path`.
    fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Self, ConfigError> {
        let kind = ConfigKind::of_path(path);
        match workflint_yaml::decode(bytes) {
            Ok(text) => Self::parse(path, kind, &text),
            Err(err) => Err(ConfigError::new(format!(
                "could not parse config file {}: yaml: {}",
                quote(&path.to_string_lossy()),
                err.problem()
            ))),
        }
    }

    /// The path of the file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the file is actionlint's or workflint's.
    #[must_use]
    pub fn kind(&self) -> ConfigKind {
        self.kind
    }

    /// The settings that the rules read.
    #[must_use]
    pub fn rules(&self) -> &RuleConfig {
        &self.rules
    }

    /// The `shellcheck` keys.
    #[must_use]
    pub fn shellcheck(&self) -> &ShellcheckConfig {
        &self.shellcheck
    }

    /// `tool-timeout`: how long an external tool process may run, if the file sets it.
    #[must_use]
    pub fn tool_timeout(&self) -> Option<Duration> {
        self.tool_timeout
    }

    /// The error for a run whose shellcheck was not found, `reason`, when this file asks for
    /// shellcheck with a `shellcheck` key.
    pub(crate) fn missing_shellcheck(&self, reason: &str) -> Option<ConfigError> {
        self.shellcheck.requested.then(|| {
            ConfigError::new(format!(
                "shellcheck that config file {} configures was not found: {reason}",
                quote(&self.path.to_string_lossy())
            ))
        })
    }

    /// Reads `shellcheck.rcfile`, a path from `base`, and returns its path and its text, or
    /// `None` if the key is not set.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, with a message that names it and this config file.
    pub fn read_rcfile(&self, base: &Path) -> Result<Option<(PathBuf, Vec<u8>)>, ConfigError> {
        let Some(rcfile) = &self.shellcheck.rcfile else {
            return Ok(None);
        };
        let path = crate::path::absolute(base, Path::new(rcfile));
        match fs::read(&path) {
            Ok(text) => Ok(Some((path, text))),
            Err(err) => Err(ConfigError::new(format!(
                "could not read shellcheck.rcfile {} of config file {}: {err}",
                quote(&path.to_string_lossy()),
                quote(&self.path.to_string_lossy())
            ))),
        }
    }

    /// The unknown keys of an actionlint config file, in the order they are written.
    #[must_use]
    pub fn warnings(&self) -> &[ConfigWarning] {
        &self.warnings
    }

    /// Whether an `ignore` pattern of a `paths` entry whose glob matches `path` matches
    /// `message`. `path` is the path of the linted file from the project root, with `/`
    /// separators.
    #[must_use]
    pub fn is_ignored(&self, path: &str, message: &str) -> bool {
        self.paths_for(path)
            .any(|entry| entry.ignore.iter().any(|pattern| pattern.is_match(message)))
    }

    /// The `paths` entries whose glob matches `path`, in order.
    fn paths_for<'c>(&'c self, path: &'c str) -> impl Iterator<Item = &'c PathConfig> {
        self.paths.iter().filter(move |entry| entry.matches(path))
    }

    /// Every `paths` entry, in order.
    pub(crate) fn paths_for_all(&self) -> &[PathConfig] {
        &self.paths
    }
}

fn read_error(path: &Path, err: &io::Error) -> ConfigError {
    ConfigError::new(format!(
        "could not read config file {}: {err}",
        quote(&path.to_string_lossy())
    ))
}

/// Writes [`DEFAULT_CONFIG`] to `.github/workflint.yaml` in the project that contains `cwd`,
/// the reference's `GenerateDefaultConfig`, and returns the path of the file.
///
/// # Errors
///
/// Fails if `cwd` is in no project, if the project already has one of the config files that
/// [`Config::find`] looks for, or if the file cannot be written.
pub fn init_config(cwd: &Path) -> Result<PathBuf, ConfigError> {
    tracing::info!(
        "Generating default {DEFAULT_FILE_NAME} in repository: {}",
        cwd.display()
    );
    let Some(root) = ProjectRoots::default().find_dir(cwd, cwd) else {
        return Err(ConfigError::new(
            "project is not found. check current project is initialized as Git repository and \
             \".github/workflows\" directory exists"
                .to_owned(),
        ));
    };
    let dir = root.join(".github");
    for name in FILE_NAMES {
        let path = dir.join(name);
        if fs::metadata(&path).is_ok() {
            return Err(ConfigError::new(format!(
                "config file already exists at {}",
                quote(&path.to_string_lossy())
            )));
        }
    }
    let path = dir.join(DEFAULT_FILE_NAME);
    write_new(&path, DEFAULT_CONFIG.as_bytes()).map_err(|err| {
        ConfigError::new(format!(
            "could not write default configuration file at {}: {err}",
            quote(&path.to_string_lossy())
        ))
    })?;
    Ok(path)
}

/// Writes `contents` to a new file at `path` with mode 0644 on Unix, as the reference does.
fn write_new(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    options.open(path)?.write_all(contents)
}
