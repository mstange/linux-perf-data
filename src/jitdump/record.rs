use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linux_perf_event_reader::{Endianness, RawData};

use super::records::*;

/// The record type of a jitdump record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JitDumpRecordType(pub u32);

impl JitDumpRecordType {
    pub const JIT_CODE_LOAD: Self = Self(0);
    pub const JIT_CODE_MOVE: Self = Self(1);
    pub const JIT_CODE_DEBUG_INFO: Self = Self(2);
    pub const JIT_CODE_CLOSE: Self = Self(3);
    pub const JIT_CODE_UNWINDING_INFO: Self = Self(4);
}

/// The header which is at the start of every jitdump record.
#[derive(Debug, Clone)]
pub struct JitDumpRecordHeader {
    /// The record type.
    pub record_type: JitDumpRecordType,
    /// The size in bytes of the record including the header.
    pub total_size: u32,
    /// A timestamp of when the record was created.
    pub timestamp: u64,
}

impl JitDumpRecordHeader {
    pub const SIZE: usize = 16; // 16 bytes

    pub fn parse(endian: Endianness, data: RawData) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }

    pub fn parse_impl<O: ByteOrder>(data: RawData) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let record_type = JitDumpRecordType(cur.read_u32::<O>()?);
        let total_size = cur.read_u32::<O>()?;
        let timestamp = cur.read_u64::<O>()?;
        Ok(Self {
            record_type,
            total_size,
            timestamp,
        })
    }
}

/// An enum carrying a parsed jitdump record.
#[derive(Debug, Clone)]
pub enum JitDumpRecord<'a> {
    CodeLoad(JitCodeLoadRecord<'a>),
    CodeMove(JitCodeMoveRecord),
    CodeDebugInfo(JitCodeDebugInfoRecord<'a>),
    CodeClose,
    CodeUnwindingInfo(JitCodeUnwindingInfoRecord<'a>),
    Other(JitDumpRawRecord<'a>),
}

/// A raw jitdump record whose body hasn't been parsed yet.
#[derive(Debug, Clone)]
pub struct JitDumpRawRecord<'a> {
    /// The file endian (needs to be known during parsing).
    pub endian: Endianness,
    /// The record type.
    pub record_type: JitDumpRecordType,
    /// The timestamp.
    pub timestamp: u64,
    /// The offset in the jitdump file at which this record is stored. This
    /// points to the start of the record header.
    pub start_offset: u64,
    /// The size of this record in bytes, including the record header.
    pub record_size: u32,
    /// The raw data for the body of this record.
    pub body: RawData<'a>,
}

impl<'a> JitDumpRawRecord<'a> {
    pub fn parse(&self) -> Result<JitDumpRecord, std::io::Error> {
        match self.record_type {
            JitDumpRecordType::JIT_CODE_LOAD => {
                let record = JitCodeLoadRecord::parse(self.endian, self.body)?;
                Ok(JitDumpRecord::CodeLoad(record))
            }
            JitDumpRecordType::JIT_CODE_MOVE => {
                let record = JitCodeMoveRecord::parse(self.endian, self.body)?;
                Ok(JitDumpRecord::CodeMove(record))
            }
            JitDumpRecordType::JIT_CODE_DEBUG_INFO => {
                let record = JitCodeDebugInfoRecord::parse(self.endian, self.body)?;
                Ok(JitDumpRecord::CodeDebugInfo(record))
            }
            JitDumpRecordType::JIT_CODE_CLOSE => Ok(JitDumpRecord::CodeClose),
            JitDumpRecordType::JIT_CODE_UNWINDING_INFO => {
                let record = JitCodeUnwindingInfoRecord::parse(self.endian, self.body)?;
                Ok(JitDumpRecord::CodeUnwindingInfo(record))
            }
            _ => Ok(JitDumpRecord::Other(self.clone())),
        }
    }
}
