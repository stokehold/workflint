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
