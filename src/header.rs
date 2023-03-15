use std::io::Read;

use byteorder::{ByteOrder, ReadBytesExt};

use super::features::FeatureSet;
use super::section::PerfFileSection;

/// `perf_header`
///
/// The magic number identifies the perf file and the version. Current perf versions
/// use PERFILE2. Old perf versions generated a version 1 format (PERFFILE). Version 1
/// is not described here. The magic number also identifies the endian. When the
/// magic value is 64bit byte swapped compared the file is in non-native
/// endian.
#[derive(Debug, Clone, Copy)]
pub struct PerfHeader {
    pub magic: [u8; 8],
    /// size of the header
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
    pub fn parse<R: Read>(mut reader: R) -> Result<Self, std::io::Error> {
        let mut magic = [0; 8];
        reader.read_exact(&mut magic)?;

        if magic[0] == b'P' {
            Self::parse_impl::<R, byteorder::LittleEndian>(reader, magic)
        } else {
            Self::parse_impl::<R, byteorder::BigEndian>(reader, magic)
        }
    }

    fn parse_impl<R: Read, T: ByteOrder>(
        mut reader: R,
        magic: [u8; 8],
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
