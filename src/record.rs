use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linux_perf_event_reader::records::RawEventRecord;
use linux_perf_event_reader::{Endianness, RawData, RecordType};

use crate::constants::*;
use crate::thread_map::ThreadMap;

/// A record from a perf.data file's data stream.
///
/// This can be either a record emitted by the kernel for a perf event, or a
/// synthesized record that was added by a user-space tool like `perf`.
pub enum PerfFileRecord<'a> {
    /// Emitted by the kernel for a perf event.
    EventRecord {
        /// And index into the array returned by [`PerfFileReader::event_attributes`](crate::PerfFileReader::event_attributes).
        attr_index: usize,
        /// The record.
        record: RawEventRecord<'a>,
    },
    /// Synthesized by a user space tool, for example by `perf` or by `simpleperf`.
    UserRecord(RawUserRecord<'a>),
}

/// A record emitted by a user space tool, for example by `perf` or by `simpleperf`.
#[derive(Debug, Clone)]
pub enum UserRecord<'a> {
    ThreadMap(ThreadMap<'a>),
    Raw(RawUserRecord<'a>),
}

/// A namespace for `RecordType` constants for which `RecordType::is_user_type()` returns true.
pub struct UserRecordType;

impl UserRecordType {
    pub const PERF_HEADER_ATTR: RecordType = RecordType(PERF_RECORD_HEADER_ATTR);
    pub const PERF_HEADER_EVENT_TYPE: RecordType = RecordType(PERF_RECORD_HEADER_EVENT_TYPE);
    pub const PERF_HEADER_TRACING_DATA: RecordType = RecordType(PERF_RECORD_HEADER_TRACING_DATA);
    pub const PERF_HEADER_BUILD_ID: RecordType = RecordType(PERF_RECORD_HEADER_BUILD_ID);
    pub const PERF_FINISHED_ROUND: RecordType = RecordType(PERF_RECORD_FINISHED_ROUND);
    pub const PERF_ID_INDEX: RecordType = RecordType(PERF_RECORD_ID_INDEX);
    pub const PERF_AUXTRACE_INFO: RecordType = RecordType(PERF_RECORD_AUXTRACE_INFO);
    pub const PERF_AUXTRACE: RecordType = RecordType(PERF_RECORD_AUXTRACE);
    pub const PERF_AUXTRACE_ERROR: RecordType = RecordType(PERF_RECORD_AUXTRACE_ERROR);
    pub const PERF_THREAD_MAP: RecordType = RecordType(PERF_RECORD_THREAD_MAP);
    pub const PERF_CPU_MAP: RecordType = RecordType(PERF_RECORD_CPU_MAP);
    pub const PERF_STAT_CONFIG: RecordType = RecordType(PERF_RECORD_STAT_CONFIG);
    pub const PERF_STAT: RecordType = RecordType(PERF_RECORD_STAT);
    pub const PERF_STAT_ROUND: RecordType = RecordType(PERF_RECORD_STAT_ROUND);
    pub const PERF_EVENT_UPDATE: RecordType = RecordType(PERF_RECORD_EVENT_UPDATE);
    pub const PERF_TIME_CONV: RecordType = RecordType(PERF_RECORD_TIME_CONV);
    pub const PERF_HEADER_FEATURE: RecordType = RecordType(PERF_RECORD_HEADER_FEATURE);
    pub const PERF_COMPRESSED: RecordType = RecordType(PERF_RECORD_COMPRESSED);

    pub const SIMPLEPERF_KERNEL_SYMBOL: RecordType = RecordType(SIMPLE_PERF_RECORD_KERNEL_SYMBOL);
    pub const SIMPLEPERF_DSO: RecordType = RecordType(SIMPLE_PERF_RECORD_DSO);
    pub const SIMPLEPERF_SYMBOL: RecordType = RecordType(SIMPLE_PERF_RECORD_SYMBOL);
    pub const SIMPLEPERF_SPLIT: RecordType = RecordType(SIMPLE_PERF_RECORD_SPLIT);
    pub const SIMPLEPERF_SPLIT_END: RecordType = RecordType(SIMPLE_PERF_RECORD_SPLIT_END);
    pub const SIMPLEPERF_EVENT_ID: RecordType = RecordType(SIMPLE_PERF_RECORD_EVENT_ID);
    pub const SIMPLEPERF_CALLCHAIN: RecordType = RecordType(SIMPLE_PERF_RECORD_CALLCHAIN);
    pub const SIMPLEPERF_UNWINDING_RESULT: RecordType =
        RecordType(SIMPLE_PERF_RECORD_UNWINDING_RESULT);
    pub const SIMPLEPERF_TRACING_DATA: RecordType = RecordType(SIMPLE_PERF_RECORD_TRACING_DATA);
}

/// A raw user record.
///
/// Can be turned into a parsed [`UserRecord`] using [`RawUserRecord::parse`].
#[derive(Debug, Clone)]
pub struct RawUserRecord<'a> {
    pub record_type: RecordType,
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
            // UserRecordType::PERF_HEADER_ATTR => {},
            // UserRecordType::PERF_HEADER_EVENT_TYPE => {},
            // UserRecordType::PERF_HEADER_TRACING_DATA => {},
            // UserRecordType::PERF_HEADER_BUILD_ID => {},
            // UserRecordType::PERF_FINISHED_ROUND => {},
            // UserRecordType::PERF_ID_INDEX => {},
            // UserRecordType::PERF_AUXTRACE_INFO => {},
            // UserRecordType::PERF_AUXTRACE => {},
            // UserRecordType::PERF_AUXTRACE_ERROR => {},
            UserRecordType::PERF_THREAD_MAP => {
                UserRecord::ThreadMap(ThreadMap::parse::<T>(self.data)?)
            }
            // UserRecordType::PERF_CPU_MAP => {},
            // UserRecordType::PERF_STAT_CONFIG => {},
            // UserRecordType::PERF_STAT => {},
            // UserRecordType::PERF_STAT_ROUND => {},
            // UserRecordType::PERF_EVENT_UPDATE => {},
            // UserRecordType::PERF_TIME_CONV => {},
            // UserRecordType::PERF_HEADER_FEATURE => {},
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
