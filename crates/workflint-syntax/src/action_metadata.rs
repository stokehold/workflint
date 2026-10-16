//! The metadata of a local action, from its `action.yml`.

use std::collections::{BTreeMap, BTreeSet};

use workflint_diagnostics::SourceFile;
use workflint_diagnostics::quote::quote;
use workflint_yaml::{NodeId, NodeKind, simple_lowercase};

use crate::MetadataError;
use crate::typed::{DecodeError, Decoder, Fatal, Source};

/// The metadata of a local action, decoded from its `action.yml` as the
/// reference decodes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionMetadata {
    /// `name`. It is empty if the key is missing.
    pub name: String,
    /// `description`. It is empty if the key is missing.
    pub description: String,
    /// `inputs`, keyed by the input name in lowercase.
    pub inputs: BTreeMap<String, ActionInput>,
    /// `outputs`, keyed by the output name in lowercase.
    pub outputs: BTreeMap<String, ActionOutput>,
    /// `runs`.
    pub runs: ActionRuns,
    /// `branding`.
    pub branding: ActionBranding,
}

impl ActionMetadata {
    /// Decodes the `action.yml` in `file`.
    ///
    /// # Errors
    ///
    /// Returns the reference's error text if the file is not valid YAML or
    /// its metadata cannot be decoded.
    ///
    /// The text is what the reference puts after `could not parse action
    /// metadata in "<dir>": `. Several errors are joined as the reference
    /// joins them, with their line breaks removed.
    ///
    /// ```
    /// use workflint_diagnostics::{FileId, SourceFile};
    /// use workflint_syntax::ActionMetadata;
    ///
    /// let text = "name: Greet\ninputs:\n  who:\n    required: yes\n";
    /// let file = SourceFile::new(FileId::new(0), "action.yml", text).unwrap();
    /// let metadata = ActionMetadata::decode(&file).unwrap();
    /// assert!(metadata.inputs["who"].required);
    ///
    /// let file = SourceFile::new(FileId::new(0), "action.yml", "outputs: x\n").unwrap();
    /// let err = ActionMetadata::decode(&file).unwrap_err();
    /// assert_eq!(
    ///     err.message(),
    ///     "line 1: yaml: outputs must be mapping node but scalar node was found at line:1, col:10",
    /// );
    /// ```
    pub fn decode(file: &SourceFile) -> Result<Self, MetadataError> {
        let source = Source::parse(file).map_err(MetadataError::new)?;
        let decoded = source.unmarshal(|d, root| {
            let mut metadata = Self::default();
            if let Some(root) = root {
                decode_metadata(d, root, &mut metadata)?;
            }
            Ok(metadata)
        });
        decoded.map_err(|err| {
            MetadataError::new(match err {
                // The reference unwraps a single error and removes the line
                // breaks of several.
                DecodeError::Type(errors) if errors.len() == 1 => {
                    errors.first().map(ToString::to_string).unwrap_or_default()
                }
                err @ DecodeError::Type(_) => err.to_string().replace('\n', ""),
                DecodeError::Other(message) => message,
            })
        })
    }
}

/// Decodes the root of an `action.yml` into the reference's
/// `ActionMetadata` struct.
fn decode_metadata(
    d: &mut Decoder<'_>,
    id: NodeId,
    metadata: &mut ActionMetadata,
) -> Result<(), Fatal> {
    const FIELDS: &[&str] = &[
        "name",
        "description",
        "inputs",
        "outputs",
        "runs",
        "branding",
    ];
    d.structure(
        id,
        "actionlint.ActionMetadata",
        FIELDS,
        &mut |d, field, value| {
            match field {
                0 => {
                    d.string(value, &mut metadata.name)?;
                }
                1 => {
                    d.string(value, &mut metadata.description)?;
                }
                2 => {
                    if let Some(inputs) = d.custom(value, decode_inputs)? {
                        metadata.inputs = inputs;
                    }
                }
                3 => {
                    if let Some(outputs) = d.custom(value, decode_outputs)? {
                        metadata.outputs = outputs;
                    }
                }
                4 => decode_runs(d, value, &mut metadata.runs)?,
                _ => decode_branding(d, value, &mut metadata.branding)?,
            }
            Ok(())
        },
    )?;
    Ok(())
}

/// The keys of [`InputDefinition`].
const INPUT_FIELDS: &[&str] = &["required", "default", "deprecationMessage"];

/// The reference's `actionInputMetadata`: what it decodes of one input.
#[derive(Default)]
struct InputDefinition {
    required: bool,
    default: Option<String>,
    deprecation_message: String,
}

