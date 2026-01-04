use std::io::{Read, Seek, SeekFrom};

use byteorder::{ByteOrder, ReadBytesExt};

use super::features::FeatureSet;
use super::section::PerfFileSection;

const MAGIC_SIZE: usize = 8;

/// `perf_header`
///
/// The magic number identifies the perf file and the version. Current perf versions
/// use PERFILE2. Old perf versions generated a version 1 format (PERFFILE). Version 1
/// is not described here. The magic number also identifies the endian. When the
/// magic value is 64bit byte swapped compared the file is in non-native
/// endian.
#[derive(Debug, Clone, Copy)]
pub struct PerfHeader {
    pub magic: [u8; MAGIC_SIZE],
    /// size of the header
    #[allow(dead_code)]
    pub header_size: u64,
    /// size of an attribute in attrs
    pub attr_size: u64,
    pub attr_section: PerfFileSection,
    pub data_section: PerfFileSection,
    pub event_types_section: PerfFileSection,
    /// Feature flags
    pub features: FeatureSet,
}

impl PerfHeader {
    pub fn parse<R: Read + Seek>(mut reader: R) -> Result<Self, std::io::Error> {
        let mut magic = [0; MAGIC_SIZE];
        reader.read_exact(&mut magic)?;

        if magic[0] == b'P' {
            Self::parse_impl::<R, byteorder::LittleEndian>(reader, magic)
        } else {
            Self::parse_impl::<R, byteorder::BigEndian>(reader, magic)
        }
    }

    fn parse_impl<R: Read + Seek, T: ByteOrder>(
        mut reader: R,
        magic: [u8; MAGIC_SIZE],
    ) -> Result<Self, std::io::Error> {
        let header_size = reader.read_u64::<T>()?;
        let attr_size = reader.read_u64::<T>()?;
        let attr_section = PerfFileSection::parse::<_, T>(&mut reader)?;
        let data_section = PerfFileSection::parse::<_, T>(&mut reader)?;
        let event_types_section = PerfFileSection::parse::<_, T>(&mut reader)?;
        let features = FeatureSet([
            reader.read_u64::<T>()?,
            reader.read_u64::<T>()?,
            reader.read_u64::<T>()?,
            reader.read_u64::<T>()?,
        ]);

        // Skip any additional header bytes from newer formats.
        let known_size = std::mem::size_of::<Self>() as u64;
        if header_size > known_size {
            let extra_bytes = header_size - known_size;
            reader.seek(SeekFrom::Current(extra_bytes as i64))?;
        }

        Ok(Self {
            magic,
            header_size,
            attr_size,
            attr_section,
            data_section,
            event_types_section,
            features,
        })
    }
}

/// `perf_pipe_file_header`
///
/// A minimal header used in pipe mode to avoid seeking.
/// In pipe mode, metadata is embedded in the stream via synthesized events
/// (PERF_RECORD_HEADER_ATTR, PERF_RECORD_HEADER_FEATURE) instead of using
/// file sections.
#[derive(Debug, Clone, Copy)]
pub struct PerfPipeHeader {
    pub magic: [u8; MAGIC_SIZE],
    /// size of the header (should be 16)
    #[allow(dead_code)]
    pub size: u64,
}

impl PerfPipeHeader {
    pub fn parse<R: Read>(mut reader: R) -> Result<Self, std::io::Error> {
        let mut magic = [0; MAGIC_SIZE];
        reader.read_exact(&mut magic)?;

        let size = if magic[0] == b'P' {
            reader.read_u64::<byteorder::LittleEndian>()?
        } else {
            reader.read_u64::<byteorder::BigEndian>()?
        };

        // Skip any additional header bytes from newer formats.
        // Use io::copy to sink instead of seek since pipe mode doesn't support seeking.
        let known_size = std::mem::size_of::<Self>() as u64;
        if size > known_size {
            let extra_bytes = size - known_size;
            std::io::copy(&mut reader.by_ref().take(extra_bytes), &mut std::io::sink())?;
        }

        Ok(Self { magic, size })
    }
}
