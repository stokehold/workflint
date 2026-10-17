//! The metadata of a local reusable workflow, from its `on.workflow_call`
//! section.

use std::collections::BTreeMap;

use workflint_diagnostics::SourceFile;
use workflint_yaml::{NodeId, NodeKind, simple_lowercase};

use crate::typed::{DecodeError, Decoder, Fatal, Source};
use crate::{Field, MetadataError, WorkflowCallEvent, WorkflowCallInputType};

/// The inputs, outputs, and secrets of a local reusable workflow, decoded
/// from its `on.workflow_call` section as the reference decodes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReusableWorkflowMetadata {
    /// `inputs`, keyed by the input name in lowercase.
    pub inputs: BTreeMap<String, ReusableWorkflowInput>,
    /// `outputs`, keyed by the output name in lowercase.
    pub outputs: BTreeMap<String, ReusableWorkflowOutput>,
    /// `secrets`, keyed by the secret name in lowercase.
    pub secrets: BTreeMap<String, ReusableWorkflowSecret>,
}

impl ReusableWorkflowMetadata {
    /// Decodes the `on.workflow_call` section of the workflow in `file`.
    ///
    /// # Errors
    ///
    /// Returns the reference's error text if the file is not valid YAML,
    /// has no `on:` or no `workflow_call` event, or its `workflow_call`
    /// section cannot be decoded.
    ///
    /// The text is what the reference puts after `error while parsing
    /// reusable workflow "<spec>": `, with line breaks replaced by spaces.
    /// The reference finds `on:` and the `workflow_call` event its own way:
    /// the `on` key is case-sensitive, the event name is not, and an alias
    /// as `on:` or as an event is not resolved.
    ///
    /// ```
    /// use workflint_diagnostics::{FileId, SourceFile};
    /// use workflint_syntax::ReusableWorkflowMetadata;
    ///
    /// let text = "on:\n  workflow_call:\n    secrets:\n      TOKEN:\n        required: true\n";
    /// let file = SourceFile::new(FileId::new(0), "reusable.yml", text).unwrap();
    /// let metadata = ReusableWorkflowMetadata::decode(&file).unwrap();
    /// assert!(metadata.secrets["token"].required);
    ///
    /// let file = SourceFile::new(FileId::new(0), "ci.yml", "on: push\n").unwrap();
    /// let err = ReusableWorkflowMetadata::decode(&file).unwrap_err();
    /// assert_eq!(
    ///     err.message(),
    ///     r#""workflow_call" event trigger is not found in "on:" at line:1, column:5"#,
    /// );
    /// ```
    pub fn decode(file: &SourceFile) -> Result<Self, MetadataError> {
        let error = |err: DecodeError| MetadataError::new(err.to_string().replace('\n', " "));
        let source = Source::parse(file).map_err(MetadataError::new)?;
        let on = source
            .unmarshal(|d, root| {
                let mut on = None;
                if let Some(root) = root {
                    d.structure(root, "actionlint.workflow", &["on"], &mut |d, _, value| {
                        on = Some(d.raw(value)?);
                        Ok(())
                    })?;
                }
                Ok(on)
            })
            .map_err(error)?;
        let Some(on) = on else {
            return Err(MetadataError::new("\"on:\" is not found".to_owned()));
        };
        let is_workflow_call =
            |id: NodeId| simple_lowercase(source.go_value(id)) == "workflow_call";
        match source.node(on).map(|node| &node.kind) {
            Some(NodeKind::Mapping(_)) => {
                if let Some(entry) = source
                    .written_entries(on)
                    .find(|entry| is_workflow_call(entry.key))
                {
                    return source
                        .decode(entry.value, decode_workflow_call)
                        .map_err(error);
                }
            }
            Some(NodeKind::Scalar(_)) if is_workflow_call(on) => return Ok(Self::default()),
            Some(NodeKind::Sequence(_))
                if source
                    .written(on)
                    .iter()
                    .any(|&item| is_workflow_call(item)) =>
            {
                return Ok(Self::default());
            }
            _ => {}
        }
        let position = source.position(on);
        Err(MetadataError::new(format!(
            "\"workflow_call\" event trigger is not found in \"on:\" at line:{}, column:{}",
            position.line, position.column
        )))
    }

