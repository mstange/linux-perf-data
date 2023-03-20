use linux_perf_event_reader::{Endianness, RawData};
use std::io::{Read, Seek};

use super::buffered_reader::BufferedReader;
use super::error::JitDumpError;
use super::header::JitDumpHeader;
use super::read_exact::ReadExactOrUntilEof;
use super::record::{JitDumpRawRecord, JitDumpRecordHeader, JitDumpRecordType};

/// Parses a jitdump file and allows iterating over records.
///
/// This reader works with complete jitdump files as well as with partial files
/// which are still being written to. This makes it useful in live-profiling
/// settings.
///
/// The records refer to memory owned by the reader, to minimize copies.
#[derive(Debug, Clone)]
pub struct JitDumpReader<R: Read> {
    reader: BufferedReader<R>,
    header: JitDumpHeader,
    endian: Endianness,
    pending_record_header: Option<JitDumpRecordHeader>,
    current_record_start_offset: u64,
}

impl<R: Read> JitDumpReader<R> {
    /// Create a new `JitDumpReader`. `JitDumpReader` does its own buffering so
    /// there is no need to wrap a [`File`](std::fs::File) into a `BufReader`.
    pub fn new(reader: R) -> Result<Self, JitDumpError> {
        Self::new_with_buffer_size(reader, 4 * 1024)
    }

    /// Create a new `JitDumpReader`, with a manually-specified buffer chunk size.
    pub fn new_with_buffer_size(mut reader: R, buffer_size: usize) -> Result<Self, JitDumpError> {
        let mut buf = vec![0; buffer_size];
        let first_data_len = reader
            .read_exact_or_until_eof(&mut buf)
            .map_err(JitDumpError::Io)?;

        let first_data = &buf[..first_data_len];
        let header = JitDumpHeader::parse(RawData::Single(first_data))?;
        let total_header_size = header.total_size;
        let endian = match &header.magic {
            b"DTiJ" => Endianness::LittleEndian,
            b"JiTD" => Endianness::BigEndian,
            _ => panic!(),
        };

        Ok(Self {
            reader: BufferedReader::new_with_partially_read_buffer(
                reader,
                buf,
                total_header_size as usize,
                first_data_len,
            ),
            header,
            endian,
            pending_record_header: None,
            current_record_start_offset: total_header_size as u64,
        })
    }

    /// The file header.
    pub fn header(&self) -> &JitDumpHeader {
        &self.header
    }

    /// The file endian.
    pub fn endian(&self) -> Endianness {
        self.endian
    }

    /// Returns the header of the next record.
    pub fn next_record_header(&mut self) -> Result<Option<JitDumpRecordHeader>, std::io::Error> {
        if self.pending_record_header.is_none() {
            if let Some(record_header_bytes) =
                self.reader.consume_data(JitDumpRecordHeader::SIZE)?
            {
                self.pending_record_header =
                    Some(JitDumpRecordHeader::parse(self.endian, record_header_bytes).unwrap());
            }
        };
        Ok(self.pending_record_header.clone())
    }

    /// Returns the timestamp of the next record.
    ///
    /// When operating on partial files, `None` means that not enough bytes for the header
    /// of the next record are available. `Some` means that we have enough bytes for the
    /// header but we may not have enough bytes to get the entire record.
    ///
    /// If `next_record_timestamp` returns `Ok(Some(...))`, the next call to `next_record()`
    /// can still return `None`!
    pub fn next_record_timestamp(&mut self) -> Result<Option<u64>, std::io::Error> {
        Ok(self.next_record_header()?.map(|r| r.timestamp))
    }

    /// Returns the record type of the next record.
    pub fn next_record_type(&mut self) -> Result<Option<JitDumpRecordType>, std::io::Error> {
        Ok(self.next_record_header()?.map(|r| r.record_type))
    }

    /// Returns the file offset at which the next record (specifically its record header) starts.
    pub fn next_record_offset(&self) -> u64 {
        self.current_record_start_offset
    }

    /// Returns the next record.
    ///
    /// When operating on partial files, this will return `Ok(None)` if the entire record is
    /// not available yet. Future calls to `next_record` may return `Ok(Some)` if the
    /// data has become available in the meantime, because they will call `read` on `R` again.
    pub fn next_record(&mut self) -> Result<Option<JitDumpRawRecord>, std::io::Error> {
        let record_size = match self.next_record_header()? {
            Some(header) => header.total_size,
            None => return Ok(None),
        };
        let body_size = record_size as usize - JitDumpRecordHeader::SIZE;

        match self.reader.consume_data(body_size)? {
            Some(record_body_data) => {
                let record_header = self.pending_record_header.take().unwrap();
                let start_offset = self.current_record_start_offset;
                self.current_record_start_offset += record_size as u64;
                Ok(Some(JitDumpRawRecord {
                    endian: self.endian,
                    start_offset,
                    record_size,
                    record_type: record_header.record_type,
                    timestamp: record_header.timestamp,
                    body: record_body_data,
                }))
            }
            None => Ok(None),
        }
    }
}

impl<R: Read + Seek> JitDumpReader<R> {
    /// Skip the upcoming record. If this returns true, the record has been skipped.
    /// If `false` is returned, it means the file could not be seeked far enough to
    /// skip the entire record (for example because this is a partial file which has
    /// not been fully written), and the next record remains unchanged from before the
    /// call to `skip_next_record`.
    ///
    /// You may want to call this if you've called `next_record_type` and have
    /// determined that you're not interested in the upcoming record. It saves having
    /// to read the full record into a contiguous slice of memory.
    pub fn skip_next_record(&mut self) -> Result<bool, std::io::Error> {
        let record_size = match self.next_record_header()? {
            Some(record_header) => record_header.total_size,
            None => return Ok(false),
        };
        let body_size = record_size as usize - JitDumpRecordHeader::SIZE; // TODO: Handle underflow

        self.reader.skip_bytes(body_size)?;
        self.pending_record_header.take();
        self.current_record_start_offset += record_size as u64;
        Ok(true)
    }
}
