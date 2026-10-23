use smol_str::SmolStr;

use super::{Encoding, Mark, ScalarStyle};

/// The token structure.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub(crate) struct Token {
    /// The token type.
    pub(crate) data: TokenData,
    /// The beginning of the token.
    pub(crate) start_mark: Mark,
    /// The end of the token.
    pub(crate) end_mark: Mark,
}

#[derive(Debug, PartialEq)]
pub(crate) enum TokenData {
    /// A STREAM-START token.
    StreamStart {
        /// The stream encoding.
        encoding: Encoding,
    },
    /// A STREAM-END token.
    StreamEnd,
    /// A VERSION-DIRECTIVE token.
    VersionDirective {
        /// The major version number.
        major: i32,
        /// The minor version number.
        minor: i32,
    },
    /// A TAG-DIRECTIVE token.
    TagDirective {
        /// The tag handle.
        handle: String,
        /// The tag prefix.
        prefix: String,
    },
    /// A DOCUMENT-START token.
    DocumentStart,
    /// A DOCUMENT-END token.
    DocumentEnd,
    /// A BLOCK-SEQUENCE-START token.
    BlockSequenceStart,
    /// A BLOCK-MAPPING-START token.
    BlockMappingStart,
    /// A BLOCK-END token.
    BlockEnd,
    /// A FLOW-SEQUENCE-START token.
    FlowSequenceStart,
    /// A FLOW-SEQUENCE-END token.
    FlowSequenceEnd,
    /// A FLOW-MAPPING-START token.
    FlowMappingStart,
    /// A FLOW-MAPPING-END token.
    FlowMappingEnd,
    /// A BLOCK-ENTRY token.
    BlockEntry,
    /// A FLOW-ENTRY token.
    FlowEntry,
    /// A KEY token.
    Key,
    /// A VALUE token.
    Value,
    /// An ALIAS token.
    Alias {
        /// The alias value.
        value: String,
    },
    /// An ANCHOR token.
    Anchor {
        /// The anchor value.
        value: String,
    },
    /// A TAG token.
    Tag {
        /// The tag handle.
        handle: String,
        /// The tag suffix.
        suffix: String,
    },
    /// A SCALAR token.
    Scalar {
        /// The scalar value.
        value: SmolStr,
        /// The scalar style.
        style: ScalarStyle,
    },
}