    /// Converts the `workflow_call` event of a parsed workflow, as the
    /// reference's `LocalReusableWorkflowCache.WriteWorkflowCallEvent` does.
    ///
    /// A linted reusable workflow gives its metadata this way, so later
    /// callers in the run do not read its file. An invalid `inputs`,
    /// `outputs`, or `secrets`, which the parser has reported, has no
    /// entries. An input is required only if `required` is the literal
    /// `true` and `default` is not written.
    #[must_use]
    pub fn from_event(event: &WorkflowCallEvent<'_>) -> Self {
        let inputs = event.inputs.value().into_iter().flatten();
        let outputs = event.outputs.value().into_iter().flatten();
        let secrets = event.secrets.value().into_iter().flatten();
        Self {
            inputs: inputs
                .map(|entry| {
                    let input = &entry.value;
                    let metadata = ReusableWorkflowInput {
                        name: entry.key.value().to_owned(),
                        required: input.is_required() && matches!(input.default, Field::Absent),
                        input_type: input.input_type.value().copied(),
                    };
                    (entry.folded().to_owned(), metadata)
                })
                .collect(),
            outputs: outputs
                .map(|entry| {
                    let name = entry.key.value().to_owned();
                    (entry.folded().to_owned(), ReusableWorkflowOutput { name })
                })
                .collect(),
            secrets: secrets
                .map(|entry| {
                    let secret = ReusableWorkflowSecret {
                        name: entry.key.value().to_owned(),
                        required: entry.value.required.literal() == Some(&true),
                    };
                    (entry.folded().to_owned(), secret)
                })
                .collect(),
        }
    }
}

/// Decodes the value of `workflow_call` into the reference's
/// `ReusableWorkflowMetadata` struct.
fn decode_workflow_call(
    d: &mut Decoder<'_>,
    id: NodeId,
) -> Result<ReusableWorkflowMetadata, Fatal> {
    let mut metadata = ReusableWorkflowMetadata::default();
    d.structure(
        id,
        "actionlint.ReusableWorkflowMetadata",
        &["inputs", "outputs", "secrets"],
        &mut |d, field, value| {
            match field {
                0 => {
                    if let Some(inputs) = d.custom(value, decode_inputs)? {
                        metadata.inputs = inputs;
                    }
                }
                1 => {
                    if let Some(outputs) = d.custom(value, decode_outputs)? {
                        metadata.outputs = outputs;
                    }
                }
                _ => {
                    if let Some(secrets) = d.custom(value, decode_secrets)? {
                        metadata.secrets = secrets;
                    }
                }
            }
            Ok(())
        },
    )?;
    Ok(metadata)
}

/// The reference's `metadata` struct in
/// `ReusableWorkflowMetadataInput.UnmarshalYAML`.
#[derive(Default)]
struct InputDefinition {
    required: bool,
    default: Option<String>,
    input_type: String,
}

