mod build_id_event;
mod dso_key;
mod error;
mod flag_feature;
mod flag_sections;
mod perf_file;

pub use dso_key::DsoKey;
pub use error::{Error, ReadError};
pub use flag_sections::NrCpus;
pub use flag_feature::FlagFeature;

/// This is a re-export of the linux-perf-event-reader crate. We use its types
/// in our public API.
pub use linux_perf_event_reader;

use std::collections::{HashMap, VecDeque};
use std::io::{Cursor, Read, Seek, SeekFrom};

use build_id_event::BuildIdEvent;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linux_perf_event_reader::records::{ParsedRecord, RawRecord, RecordParseInfo};
use linux_perf_event_reader::{CpuMode, PerfEventAttr, PerfEventHeader, RawData, RecordType};
use perf_file::{PerfFileSection, PerfHeader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    LittleEndian,
    BigEndian,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DsoBuildId {
    pub path: Vec<u8>,
    pub build_id: Vec<u8>,
}

pub struct PerfFileReader<R: Read> {
    reader: R,
    endian: Endianness,
    feature_sections: Vec<(FlagFeature, Vec<u8>)>,
    read_offset: u64,
    record_data_len: u64,
    current_event_body: Vec<u8>,
    attributes: PerfEventAttr,
    parse_info: RecordParseInfo,
    /// Sorted by time
    remaining_pending_records: VecDeque<PendingRecord>,
    buffers_for_recycling: VecDeque<Vec<u8>>,
}

impl<C: Read + Seek> PerfFileReader<C> {
    pub fn parse_file(mut cursor: C) -> Result<Self, Error> {
        let header = PerfHeader::parse(&mut cursor)?;
        match &header.magic {
            b"PERFILE2" => {
                Self::parse_file_impl::<LittleEndian>(cursor, header, Endianness::LittleEndian)
            }
            b"2ELIFREP" => {
                Self::parse_file_impl::<BigEndian>(cursor, header, Endianness::BigEndian)
            }
            _ => Err(Error::UnrecognizedMagicValue(header.magic)),
        }
    }

    fn parse_file_impl<T>(
        mut cursor: C,
        header: PerfHeader,
        endian: Endianness,
    ) -> Result<Self, Error>
    where
        T: ByteOrder,
    {
        // Read the section information for each flag, starting just after the data section.
        let mut flag = 0u32;
        let feature_pos = header.data_section.offset + header.data_section.size;
        cursor.seek(SeekFrom::Start(feature_pos))?;
        let mut feature_sections_info = Vec::new();
        for flags_chunk in header.flags {
            for bit_index in 0..8 {
                let flag_is_set = (flags_chunk & (1 << bit_index)) != 0;
                if flag_is_set {
                    let section = PerfFileSection::parse::<_, T>(&mut cursor)?;
                    if let Some(feature) = FlagFeature::from_int(flag) {
                        feature_sections_info.push((feature, section));
                    } else {
                        eprintln!("Unrecognized flag feature {}", flag);
                    }
                }
                flag += 1;
            }
        }

        let mut feature_sections = Vec::new();
        for (feature, section) in feature_sections_info {
            let offset = section.offset;
            let size = usize::try_from(section.size).map_err(|_| Error::SectionSizeTooBig)?;
            let mut data = vec![0; size];
            cursor.seek(SeekFrom::Start(offset))?;
            cursor.read_exact(&mut data)?;
            feature_sections.push((feature, data));
        }

        let attrs_offset = header.attr_section.offset;
        let attrs_size = header.attr_section.size;
        cursor.seek(SeekFrom::Start(attrs_offset))?;
        let mut perf_event_attrs = Vec::new();
        let attr_size = header.attr_size;
        let mut offset = 0;
        while offset + attr_size <= attrs_size {
            let attr = PerfEventAttr::parse::<_, T>(&mut cursor, Some(attr_size as u32))
                .map_err(|_| ReadError::PerfEventAttr)?;
            perf_event_attrs.push(attr);
            offset += attr_size;
        }

        // Grab the first of the perf event attrs.
        // TODO: What happens if there's more than one attr? How do we know which
        // records belong to which event?
        let attributes = perf_event_attrs.remove(0);
        let parse_info = RecordParseInfo::from_attr(&attributes);

        // Move the cursor to the start of the data section so that we can start
        // reading records from it.
        cursor.seek(SeekFrom::Start(header.data_section.offset))?;

        Ok(Self {
            reader: cursor,
            endian,
            attributes,
            feature_sections,
            read_offset: 0,
            record_data_len: header.data_section.size,
            parse_info,
            remaining_pending_records: VecDeque::new(),
            buffers_for_recycling: VecDeque::new(),
            current_event_body: Vec::new(),
        })
    }
}

impl<R: Read> PerfFileReader<R> {
    pub fn endian(&self) -> Endianness {
        self.endian
    }

    /// The attributes which were requested for the perf event.
    pub fn attributes(&self) -> &PerfEventAttr {
        &self.attributes
    }

    pub fn has_feature(&self, feature: FlagFeature) -> bool {
        self.feature_sections.iter().any(|(f, _)| *f == feature)
    }

    pub fn feature_section(&self, feature: FlagFeature) -> Option<&[u8]> {
        self.feature_sections
            .iter()
            .find_map(|(f, d)| if *f == feature { Some(&d[..]) } else { None })
    }

    /// Returns a map of build ID entries. `perf record` creates these records for any DSOs
    /// which it thinks have been "hit" in the profile. They supplement Mmap events
    /// the perf event stream; those usually don't come with build IDs.
    ///
    /// This method returns a HashMap so that you can easily look up the right build ID from
    /// the DsoKey in an Mmap event. For some DSOs, the path in the raw Mmap event can be
    /// different from the path in the build ID record; for example, the Mmap event for the
    /// kernel ("vmlinux") image could have the path "[kernel.kallsyms]_text", whereas the
    /// corresponding build ID record might have the path "[kernel.kallsyms]" (without the
    /// trailing "_text"), or it could even have the full absolute path to a vmlinux file.
    /// The DsoKey canonicalizes those differences away.
    ///
    /// Having the build ID for a DSO allows you to do the following:
    ///
    ///  - If the DSO file has changed in the time since the perf.data file was captured,
    ///    you can detect this change because the new file will have a different build ID.
    ///  - If debug symbols are installed for the DSO, you can sometimes find the debug symbol
    ///    file using the build ID. For example, you might find it at
    ///    /usr/lib/debug/.build-id/b8/037b6260865346802321dd2256b8ad1d857e63.debug
    ///  - If the original DSO file is gone, or you're trying to read the perf.data file on
    ///    an entirely different machine, you can sometimes retrieve the original DSO file just
    ///    from its build ID, for example from a debuginfod server.
    ///  - This also works for DSOs which are not present on the file system at all;
    ///    specifically, the vDSO file is a bit of a pain to obtain. With the build ID you can
    ///    instead obtain it from, say,
    ///    <https://debuginfod.elfutils.org/buildid/0d82ee4bd7f9609c367095ba0bedf155b71cb058/executable>
    ///
    /// This method is a bit lossy. We discard the pid, because it seems to be always -1 in
    /// the files I've tested. We also discard any entries for which we fail to create a `DsoKey`.
    pub fn build_ids(&self) -> Result<HashMap<DsoKey, DsoBuildId>, Error> {
        let section_data = match self.feature_section(FlagFeature::BuildId) {
            Some(section) => section,
            None => return Ok(HashMap::new()),
        };
        let mut cursor = Cursor::new(section_data);
        let mut build_ids = HashMap::new();
        loop {
            let event = match self.endian {
                Endianness::LittleEndian => BuildIdEvent::parse::<_, LittleEndian>(&mut cursor),
                Endianness::BigEndian => BuildIdEvent::parse::<_, BigEndian>(&mut cursor),
            };
            let event = match event {
                Ok(e) => e,
                Err(_) => break,
            };
            let misc = event.header.misc;
            let path = event.file_path;
            let build_id = event.build_id;
            let dso_key = match DsoKey::detect(&path, CpuMode::from_misc(misc)) {
                Some(dso_key) => dso_key,
                None => continue,
            };
            build_ids.insert(dso_key, DsoBuildId { path, build_id });
        }
        Ok(build_ids)
    }

    /// Only call this for features whose section is just a perf_header_string.
    fn feature_string(&self, feature: FlagFeature) -> Result<Option<&str>, Error> {
        self.feature_section(feature)
            .map(|section| self.read_string(section))
            .transpose()
    }

    pub fn hostname(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::Hostname)
    }

    pub fn os_release(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::OsRelease)
    }

    pub fn perf_version(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::Version)
    }

    pub fn arch(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::Arch)
    }

    pub fn nr_cpus(&self) -> Result<Option<NrCpus>, Error> {
        self.feature_section(FlagFeature::NrCpus)
            .map(|section| {
                let mut cursor = Cursor::new(section);
                Ok(match self.endian {
                    Endianness::LittleEndian => NrCpus::parse::<_, LittleEndian>(&mut cursor),
                    Endianness::BigEndian => NrCpus::parse::<_, BigEndian>(&mut cursor),
                }?)
            })
            .transpose()
    }

    pub fn is_stats(&self) -> bool {
        self.has_feature(FlagFeature::Stat)
    }

    fn read_string<'s>(&self, s: &'s [u8]) -> Result<&'s str, Error> {
        if s.len() < 4 {
            return Err(Error::NotEnoughSpaceForStringLen);
        }
        let (len_bytes, rest) = s.split_at(4);
        let len_bytes = [len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]];
        let len = match self.endian {
            Endianness::LittleEndian => u32::from_le_bytes(len_bytes),
            Endianness::BigEndian => u32::from_be_bytes(len_bytes),
        };
        let len = usize::try_from(len).map_err(|_| Error::StringLengthBiggerThanUsize)?;
        let s = &rest.get(..len as usize).ok_or(Error::StringLengthTooLong)?;
        let actual_len = memchr::memchr(0, s).unwrap_or(s.len());
        let s = std::str::from_utf8(&s[..actual_len]).map_err(|_| Error::StringUtf8)?;
        Ok(s)
    }

    /// Emits records in the correct order (sorted by time).
    ///
    /// It buffers records until it sees a FINISHED_ROUND record; then it sorts the
    /// buffered records and emits them one by one.
    pub fn next_record(&mut self) -> Result<Option<ParsedRecord>, Error> {
        if self.remaining_pending_records.is_empty() {
            self.read_current_round()?;
        }
        if let Some(pending_record) = self.remaining_pending_records.pop_front() {
            return Ok(Some(self.convert_pending_record(pending_record)?));
        }
        Ok(None)
    }

    /// Reads events into self.remaining_pending_records until a FINISHED_ROUND
    /// record is found and self.remaining_pending_records is non-empty, or until
    /// we've run out of records to read.
    ///
    /// When this function returns, self.remaining_pending_records is sorted by
    /// timestamp.
    fn read_current_round(&mut self) -> Result<(), Error> {
        if self.endian == Endianness::LittleEndian {
            self.read_current_round_impl::<byteorder::LittleEndian>()
        } else {
            self.read_current_round_impl::<byteorder::BigEndian>()
        }
    }

    /// Reads events into self.remaining_pending_records until a FINISHED_ROUND
    /// record is found and self.remaining_pending_records is non-empty, or until
    /// we've run out of records to read.
    ///
    /// When this function returns, self.remaining_pending_records is sorted by
    /// timestamp.
    fn read_current_round_impl<T: ByteOrder>(&mut self) -> Result<(), Error> {
        assert!(self.remaining_pending_records.is_empty());

        while self.read_offset < self.record_data_len {
            let offset = self.read_offset;
            let header = PerfEventHeader::parse::<_, T>(&mut self.reader)?;
            let size = header.size as usize;
            if size < PerfEventHeader::STRUCT_SIZE {
                return Err(Error::InvalidPerfEventSize);
            }
            self.read_offset += u64::from(header.size);

            let record_type = RecordType(header.type_);
            if record_type == RecordType::FINISHED_ROUND {
                if self.remaining_pending_records.is_empty() {
                    // Keep going so that we never return with remaining_pending_records
                    // being empty, unless we've truly run out of data to read.
                    continue;
                } else {
                    // We've finished a non-empty round. Exit the loop.
                    break;
                }
            }

            let event_body_len = size - PerfEventHeader::STRUCT_SIZE;
            let mut buffer = self.buffers_for_recycling.pop_front().unwrap_or_default();
            buffer.resize(event_body_len, 0);
            self.reader
                .read_exact(&mut buffer)
                .map_err(|_| ReadError::PerfEventData)?;

            let misc = header.misc;
            let raw_event = RawRecord {
                record_type,
                misc,
                data: RawData::from(&buffer[..]),
            };
            let timestamp = raw_event.timestamp::<T>(&self.parse_info);
            let sort_key = RecordSortKey { timestamp, offset };
            let pending_record = PendingRecord {
                sort_key,
                record_type,
                misc,
                buffer,
            };
            self.remaining_pending_records.push_back(pending_record);
        }

        self.remaining_pending_records
            .make_contiguous()
            .sort_unstable_by_key(|r| r.sort_key);
        Ok(())
    }

    /// Converts pending_record into an ParsedRecord which references the data in self.current_event_body.
    fn convert_pending_record(
        &mut self,
        pending_record: PendingRecord,
    ) -> Result<ParsedRecord, Error> {
        let PendingRecord {
            record_type,
            misc,
            buffer,
            ..
        } = pending_record;
        let prev_buffer = std::mem::replace(&mut self.current_event_body, buffer);
        self.buffers_for_recycling.push_back(prev_buffer);
        let data = RawData::from(&self.current_event_body[..]);
        let raw_record = RawRecord {
            record_type,
            misc,
            data,
        };
        Ok(if self.endian == Endianness::LittleEndian {
            raw_record.to_parsed::<byteorder::LittleEndian>(&self.parse_info)
        } else {
            raw_record.to_parsed::<byteorder::BigEndian>(&self.parse_info)
        }?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRecord {
    sort_key: RecordSortKey,
    record_type: RecordType,
    misc: u16,
    buffer: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RecordSortKey {
    timestamp: Option<u64>,
    offset: u64,
}
