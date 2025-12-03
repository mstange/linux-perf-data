use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};
use linux_perf_event_reader::RawEventRecord;
use linux_perf_event_reader::{Endianness, PerfEventAttr, RawData, RecordType};

use crate::auxtrace::Auxtrace;
use crate::constants::*;
use crate::features::Feature;
use crate::thread_map::ThreadMap;

/// A record from a perf.data file's data stream.
///
/// This can be either a record emitted by the kernel for a perf event, or a
/// synthesized record that was added by a user-space tool like `perf`.
pub enum PerfFileRecord<'a> {
    /// Emitted by the kernel for a perf event.
    EventRecord {
        /// And index into the array returned by [`PerfFile::event_attributes`](crate::PerfFile::event_attributes).
        attr_index: usize,
        /// The record.
        record: RawEventRecord<'a>,
    },
    /// Synthesized by a user space tool, for example by `perf` or by `simpleperf`.
    UserRecord(RawUserRecord<'a>),
}

/// A record emitted by a user space tool, for example by `perf` or by `simpleperf`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum UserRecord<'a> {
    ThreadMap(ThreadMap<'a>),
    HeaderAttr(HeaderAttr),
    HeaderFeature(HeaderFeature),
    Auxtrace(Auxtrace<'a>),
    Raw(RawUserRecord<'a>),
}

/// A newtype wrapping `RecordType` values for which `RecordType::is_user_type()` returns true.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserRecordType(RecordType);

impl UserRecordType {
    pub const PERF_HEADER_ATTR: Self = Self(RecordType(PERF_RECORD_HEADER_ATTR));
    pub const PERF_HEADER_EVENT_TYPE: Self = Self(RecordType(PERF_RECORD_HEADER_EVENT_TYPE));
    pub const PERF_HEADER_TRACING_DATA: Self = Self(RecordType(PERF_RECORD_HEADER_TRACING_DATA));
    pub const PERF_HEADER_BUILD_ID: Self = Self(RecordType(PERF_RECORD_HEADER_BUILD_ID));
    pub const PERF_FINISHED_ROUND: Self = Self(RecordType(PERF_RECORD_FINISHED_ROUND));
    pub const PERF_ID_INDEX: Self = Self(RecordType(PERF_RECORD_ID_INDEX));
    pub const PERF_AUXTRACE_INFO: Self = Self(RecordType(PERF_RECORD_AUXTRACE_INFO));
    pub const PERF_AUXTRACE: Self = Self(RecordType(PERF_RECORD_AUXTRACE));
    pub const PERF_AUXTRACE_ERROR: Self = Self(RecordType(PERF_RECORD_AUXTRACE_ERROR));
    pub const PERF_THREAD_MAP: Self = Self(RecordType(PERF_RECORD_THREAD_MAP));
    pub const PERF_CPU_MAP: Self = Self(RecordType(PERF_RECORD_CPU_MAP));
    pub const PERF_STAT_CONFIG: Self = Self(RecordType(PERF_RECORD_STAT_CONFIG));
    pub const PERF_STAT: Self = Self(RecordType(PERF_RECORD_STAT));
    pub const PERF_STAT_ROUND: Self = Self(RecordType(PERF_RECORD_STAT_ROUND));
    pub const PERF_EVENT_UPDATE: Self = Self(RecordType(PERF_RECORD_EVENT_UPDATE));
    pub const PERF_TIME_CONV: Self = Self(RecordType(PERF_RECORD_TIME_CONV));
    pub const PERF_HEADER_FEATURE: Self = Self(RecordType(PERF_RECORD_HEADER_FEATURE));
    pub const PERF_COMPRESSED: Self = Self(RecordType(PERF_RECORD_COMPRESSED));
    pub const PERF_COMPRESSED2: Self = Self(RecordType(PERF_RECORD_COMPRESSED2));

    pub const SIMPLEPERF_KERNEL_SYMBOL: Self = Self(RecordType(SIMPLE_PERF_RECORD_KERNEL_SYMBOL));
    pub const SIMPLEPERF_DSO: Self = Self(RecordType(SIMPLE_PERF_RECORD_DSO));
    pub const SIMPLEPERF_SYMBOL: Self = Self(RecordType(SIMPLE_PERF_RECORD_SYMBOL));
    pub const SIMPLEPERF_SPLIT: Self = Self(RecordType(SIMPLE_PERF_RECORD_SPLIT));
    pub const SIMPLEPERF_SPLIT_END: Self = Self(RecordType(SIMPLE_PERF_RECORD_SPLIT_END));
    pub const SIMPLEPERF_EVENT_ID: Self = Self(RecordType(SIMPLE_PERF_RECORD_EVENT_ID));
    pub const SIMPLEPERF_CALLCHAIN: Self = Self(RecordType(SIMPLE_PERF_RECORD_CALLCHAIN));
    pub const SIMPLEPERF_UNWINDING_RESULT: Self =
        Self(RecordType(SIMPLE_PERF_RECORD_UNWINDING_RESULT));
    pub const SIMPLEPERF_TRACING_DATA: Self = Self(RecordType(SIMPLE_PERF_RECORD_TRACING_DATA));

    pub fn try_from(record_type: RecordType) -> Option<Self> {
        if record_type.is_user_type() {
            Some(Self(record_type))
        } else {
            None
        }
    }

    pub fn record_type(&self) -> RecordType {
        self.0
    }
}

impl From<UserRecordType> for RecordType {
    fn from(record_type: UserRecordType) -> Self {
        record_type.0
    }
}

impl std::fmt::Debug for UserRecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::PERF_HEADER_ATTR => "PERF_HEADER_ATTR".fmt(f),
            Self::PERF_HEADER_EVENT_TYPE => "PERF_HEADER_EVENT_TYPE".fmt(f),
            Self::PERF_HEADER_TRACING_DATA => "PERF_HEADER_TRACING_DATA".fmt(f),
            Self::PERF_HEADER_BUILD_ID => "PERF_HEADER_BUILD_ID".fmt(f),
            Self::PERF_FINISHED_ROUND => "PERF_FINISHED_ROUND".fmt(f),
            Self::PERF_ID_INDEX => "PERF_ID_INDEX".fmt(f),
            Self::PERF_AUXTRACE_INFO => "PERF_AUXTRACE_INFO".fmt(f),
            Self::PERF_AUXTRACE => "PERF_AUXTRACE".fmt(f),
            Self::PERF_AUXTRACE_ERROR => "PERF_AUXTRACE_ERROR".fmt(f),
            Self::PERF_THREAD_MAP => "PERF_THREAD_MAP".fmt(f),
            Self::PERF_CPU_MAP => "PERF_CPU_MAP".fmt(f),
            Self::PERF_STAT_CONFIG => "PERF_STAT_CONFIG".fmt(f),
            Self::PERF_STAT => "PERF_STAT".fmt(f),
            Self::PERF_STAT_ROUND => "PERF_STAT_ROUND".fmt(f),
            Self::PERF_EVENT_UPDATE => "PERF_EVENT_UPDATE".fmt(f),
            Self::PERF_TIME_CONV => "PERF_TIME_CONV".fmt(f),
            Self::PERF_HEADER_FEATURE => "PERF_HEADER_FEATURE".fmt(f),
            Self::PERF_COMPRESSED => "PERF_COMPRESSED".fmt(f),
            Self::PERF_COMPRESSED2 => "PERF_COMPRESSED2".fmt(f),
            Self::SIMPLEPERF_KERNEL_SYMBOL => "SIMPLEPERF_KERNEL_SYMBOL".fmt(f),
            Self::SIMPLEPERF_DSO => "SIMPLEPERF_DSO".fmt(f),
            Self::SIMPLEPERF_SYMBOL => "SIMPLEPERF_SYMBOL".fmt(f),
            Self::SIMPLEPERF_SPLIT => "SIMPLEPERF_SPLIT".fmt(f),
            Self::SIMPLEPERF_SPLIT_END => "SIMPLEPERF_SPLIT_END".fmt(f),
            Self::SIMPLEPERF_EVENT_ID => "SIMPLEPERF_EVENT_ID".fmt(f),
            Self::SIMPLEPERF_CALLCHAIN => "SIMPLEPERF_CALLCHAIN".fmt(f),
            Self::SIMPLEPERF_UNWINDING_RESULT => "SIMPLEPERF_UNWINDING_RESULT".fmt(f),
            Self::SIMPLEPERF_TRACING_DATA => "SIMPLEPERF_TRACING_DATA".fmt(f),
            other => f.write_fmt(format_args!("Unknown UserRecordType {}", other.0 .0)),
        }
    }
}