/// Decodes `inputs`, as the reference's `ActionMetadataInputs.UnmarshalYAML`
/// does, with aliases resolved and `<<` keys merged
/// (docs/differences.md#action-metadata-aliases).
fn decode_inputs(
    d: &mut Decoder<'_>,
    id: NodeId,
) -> Result<BTreeMap<String, ActionInput>, DecodeError> {
    let source = d.source();
    if !source.is_mapping(id) {
        return Err(source.expected_mapping("inputs", id));
    }
    let mut inputs = BTreeMap::new();
    // An unknown key does not stop decoding. The last one is reported.
    let mut unknown_key = None;
    for (name, value) in merged_entries(d, id)? {
        let definition = source.decode(value, |d, value| {
            let mut definition = InputDefinition::default();
            d.structure(
                value,
                "actionlint.actionInputMetadata",
                INPUT_FIELDS,
                &mut |d, field, value| {
                    match field {
                        0 => d.bool(value, &mut definition.required)?,
                        1 => d.optional_string(value, &mut definition.default)?,
                        _ => d.string(value, &mut definition.deprecation_message)?,
                    };
                    Ok(())
                },
            )?;
            Ok(definition)
        })?;
        let id = simple_lowercase(&name).into_owned();
        if inputs.contains_key(&id) {
            return Err(DecodeError::Other(format!(
                "input {} is duplicated",
                quote(&name)
            )));
        }
        let mut deprecated = false;
        let definition_node = source.resolve_alias(value);
        if source.is_mapping(definition_node) {
            for entry in source.written_entries(definition_node) {
                match key_text(d, entry.key) {
                    "deprecationMessage" => deprecated = true,
                    "description" | "required" | "default" => {}
                    key => {
                        unknown_key = Some(format!(
                            "unexpected key {} for definition of input {}",
                            quote(key),
                            quote(&name)
                        ));
                    }
                }
            }
        }
        let deprecation_message =
            deprecated.then(|| definition.deprecation_message.trim().to_owned());
        inputs.insert(
            id,
            ActionInput {
                name,
                required: definition.required && definition.default.is_none(),
                deprecation_message,
            },
        );
    }
    match unknown_key {
        Some(message) => Err(DecodeError::Other(message)),
        None => Ok(inputs),
    }
}

/// Returns the name of a key under `inputs` or in an input definition: the
/// value of the scalar it is or refers to, or the empty string.
fn key_text<'s>(d: &Decoder<'s>, key: NodeId) -> &'s str {
    d.source().go_value(d.source().resolve_alias(key))
}

/// Lists the inputs of the mapping `id`: its own entries in order, except
/// `<<` keys, then the entries that its `<<` keys merge in, which do not
/// override a key that is already there. Aliases are resolved
/// (docs/differences.md#action-metadata-aliases).
fn merged_entries(d: &Decoder<'_>, id: NodeId) -> Result<Vec<(String, NodeId)>, DecodeError> {
    fn collect(
        d: &Decoder<'_>,
        id: NodeId,
        merged: bool,
        seen: &mut BTreeSet<String>,
        expanding: &mut Vec<NodeId>,
        out: &mut Vec<(String, NodeId)>,
    ) -> Result<(), DecodeError> {
        let source = d.source();
        if expanding.contains(&id) {
            let name = source
                .node(id)
                .and_then(|node| node.anchor.as_ref())
                .map_or("", |anchor| anchor.name.as_str());
            return Err(DecodeError::Other(format!(
                "yaml: anchor '{name}' value contains itself"
            )));
        }
        expanding.push(id);
        let mut merge_values = Vec::new();
        for entry in source.written_entries(id) {
            if source.is_merge_key(entry.key) {
                merge_values.push(entry.value);
                continue;
            }
            let name = key_text(d, entry.key).to_owned();
            if seen.insert(name.clone()) || !merged {
                out.push((name, entry.value));
            }
        }
        for merge in merge_values {
            let want_map = || {
                DecodeError::Other(
                    "yaml: map merge requires map or sequence of maps as the value".to_owned(),
                )
            };
            let merge = source.resolve_alias(merge);
            let mappings: Vec<NodeId> = match source.node(merge).map(|node| &node.kind) {
                Some(NodeKind::Mapping(_)) => vec![merge],
                Some(NodeKind::Sequence(_)) => source
                    .written(merge)
                    .iter()
                    .map(|&item| source.resolve_alias(item))
                    .collect(),
                _ => return Err(want_map()),
            };
            if !mappings.iter().all(|&mapping| source.is_mapping(mapping)) {
                return Err(want_map());
            }
            for mapping in mappings {
                collect(d, mapping, true, seen, expanding, out)?;
            }
        }
        expanding.pop();
        Ok(())
    }

    let mut out = Vec::new();
    collect(
        d,
        id,
        false,
        &mut BTreeSet::new(),
        &mut Vec::new(),
        &mut out,
    )?;
    Ok(out)
}

