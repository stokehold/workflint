//! The config file, decoded as the reference decodes `.github/actionlint.yaml`, with the keys
//! that workflint adds.

use std::collections::BTreeSet;
use std::fmt;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{LineCol, SourceFile};
use workflint_yaml::{Entry, NodeId, NodeKind, Tag};

use crate::typed::{DecodeError, Decoder, Fatal, Source, UnmarshalError};

/// The keys of the config's root mapping: the reference's, then workflint's.
const ROOT_FIELDS: &[&str] = &[
    "self-hosted-runner",
    "config-variables",
    "paths",
    "python",
    "shellcheck",
    "tool-timeout",
];

/// The keys of `python`.
const PYTHON_FIELDS: &[&str] = &["linter"];

/// The keys of `shellcheck`.
const SHELLCHECK_FIELDS: &[&str] = &["rcfile", "args", "cache"];

/// The Go type of `self-hosted-runner`, an anonymous struct, as go-yaml names it.
const SELF_HOSTED_RUNNER_TYPE: &str = r#"struct { Labels []string "yaml:\"labels\"" }"#;

/// A config file, decoded into the reference's `Config` struct and workflint's keys. `P` is a
/// compiled pattern of `paths.<glob>.ignore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDocument<P> {
    /// `self-hosted-runner.labels`. `None` if the key is missing or null.
    pub runner_labels: Option<Vec<String>>,
    /// `config-variables`. `None` if the key is missing or null.
    pub config_variables: Option<Vec<String>>,
    /// The entries of `paths`, in the order go-yaml sets them: as written, then the entries
    /// that `<<` keys merge in.
    pub paths: Vec<PathEntry<P>>,
    /// `python.linter`. `None` if the key is missing or null.
    pub python_linter: Option<PythonLinter>,
    /// `shellcheck`. `None` if the key is missing or null.
    pub shellcheck: Option<ShellcheckKeys>,
    /// `tool-timeout`, in seconds. `None` if the key is missing or null.
    pub tool_timeout: Option<u64>,
    /// The keys that neither the reference nor workflint reads, in the order they are written.
    /// A key that a `<<` key merges in more than once is listed once.
    pub unknown_keys: Vec<UnknownKey>,
}

/// The value of `python.linter`: what checks the scripts of `shell: python` steps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PythonLinter {
    /// `auto`: ruff if it is installed, else pyflakes.
    #[default]
    Auto,
    /// `ruff`, restricted to the pyflakes rules.
    Ruff,
    /// `pyflakes`.
    Pyflakes,
    /// `off`: nothing checks the scripts.
    Off,
}

impl PythonLinter {
    /// Every value, in the order the docs list them.
    pub const ALL: [Self; 4] = [Self::Auto, Self::Ruff, Self::Pyflakes, Self::Off];

    /// The value as written in the config file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Ruff => "ruff",
            Self::Pyflakes => "pyflakes",
            Self::Off => "off",
        }
    }
}

impl fmt::Display for PythonLinter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One entry of `paths`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEntry<P> {
    /// The key: a glob, not validated.
    pub glob: String,
    /// The compiled patterns of `ignore`, in order.
    pub ignore: Vec<P>,
}

/// The keys of `shellcheck`. A missing or null key is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellcheckKeys {
    /// `rcfile`, as written.
    pub rcfile: Option<String>,
    /// `args`.
    pub args: Option<Vec<String>>,
    /// `cache`.
    pub cache: Option<bool>,
}

/// A key of the config file that neither the reference nor workflint reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKey {
    /// The text of the key.
    pub key: String,
    /// The mapping that has the key.
    pub parent: KeyParent,
    /// Where the key is written.
    pub position: LineCol,
}

/// The mapping that has an [`UnknownKey`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyParent {
    /// The root mapping.
    Root,
    /// `self-hosted-runner`.
    SelfHostedRunner,
    /// `shellcheck`.
    Shellcheck,
    /// The value of the `paths` entry with this glob.
    Path(String),
    /// `python`.
    Python,
}