/// A raw user record.
///
/// Can be turned into a parsed [`UserRecord`] using [`RawUserRecord::parse`].
#[derive(Debug, Clone)]
pub struct RawUserRecord<'a> {
    pub record_type: UserRecordType,
    pub endian: Endianness,
    pub misc: u16,
    pub data: RawData<'a>,
}

impl<'a> RawUserRecord<'a> {
    pub fn parse(&self) -> Result<UserRecord<'a>, std::io::Error> {
        match self.endian {
            Endianness::LittleEndian => self.parse_impl::<LittleEndian>(),
            Endianness::BigEndian => self.parse_impl::<BigEndian>(),
        }
    }

    pub fn parse_impl<T: ByteOrder>(&self) -> Result<UserRecord<'a>, std::io::Error> {
        let record_type = self.record_type;

        let record = match record_type {
            UserRecordType::PERF_HEADER_ATTR => {
                UserRecord::HeaderAttr(HeaderAttr::parse::<T>(self.data)?)
            }
            UserRecordType::PERF_THREAD_MAP => {
                UserRecord::ThreadMap(ThreadMap::parse::<T>(self.data)?)
            }
            UserRecordType::PERF_HEADER_FEATURE => {
                UserRecord::HeaderFeature(HeaderFeature::parse::<T>(self.data)?)
            }
            // UserRecordType::PERF_HEADER_EVENT_TYPE => {},
            // UserRecordType::PERF_HEADER_TRACING_DATA => {},
            // UserRecordType::PERF_HEADER_BUILD_ID => {},
            // UserRecordType::PERF_FINISHED_ROUND => {},
            // UserRecordType::PERF_ID_INDEX => {},
            // UserRecordType::PERF_AUXTRACE_INFO => {},
            UserRecordType::PERF_AUXTRACE => UserRecord::Auxtrace(Auxtrace::parse::<T>(self.data)?),
            // UserRecordType::PERF_AUXTRACE_ERROR => {},
            // UserRecordType::PERF_CPU_MAP => {},
            // UserRecordType::PERF_STAT_CONFIG => {},
            // UserRecordType::PERF_STAT => {},
            // UserRecordType::PERF_STAT_ROUND => {},
            // UserRecordType::PERF_EVENT_UPDATE => {},
            // UserRecordType::PERF_TIME_CONV => {},
            // UserRecordType::PERF_COMPRESSED => {},
            // UserRecordType::SIMPLEPERF_KERNEL_SYMBOL => {},
            // UserRecordType::SIMPLEPERF_DSO => {},
            // UserRecordType::SIMPLEPERF_SYMBOL => {},
            // UserRecordType::SIMPLEPERF_SPLIT => {},
            // UserRecordType::SIMPLEPERF_SPLIT_END => {},
            // UserRecordType::SIMPLEPERF_EVENT_ID => {},
            // UserRecordType::SIMPLEPERF_CALLCHAIN => {},
            // UserRecordType::SIMPLEPERF_UNWINDING_RESULT => {},
            // UserRecordType::SIMPLEPERF_TRACING_DATA => {},
            _ => UserRecord::Raw(self.clone()),
        };
        Ok(record)
    }
}

