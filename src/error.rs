use std::io;

/// The error type used in this crate.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The data slice was not big enough to read the struct, or we
    /// were trying to follow an invalid offset to somewhere outside
    /// of the data bounds.
    #[error("Read error: {0}")]
    Read(#[from] ReadError),

    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    #[error("Did not recognize magic value {0:?}")]
    UnrecognizedMagicValue([u8; 8]),

    #[error("Section size did not fit into usize")]
    SectionSizeTooBig,

    #[error("The file declares no perf event attributes, so samples cannot be parsed")]
    NoAttributes,

    #[error("The file contains multiple events but attr {0} does not specify IDENTIFIER")]
    NoIdentifierDespiteMultiEvent(usize),

    #[error("The file contains multiple events but attr {0} does not agree with attr zero about SAMPLE_ID_ALL")]
    InconsistentSampleIdAllWithMultiEvent(usize),

    #[error("The section wasn't big enough to contain the u32 string length")]
    NotEnoughSpaceForStringLen,

    #[error("The section wasn't big enough to contain the u32 string list length")]
    NotEnoughSpaceForStringListLen,

    #[error("The feature section wasn't big enough")]
    FeatureSectionTooSmall,

    #[error("No event types found in the simpleperf meta info section")]
    NoEventTypesInSimpleperfMetaInfo,

    #[error("The indicated string length wouldn't fit in the indicated section size")]
    StringLengthTooLong,

    #[error("The indicated string list length wouldn't fit into usize")]
    StringListLengthBiggerThanUsize,

    #[error("The indicated string length wouldn't fit into usize")]
    StringLengthBiggerThanUsize,

    #[error("The string was not valid utf-8")]
    StringUtf8,

    #[error("The specified size in the perf event header was smaller than the header itself")]
    InvalidPerfEventSize,
}

impl From<std::str::Utf8Error> for Error {
    fn from(_: std::str::Utf8Error) -> Self {
        Error::StringUtf8
    }
}

/// This error indicates that the data slice was not large enough to
/// read the respective item.
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadError {
    #[error("Could not read PerfHeader")]
    PerfHeader,

    #[error("Could not read FeatureSection")]
    FeatureSection,

    #[error("Could not read BuildIdSection")]
    BuildIdSection,

    #[error("Could not read StringLen")]
    StringLen,

    #[error("Could not read String")]
    String,

    #[error("Could not read NrCpus")]
    NrCpus,

    #[error("Could not read AttrsSection")]
    AttrsSection,

    #[error("Could not read PerfEventAttr")]
    PerfEventAttr,

    #[error("Could not read PerfEventHeader")]
    PerfEventHeader,

    #[error("Could not read PerfEvent data")]
    PerfEventData,
}