impl fmt::Display for UnknownKey {
    /// Formats the key as `unknown key "x" in "self-hosted-runner" at line:2,col:3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown key {}", quote(&self.key))?;
        match &self.parent {
            KeyParent::Root => {}
            KeyParent::SelfHostedRunner => f.write_str(" in \"self-hosted-runner\"")?,
            KeyParent::Shellcheck => f.write_str(" in \"shellcheck\"")?,
            KeyParent::Path(glob) => write!(f, " in \"paths\" entry {}", quote(glob))?,
            KeyParent::Python => f.write_str(" in \"python\"")?,
        }
        write!(f, " at {}", self.position)
    }
}

/// The text of the error of a config file that does not decode, without the path of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDecodeError {
    message: String,
}

impl ConfigDecodeError {
    /// Returns the text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ConfigDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigDecodeError {}

impl<P> ConfigDocument<P> {
    /// Decodes the config file in `file` as the reference's `ParseConfig` does, compiling each
    /// pattern of `paths.<glob>.ignore` with `compile`. An empty file, or one with only
    /// comments, has no keys.
    ///
    /// # Errors
    ///
    /// Returns the reference's error text if the file is not valid YAML or does not decode into
    /// the reference's `Config`. Several errors are joined as the reference joins them, with
    /// their line breaks replaced by spaces. The errors of `ignore` differ from the reference's:
    /// an entry that is a collection or null is an error, and an invalid pattern is reported at
    /// its own position, written `line:L,col:C` (docs/differences.md#config-ignore-entries,
    /// docs/differences.md#config-error-messages).
    ///
    /// ```
    /// use workflint_diagnostics::{FileId, SourceFile};
    /// use workflint_syntax::ConfigDocument;
    ///
    /// let text = "config-variables: [FOO]\nself-hosted-runner:\n  label: [gpu]\n";
    /// let file = SourceFile::new(FileId::new(0), "actionlint.yaml", text).unwrap();
    /// let compile = &mut |pattern: &str| Ok::<_, String>(pattern.to_owned());
    /// let config = ConfigDocument::decode(&file, compile).unwrap();
    /// assert_eq!(config.config_variables, Some(vec!["FOO".to_owned()]));
    /// assert_eq!(
    ///     config.unknown_keys[0].to_string(),
    ///     r#"unknown key "label" in "self-hosted-runner" at line:3,col:3"#,
    /// );
    /// ```
    pub fn decode(
        file: &SourceFile,
        compile: &mut dyn FnMut(&str) -> Result<P, String>,
    ) -> Result<Self, ConfigDecodeError> {
        let error = |message: String| ConfigDecodeError {
            message: message.replace('\n', " "),
        };
        let source = Source::parse(file).map_err(error)?;
        let decoded = source.unmarshal(|d, root| {
            let mut config = Self {
                runner_labels: None,
                config_variables: None,
                paths: Vec::new(),
                python_linter: None,
                shellcheck: None,
                tool_timeout: None,
                unknown_keys: Vec::new(),
            };
            if let Some(root) = root {
                d.structure(
                    root,
                    "actionlint.Config",
                    ROOT_FIELDS,
                    &mut |d, field, value| {
                        match field {
                            0 => {
                                d.structure(
                                    value,
                                    SELF_HOSTED_RUNNER_TYPE,
                                    &["labels"],
                                    &mut |d, _, value| {
                                        d.string_slice(value, &mut config.runner_labels)?;
                                        Ok(())
                                    },
                                )?;
                            }
                            1 => {
                                d.string_slice(value, &mut config.config_variables)?;
                            }
                            3 => {
                                d.structure(
                                    value,
                                    "workflint.PythonConfig",
                                    PYTHON_FIELDS,
                                    &mut |d, _, value| {
                                        if let Some(linter) =
                                            d.custom(value, decode_python_linter)?
                                        {
                                            config.python_linter = Some(linter);
                                        }
                                        Ok(())
                                    },
                                )?;
                            }
                            4 => {
                                config.shellcheck = decode_shellcheck(d, value)?;
                            }
                            5 => {
                                config.tool_timeout =
                                    d.custom(value, |d, id| decode_seconds(d, id, "tool-timeout"))?;
                            }
                            _ => {
                                d.string_map(
                                    value,
                                    "map[string]actionlint.PathConfig",
                                    &mut |d, glob, value| {
                                        let mut ignore = Vec::new();
                                        d.structure(
                                            value,
                                            "actionlint.PathConfig",
                                            &["ignore"],
                                            &mut |d, _, value| {
                                                let patterns = d.custom(value, |d, id| {
                                                    decode_ignore(d, id, compile)
                                                })?;
                                                if let Some(patterns) = patterns {
                                                    ignore = patterns;
                                                }
                                                Ok(())
                                            },
                                        )?;
                                        config.paths.push(PathEntry { glob, ignore });
                                        Ok(())
                                    },
                                )?;
                            }
                        }
                        Ok(())
                    },
                )?;
            }
            Ok(config)
        });
        let mut config = decoded.map_err(|err| error(err.to_string()))?;
        if let Some(root) = source.root() {
            config.unknown_keys = unknown_keys(&source, root);
        }
        Ok(config)
    }
}

/// Decodes `shellcheck`. Returns `None` for a null.
fn decode_shellcheck(d: &mut Decoder<'_>, id: NodeId) -> Result<Option<ShellcheckKeys>, Fatal> {
    let mut keys = ShellcheckKeys::default();
    let set = d.structure(
        id,
        "workflint.ShellcheckConfig",
        SHELLCHECK_FIELDS,
        &mut |d, field, value| {
            match field {
                0 => {
                    d.optional_string(value, &mut keys.rcfile)?;
                }
                1 => {
                    d.string_slice(value, &mut keys.args)?;
                }
                _ => {
                    let mut cache = false;
                    if d.bool(value, &mut cache)? {
                        keys.cache = Some(cache);
                    }
                }
            }
            Ok(())
        },
    )?;
    Ok(set.then_some(keys))
}

/// Decodes a number of seconds greater than 0, the value of the key `key`.
fn decode_seconds(d: &mut Decoder<'_>, id: NodeId, key: &str) -> Result<u64, DecodeError> {
    let source = d.source();
    let item = source.resolve_alias(id);
    let at = source.position(item);
    let found = match source.node(item).map(|node| &node.kind) {
        Some(NodeKind::Scalar(scalar)) => {
            if scalar.tag == Tag::Int
                && let Ok(seconds) = scalar.value.parse::<u64>()
                && seconds > 0
            {
                return Ok(seconds);
            }
            quote(&scalar.value)
        }
        Some(NodeKind::Sequence(_)) => "a sequence".to_owned(),
        Some(NodeKind::Mapping(_)) => "a mapping".to_owned(),
        Some(NodeKind::Alias(_)) | None => "an alias".to_owned(),
    };
    Err(DecodeError::Type(vec![UnmarshalError {
        line: at.line,
        message: format!(
            "{} must be a whole number of seconds greater than 0, but {found} was found at {at}",
            quote(key)
        ),
    }]))
}

/// Decodes `ignore` as the reference's `IgnorePatterns.UnmarshalYAML` does, except that an entry
/// that is a collection or null is an error and that an alias entry is the text it refers to.
fn decode_ignore<P>(
    d: &mut Decoder<'_>,
    id: NodeId,
    compile: &mut dyn FnMut(&str) -> Result<P, String>,
) -> Result<Vec<P>, DecodeError> {
    let source = d.source();
    let is_sequence = matches!(
        source.node(id).map(|node| &node.kind),
        Some(NodeKind::Sequence(_))
    );
    if !is_sequence {
        let at = source.position(id);
        return Err(DecodeError::Other(format!(
            "yaml: \"ignore\" must be a sequence node at line:{},col:{}",
            at.line, at.column
        )));
    }
    let mut patterns = Vec::new();
    for &item in source.written(id) {
        let at = source.position(item);
        let fail = |message: String| {
            DecodeError::Type(vec![UnmarshalError {
                line: at.line,
                message,
            }])
        };
        let found = match source
            .node(source.resolve_alias(item))
            .map(|node| &node.kind)
        {
            Some(NodeKind::Scalar(scalar)) if scalar.tag == Tag::Null => "null",
            Some(NodeKind::Scalar(scalar)) => match compile(&scalar.value) {
                Ok(pattern) => {
                    patterns.push(pattern);
                    continue;
                }
                Err(err) => {
                    return Err(fail(format!(
                        "invalid regular expression {} in \"ignore\" at {at}: {err}",
                        quote(&scalar.value)
                    )));
                }
            },
            Some(NodeKind::Sequence(_)) => "sequence node",
            Some(NodeKind::Mapping(_)) => "mapping node",
            Some(NodeKind::Alias(_)) | None => "alias",
        };
        return Err(fail(format!(
            "\"ignore\" entry must be a string but {found} was found at {at}"
        )));
    }
    Ok(patterns)
}

/// Decodes `python.linter`, which must be the name of a [`PythonLinter`], with case.
fn decode_python_linter(d: &mut Decoder<'_>, id: NodeId) -> Result<PythonLinter, DecodeError> {
    let source = d.source();
    let at = source.position(id);
    let found = match source.node(source.resolve_alias(id)).map(|node| &node.kind) {
        Some(NodeKind::Scalar(scalar)) => {
            if let Some(linter) = PythonLinter::ALL
                .into_iter()
                .find(|linter| linter.name() == &*scalar.value)
            {
                return Ok(linter);
            }
            quote(&scalar.value)
        }
        Some(NodeKind::Sequence(_)) => "sequence node".to_owned(),
        Some(NodeKind::Mapping(_)) => "mapping node".to_owned(),
        Some(NodeKind::Alias(_)) | None => "alias".to_owned(),
    };
    Err(DecodeError::Type(vec![UnmarshalError {
        line: at.line,
        message: format!(
            "\"python.linter\" must be \"auto\", \"ruff\", \"pyflakes\", or \"off\", but {found} \
             was found at {at}"
        ),
    }]))
}

/// Lists the keys under `root` that neither the reference nor workflint reads.
fn unknown_keys(source: &Source<'_>, root: NodeId) -> Vec<UnknownKey> {
    let mut unknown = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |key: NodeId, parent: KeyParent| {
        if seen.insert((key, parent.clone())) {
            unknown.push(UnknownKey {
                key: key_text(source, key).to_owned(),
                parent,
                position: source.position(key),
            });
        }
    };
    for entry in entries(source, root) {
        match key_text(source, entry.key) {
            "self-hosted-runner" => {
                for inner in entries(source, entry.value) {
                    if key_text(source, inner.key) != "labels" {
                        add(inner.key, KeyParent::SelfHostedRunner);
                    }
                }
            }
            "config-variables" | "tool-timeout" => {}
            "python" => {
                for inner in entries(source, entry.value) {
                    if !PYTHON_FIELDS.contains(&key_text(source, inner.key)) {
                        add(inner.key, KeyParent::Python);
                    }
                }
            }
            "shellcheck" => {
                for inner in entries(source, entry.value) {
                    if !SHELLCHECK_FIELDS.contains(&key_text(source, inner.key)) {
                        add(inner.key, KeyParent::Shellcheck);
                    }
                }
            }
            "paths" => {
                for path in entries(source, entry.value) {
                    let glob = key_text(source, path.key);
                    for inner in entries(source, path.value) {
                        if key_text(source, inner.key) != "ignore" {
                            add(inner.key, KeyParent::Path(glob.to_owned()));
                        }
                    }
                }
            }
            _ => add(entry.key, KeyParent::Root),
        }
    }
    unknown
}

/// The text of the key `key`, with aliases resolved.
fn key_text<'s>(source: &'s Source<'_>, key: NodeId) -> &'s str {
    source.go_value(source.resolve_alias(key))
}

/// The entries of the mapping `id`, or of the mapping it refers to: its own entries as written,
/// without `<<` keys, then those of the mappings that its `<<` keys merge in. Each mapping is
/// listed once. A node that is not a mapping has no entries.
fn entries(source: &Source<'_>, id: NodeId) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut visited = BTreeSet::new();
    let mut work = vec![id];
    while let Some(id) = work.pop() {
        let id = source.resolve_alias(id);
        if !source.is_mapping(id) || !visited.insert(id) {
            continue;
        }
        let mut merges = Vec::new();
        for entry in source.written_entries(id) {
            if source.is_merge_key(entry.key) {
                let value = source.resolve_alias(entry.value);
                match source.node(value).map(|node| &node.kind) {
                    Some(NodeKind::Sequence(_)) => merges.extend(source.written(value)),
                    _ => merges.push(value),
                }
            } else {
                out.push(entry);
            }
        }
        work.extend(merges.into_iter().rev());
    }
    out
}