/// PERF_RECORD_HEADER_ATTR - Contains event attribute and associated event IDs
///
/// Used in pipe mode to transmit event attribute information that would
/// otherwise be in the attrs section of a regular perf.data file.
#[derive(Debug, Clone)]
pub struct HeaderAttr {
    pub attr: PerfEventAttr,
    pub ids: Vec<u64>,
}

impl HeaderAttr {
    pub fn parse<T: ByteOrder>(data: RawData) -> Result<Self, std::io::Error> {
        let mut cursor = std::io::Cursor::new(data.as_slice());

        // Parse the perf_event_attr
        let (attr, _attr_size) = PerfEventAttr::parse::<_, T>(&mut cursor)?;

        // Remaining data is the array of event IDs
        let mut ids = Vec::new();
        while cursor.position() < data.len() as u64 {
            ids.push(cursor.read_u64::<T>()?);
        }

        Ok(Self { attr, ids })
    }
}

/// PERF_RECORD_HEADER_FEATURE - Contains feature section data
///
/// Used in pipe mode to transmit feature data that would otherwise be in
/// the feature sections at the end of a regular perf.data file.
#[derive(Debug, Clone)]
pub struct HeaderFeature {
    pub feature: Feature,
    pub data: Vec<u8>,
}

impl HeaderFeature {
    pub fn parse<T: ByteOrder>(data: RawData) -> Result<Self, std::io::Error> {
        let mut cursor = std::io::Cursor::new(data.as_slice());

        // First 8 bytes is the feature type
        let feature_type = cursor.read_u64::<T>()? as u32;
        let feature = Feature(feature_type);

        // Remaining data is the feature data itself
        let start_pos = cursor.position() as usize;
        let feature_data = data.as_slice()[start_pos..].to_vec();

        Ok(Self {
            feature,
            data: feature_data,
        })
    }
}