/// Decodes `inputs`, as `ReusableWorkflowMetadataInputs.UnmarshalYAML`
/// does. A later input replaces an earlier one with the same lowercase
/// name.
fn decode_inputs(
    d: &mut Decoder<'_>,
    id: NodeId,
) -> Result<BTreeMap<String, ReusableWorkflowInput>, DecodeError> {
    let source = d.source();
    if !source.is_mapping(id) {
        return Err(source.expected_mapping("on.workflow_call.inputs", id));
    }
    let mut inputs = BTreeMap::new();
    for entry in source.written_entries(id) {
        // `ReusableWorkflowMetadataInput` has an `UnmarshalYAML` method,
        // which decodes the input with another decoder.
        let input = source.decode(entry.value, |d, value| {
            d.custom(value, |d, value| {
                d.source().decode(value, |d, value| {
                    let mut definition = InputDefinition::default();
                    d.structure(
                        value,
                        "actionlint.metadata",
                        &["required", "default", "type"],
                        &mut |d, field, value| {
                            match field {
                                0 => d.bool(value, &mut definition.required)?,
                                1 => d.optional_string(value, &mut definition.default)?,
                                _ => d.string(value, &mut definition.input_type)?,
                            };
                            Ok(())
                        },
                    )?;
                    Ok(definition)
                })
            })
        })?;
        let name = source.go_value(entry.key).to_owned();
        let input = input.map_or_else(
            || ReusableWorkflowInput {
                name: name.clone(),
                ..ReusableWorkflowInput::default()
            },
            |definition| ReusableWorkflowInput {
                name: name.clone(),
                required: definition.required && definition.default.is_none(),
                input_type: match definition.input_type.as_str() {
                    "boolean" => Some(WorkflowCallInputType::Boolean),
                    "number" => Some(WorkflowCallInputType::Number),
                    "string" => Some(WorkflowCallInputType::String),
                    _ => None,
                },
            },
        );
        inputs.insert(simple_lowercase(&name).into_owned(), input);
    }
    Ok(inputs)
}

/// Decodes `outputs`, as `ReusableWorkflowMetadataOutputs.UnmarshalYAML`
/// does: only the keys are read.
fn decode_outputs(
    d: &mut Decoder<'_>,
    id: NodeId,
) -> Result<BTreeMap<String, ReusableWorkflowOutput>, DecodeError> {
    let source = d.source();
    if !source.is_mapping(id) {
        return Err(source.expected_mapping("on.workflow_call.outputs", id));
    }
    Ok(source
        .written_entries(id)
        .map(|entry| {
            let name = source.go_value(entry.key).to_owned();
            (
                simple_lowercase(&name).into_owned(),
                ReusableWorkflowOutput { name },
            )
        })
        .collect())
}

/// Decodes `secrets`, as `ReusableWorkflowMetadataSecrets.UnmarshalYAML`
/// does.
fn decode_secrets(
    d: &mut Decoder<'_>,
    id: NodeId,
) -> Result<BTreeMap<String, ReusableWorkflowSecret>, DecodeError> {
    let source = d.source();
    if !source.is_mapping(id) {
        return Err(source.expected_mapping("on.workflow_call.secrets", id));
    }
    let mut secrets = BTreeMap::new();
    for entry in source.written_entries(id) {
        let required = source.decode(entry.value, |d, value| {
            let mut required = false;
            // The struct also has a `Name` field, which a `name` key sets
            // before the reference replaces it with the key.
            d.structure(
                value,
                "actionlint.ReusableWorkflowMetadataSecret",
                &["name", "required"],
                &mut |d, field, value| {
                    if field == 0 {
                        d.string(value, &mut String::new())?;
                    } else {
                        d.bool(value, &mut required)?;
                    }
                    Ok(())
                },
            )?;
            Ok(required)
        })?;
        let name = source.go_value(entry.key).to_owned();
        secrets.insert(
            simple_lowercase(&name).into_owned(),
            ReusableWorkflowSecret { name, required },
        );
    }
    Ok(secrets)
}

/// One input of a reusable workflow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReusableWorkflowInput {
    /// The input name as written.
    pub name: String,
    /// Whether `required` is true and the input has no `default`.
    pub required: bool,
    /// `type`, or `None` if it is missing or not one of the three types,
    /// which the reference types as `any`.
    pub input_type: Option<WorkflowCallInputType>,
}

/// One output of a reusable workflow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReusableWorkflowOutput {
    /// The output name as written.
    pub name: String,
}

/// One secret of a reusable workflow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReusableWorkflowSecret {
    /// The secret name as written.
    pub name: String,
    /// `required`.
    pub required: bool,
}