/// Decodes `outputs`, as the reference's `ActionMetadataOutputs.UnmarshalYAML`
/// does: only the keys are read.
fn decode_outputs(
    d: &mut Decoder<'_>,
    id: NodeId,
) -> Result<BTreeMap<String, ActionOutput>, DecodeError> {
    let source = d.source();
    if !source.is_mapping(id) {
        return Err(source.expected_mapping("outputs", id));
    }
    let mut outputs = BTreeMap::new();
    for entry in source.written_entries(id) {
        let name = source.go_value(entry.key).to_owned();
        let id = simple_lowercase(&name).into_owned();
        if outputs.contains_key(&id) {
            return Err(DecodeError::Other(format!(
                "output {} is duplicated",
                quote(&name)
            )));
        }
        outputs.insert(id, ActionOutput { name });
    }
    Ok(outputs)
}

fn decode_runs(d: &mut Decoder<'_>, id: NodeId, runs: &mut ActionRuns) -> Result<(), Fatal> {
    const FIELDS: &[&str] = &[
        "using",
        "main",
        "pre",
        "pre-if",
        "post",
        "post-if",
        "steps",
        "image",
        "pre-entrypoint",
        "entrypoint",
        "post-entrypoint",
        "args",
        "env",
    ];
    d.structure(
        id,
        "actionlint.ActionMetadataRuns",
        FIELDS,
        &mut |d, field, value| {
            let string = match field {
                0 => &mut runs.using,
                1 => &mut runs.main,
                2 => &mut runs.pre,
                3 => &mut runs.pre_if,
                4 => &mut runs.post,
                5 => &mut runs.post_if,
                6 => {
                    d.any_slice(value, &mut runs.has_steps)?;
                    return Ok(());
                }
                7 => &mut runs.image,
                8 => &mut runs.pre_entrypoint,
                9 => &mut runs.entrypoint,
                10 => &mut runs.post_entrypoint,
                11 => {
                    d.any_slice(value, &mut false)?;
                    return Ok(());
                }
                _ => {
                    d.any_map(value)?;
                    return Ok(());
                }
            };
            d.string(value, string)?;
            Ok(())
        },
    )?;
    Ok(())
}

fn decode_branding(
    d: &mut Decoder<'_>,
    id: NodeId,
    branding: &mut ActionBranding,
) -> Result<(), Fatal> {
    d.structure(
        id,
        "actionlint.ActionMetadataBranding",
        &["icon", "color"],
        &mut |d, field, value| {
            let string = if field == 0 {
                &mut branding.icon
            } else {
                &mut branding.color
            };
            d.string(value, string)?;
            Ok(())
        },
    )?;
    Ok(())
}

/// One input of a local action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionInput {
    /// The input name as written.
    pub name: String,
    /// Whether `required` is true and the input has no `default`.
    pub required: bool,
    /// The `deprecationMessage` with surrounding white space removed, if the
    /// input has the key. It is empty if the value is null or blank.
    pub deprecation_message: Option<String>,
}

/// One output of a local action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionOutput {
    /// The output name as written.
    pub name: String,
}

/// The `runs` section of a local action. A missing key is an empty string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionRuns {
    /// `using`.
    pub using: String,
    /// `main`, for a JavaScript action.
    pub main: String,
    /// `pre`, for a JavaScript action.
    pub pre: String,
    /// `pre-if`, for a JavaScript action.
    pub pre_if: String,
    /// `post`, for a JavaScript action.
    pub post: String,
    /// `post-if`, for a JavaScript action.
    pub post_if: String,
    /// `image`, for a Docker action.
    pub image: String,
    /// `pre-entrypoint`, for a Docker action.
    pub pre_entrypoint: String,
    /// `entrypoint`, for a Docker action.
    pub entrypoint: String,
    /// `post-entrypoint`, for a Docker action.
    pub post_entrypoint: String,
    /// Whether `steps` is a sequence, for a composite action. The
    /// reference checks only this, so the steps themselves are not kept.
    pub has_steps: bool,
}

/// The `branding` section of a local action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionBranding {
    /// `icon`.
    pub icon: String,
    /// `color`.
    pub color: String,
}
