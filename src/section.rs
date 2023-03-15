use byteorder::{ByteOrder, ReadBytesExt};

use std::io::Read;

/// `perf_file_section`
///
/// A PerfFileSection contains a pointer to another section of the perf file.
/// The header contains three such pointers: for attributes, data and event types.
#[derive(Debug, Clone, Copy)]
pub struct PerfFileSection {
    /// offset from start of file
    pub offset: u64,
    /// size of the section
    pub size: u64,
}

impl PerfFileSection {
    pub const STRUCT_SIZE: u64 = 8 + 8;

    pub fn parse<R: Read, T: ByteOrder>(mut reader: R) -> Result<Self, std::io::Error> {
        let offset = reader.read_u64::<T>()?;
        let size = reader.read_u64::<T>()?;
        Ok(Self { offset, size })
    }
}
