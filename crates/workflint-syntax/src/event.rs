//! The events of `on:`.

use std::sync::Arc;

use workflint_diagnostics::Span;

use crate::{Field, Keyed, Str, Template, Value};

/// One event of `on:`.
///
/// Every event has a `name`, the string that names it: the key in the
/// mapping form of `on:`, or the string or sequence item in the other
/// forms. Its span is the reference's event `Pos`. Every event also has a
/// `body`, the span of the value under its key in the mapping form, which
/// is `None` in the other forms.
// A workflow has a handful of events, so boxing the large webhook variant
// would add an allocation per event and save little memory.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Event<'a> {
    /// A webhook event, such as `push` or `pull_request`.
    Webhook(WebhookEvent<'a>),
    /// `schedule`.
    Schedule(ScheduleEvent<'a>),
    /// `workflow_dispatch`.
    WorkflowDispatch(WorkflowDispatchEvent<'a>),
    /// `repository_dispatch`.
    RepositoryDispatch(RepositoryDispatchEvent<'a>),
    /// `workflow_call`.
    WorkflowCall(WorkflowCallEvent<'a>),
    /// `image_version`.
    ImageVersion(ImageVersionEvent<'a>),
}

impl<'a> Event<'a> {
    /// Returns the string that names the event. Its value is the
    /// reference's `EventName()`.
    #[must_use]
    pub fn name(&self) -> &Str<'a> {
        match self {
            Self::Webhook(event) => &event.name,
            Self::Schedule(event) => &event.name,
            Self::WorkflowDispatch(event) => &event.name,
            Self::RepositoryDispatch(event) => &event.name,
            Self::WorkflowCall(event) => &event.name,
            Self::ImageVersion(event) => &event.name,
        }
    }

    /// Returns the span of the value under the event's key, or `None` when
    /// the event is named in a string or a sequence.
    #[must_use]
    pub fn body(&self) -> Option<Span> {
        match self {
            Self::Webhook(event) => event.body,
            Self::Schedule(event) => Some(event.body),
            Self::WorkflowDispatch(event) => event.body,
            Self::RepositoryDispatch(event) => event.body,
            Self::WorkflowCall(event) => event.body,
            Self::ImageVersion(event) => event.body,
        }
    }
}

/// A webhook event and its filters.
///
/// Every event that is not one of the other [`Event`] variants is a webhook
/// event, including unknown names. The parser does not check which filters
/// an event allows.
///
/// Each filter is a string or a sequence of strings. A string becomes a
/// list of one. The reference's `WebhookEventFilter.Name` is the filter
/// field's key.
#[derive(Debug, Clone, PartialEq)]
pub struct WebhookEvent<'a> {
    /// The event name as written, the reference's `Hook`.
    pub name: Str<'a>,
    /// The span of the value under the event's key, if any.
    pub body: Option<Span>,
    /// `types`.
    pub types: Field<'a, Arc<[Template<'a>]>>,
    /// `branches`.
    pub branches: Field<'a, Arc<[Template<'a>]>>,
    /// `branches-ignore`.
    pub branches_ignore: Field<'a, Arc<[Template<'a>]>>,
    /// `tags`.
    pub tags: Field<'a, Arc<[Template<'a>]>>,
    /// `tags-ignore`.
    pub tags_ignore: Field<'a, Arc<[Template<'a>]>>,
    /// `paths`.
    pub paths: Field<'a, Arc<[Template<'a>]>>,
    /// `paths-ignore`.
    pub paths_ignore: Field<'a, Arc<[Template<'a>]>>,
    /// `workflows`.
    pub workflows: Field<'a, Arc<[Template<'a>]>>,
}

/// `schedule`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleEvent<'a> {
    /// The string `schedule`.
    pub name: Str<'a>,
    /// The span of the sequence under the event's key. The parser drops a
    /// `schedule` with no sequence, so every `schedule` has one.
    pub body: Span,
    /// The items of the sequence whose `cron` is valid, in source order.
    /// The parser drops the other items, as the reference does.
    pub schedules: Arc<[ScheduleEntry<'a>]>,
}

/// One item of `schedule`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleEntry<'a> {
    /// The span of the item.
    pub span: Span,
    /// `cron`. It is always valid.
    pub cron: Field<'a, Template<'a>>,
    /// `timezone`.
    pub timezone: Field<'a, Template<'a>>,
}

