/// The error type used for jitdump parsing.
#[derive(thiserror::Error, Debug)]
pub enum JitDumpError {
    #[error("The file does not contain enough bytes to parse the jitdump header.")]
    NotEnoughBytesForHeader,

    #[error("Invalid jitdump header size: {0}")]
    InvalidHeaderSize(u32),

    #[error("The file does not appear to be a jitdump file, due to unexpected magic bytes: {:02x} {:02x} {:02x} {:02x}", .0[0], .0[1], .0[2], .0[3])]
    InvalidMagicBytes([u8; 4]),

    #[error("The jitdump file has an unrecognized version: {0}")]
    UnrecognizedVersion(u32),

    #[error("Failed to read from the jitdump file: {0}")]
    Io(#[from] std::io::Error),
}
