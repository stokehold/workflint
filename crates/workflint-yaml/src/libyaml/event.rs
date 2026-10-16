use super::{
    CommentKind, Encoding, MappingStyle, Mark, ScalarStyle, SequenceStyle, TagDirective,
    VersionDirective,
};

/// One parsing event, with the input range it covers.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct Event {
    /// The event data.
    pub data: EventData,
    /// The beginning of the event.
    pub start_mark: Mark,
    /// The end of the event.
    pub end_mark: Mark,
    /// Where the anchor token and the content of a node are, a workflint
    /// addition.
    pub node_marks: NodeMarks,
}

/// The marks inside a node event's properties, a workflint addition.
///
/// libyaml starts a node event at its first property, the `&anchor` or
/// `!tag` token, and drops the marks of the other tokens. The tree needs
/// them for the span of the anchor and for the start of the content.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct NodeMarks {
    /// The start and end of the `&anchor` token, if the node has an anchor.
    pub anchor: Option<(Mark, Mark)>,
    /// Where the node's content starts after its anchor and tag, if it has
    /// either. For a node with properties and no content, such as `&a` alone,
    /// this is the end of the properties. `None` means the content starts at
    /// the event's `start_mark`.
    pub content: Option<Mark>,
}

/// The kind of an [`Event`] and its data.
#[derive(Debug, PartialEq)]
pub enum EventData {
    /// The stream parameters (for `YAML_STREAM_START_EVENT`).
    StreamStart {
        /// The document encoding.
        encoding: Encoding,
    },
    /// The end of the input.
    StreamEnd,
    /// The document parameters (for `YAML_DOCUMENT_START_EVENT`).
    DocumentStart {
        /// The version directive.
        version_directive: Option<VersionDirective>,
        /// The tag directives list.
        tag_directives: Vec<TagDirective>,
        /// Is the document indicator implicit?
        implicit: bool,
    },
    /// The document end parameters (for `YAML_DOCUMENT_END_EVENT`).
    DocumentEnd {
        /// Is the document end indicator implicit?
        implicit: bool,
    },
    /// The alias parameters (for `YAML_ALIAS_EVENT`).
    Alias {
        /// The anchor.
        anchor: String,
    },
    /// The scalar parameters (for `YAML_SCALAR_EVENT`).
    Scalar {
        /// The anchor.
        anchor: Option<String>,
        /// The tag.
        tag: Option<String>,
        /// The scalar value.
        value: String,
        /// Is the tag optional for the plain style?
        plain_implicit: bool,
        /// Is the tag optional for any non-plain style?
        quoted_implicit: bool,
        /// The scalar style.
        style: ScalarStyle,
    },
    /// The sequence parameters (for `YAML_SEQUENCE_START_EVENT`).
    SequenceStart {
        /// The anchor.
        anchor: Option<String>,
        /// The tag.
        tag: Option<String>,
        /// Is the tag optional?
        implicit: bool,
        /// The sequence style.
        style: SequenceStyle,
    },
    /// The end of a sequence.
    SequenceEnd,
    /// The mapping parameters (for `YAML_MAPPING_START_EVENT`).
    MappingStart {
        /// The anchor.
        anchor: Option<String>,
        /// The tag.
        tag: Option<String>,
        /// Is the tag optional?
        implicit: bool,
        /// The mapping style.
        style: MappingStyle,
    },
    /// The end of a mapping.
    MappingEnd,
    /// A comment, a workflint addition.
    ///
    /// The event's marks span the comment from its `#` to the end of its
    /// line, without the line break, so `start_mark.index..end_mark.index`
    /// slices the comment text from the source. The parser reports comments
    /// in source order, each one before the first other event that starts
    /// after it.
    Comment {
        /// Whether code precedes the comment on its line.
        kind: CommentKind,
    },
}