/// `workflow_dispatch`.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDispatchEvent<'a> {
    /// The string `workflow_dispatch`.
    pub name: Str<'a>,
    /// The span of the value under the event's key, if any.
    pub body: Option<Span>,
    /// `inputs`, keyed by input name. The reference's `DispatchInput.Name`
    /// is the entry's key.
    pub inputs: Field<'a, Keyed<'a, WorkflowDispatchInput<'a>>>,
}

/// One input of `workflow_dispatch`, the reference's `DispatchInput`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkflowDispatchInput<'a> {
    /// `description`. It may be empty.
    pub description: Field<'a, Template<'a>>,
    /// `required`.
    pub required: Value<'a, bool>,
    /// `default`. It may be empty.
    pub default: Field<'a, Template<'a>>,
    /// `type`. The reference's `WorkflowDispatchEventInputTypeNone` is
    /// [`Field::Absent`] or [`Field::Invalid`].
    pub input_type: Field<'a, WorkflowDispatchInputType>,
    /// `options`.
    pub options: Field<'a, Arc<[Template<'a>]>>,
}

/// The `type` of a `workflow_dispatch` input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkflowDispatchInputType {
    /// `string`.
    String,
    /// `number`.
    Number,
    /// `boolean`.
    Boolean,
    /// `choice`.
    Choice,
    /// `environment`.
    Environment,
}

/// `repository_dispatch`.
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryDispatchEvent<'a> {
    /// The string `repository_dispatch`.
    pub name: Str<'a>,
    /// The span of the value under the event's key, if any.
    pub body: Option<Span>,
    /// `types`. A string becomes a list of one.
    pub types: Field<'a, Arc<[Template<'a>]>>,
}

/// `workflow_call`, which makes the workflow a reusable workflow.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowCallEvent<'a> {
    /// The string `workflow_call`.
    pub name: Str<'a>,
    /// The span of the value under the event's key, if any.
    pub body: Option<Span>,
    /// `inputs`, keyed by input name. The entry's folded key is the
    /// reference's `WorkflowCallEventInput.ID`.
    pub inputs: Field<'a, Keyed<'a, WorkflowCallEventInput<'a>>>,
    /// `secrets`, keyed by secret name.
    ///
    /// [`Field::Absent`] means the key is not written, which the
    /// reference's `nil` map records, and a null value is an empty
    /// collection. A value that is not a mapping is [`Field::Invalid`],
    /// where the reference has an empty map.
    pub secrets: Field<'a, Keyed<'a, WorkflowCallEventSecret<'a>>>,
    /// `outputs`, keyed by output name.
    pub outputs: Field<'a, Keyed<'a, WorkflowCallEventOutput<'a>>>,
}

/// One input of `workflow_call`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkflowCallEventInput<'a> {
    /// `description`. It may be empty.
    pub description: Field<'a, Template<'a>>,
    /// `default`. It may be empty.
    pub default: Field<'a, Template<'a>>,
    /// `required`.
    pub required: Value<'a, bool>,
    /// `type`. The reference's `WorkflowCallEventInputTypeInvalid` is
    /// [`Field::Absent`] or [`Field::Invalid`].
    pub input_type: Field<'a, WorkflowCallInputType>,
}

impl WorkflowCallEventInput<'_> {
    /// Returns whether `required` is the literal `true`, the reference's
    /// `IsRequired`.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.required.literal() == Some(&true)
    }
}

/// The `type` of a `workflow_call` input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkflowCallInputType {
    /// `boolean`.
    Boolean,
    /// `number`.
    Number,
    /// `string`.
    String,
}

/// One secret of `workflow_call`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkflowCallEventSecret<'a> {
    /// `description`. It may be empty.
    pub description: Field<'a, Template<'a>>,
    /// `required`.
    pub required: Value<'a, bool>,
}

/// One output of `workflow_call`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkflowCallEventOutput<'a> {
    /// `description`. It may be empty.
    pub description: Field<'a, Template<'a>>,
    /// `value`.
    pub value: Field<'a, Template<'a>>,
}

/// `image_version`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageVersionEvent<'a> {
    /// The string `image_version`.
    pub name: Str<'a>,
    /// The span of the value under the event's key, if any.
    pub body: Option<Span>,
    /// `names`.
    pub names: Field<'a, Arc<[Template<'a>]>>,
    /// `versions`.
    pub versions: Field<'a, Arc<[Template<'a>]>>,
}
