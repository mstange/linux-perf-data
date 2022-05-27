mod build_id_event;
mod dso_key;
mod error;
mod flag_feature;
mod flag_sections;
mod perf_file;

pub use dso_key::DsoKey;
pub use error::{Error, ReadError};
pub use flag_feature::{FlagFeature, FlagFeatureSet};
pub use flag_sections::{NrCpus, SampleTimeRange};

/// This is a re-export of the linux-perf-event-reader crate. We use its types
/// in our public API.
pub use linux_perf_event_reader;

pub use linux_perf_event_reader::Endianness;

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::ops::Deref;

use build_id_event::BuildIdEvent;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use flag_sections::AttributeDescription;
use linear_map::LinearMap;
use linux_perf_event_reader::records::{get_record_event_identifier, RawRecord, RecordParseInfo};
use linux_perf_event_reader::{
    AttrFlags, CpuMode, PerfEventHeader, RawData, RecordType, SampleFormat,
};
use perf_file::{PerfFileSection, PerfHeader};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DsoBuildId {
    pub path: Vec<u8>,
    pub build_id: Vec<u8>,
}

pub struct PerfFileReader<R: Read> {
    reader: R,
    endian: Endianness,
    feature_flags: FlagFeatureSet,
    feature_sections: LinearMap<FlagFeature, Vec<u8>>,
    read_offset: u64,
    record_data_len: u64,
    current_event_body: Vec<u8>,
    /// Guaranteed to have at least one element
    attributes: Vec<AttributeDescription>,
    /// Guaranteed to have at least one element
    parse_infos: Vec<RecordParseInfo>,
    event_id_to_attr_index: HashMap<u64, usize>,
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
        let feature_pos = header.data_section.offset + header.data_section.size;
        cursor.seek(SeekFrom::Start(feature_pos))?;
        let mut feature_sections_info = Vec::new();
        for flag in header.flags.iter() {
            let section = PerfFileSection::parse::<_, T>(&mut cursor)?;
            if let Some(feature) = FlagFeature::from_int(flag) {
                feature_sections_info.push((feature, section));
            } else {
                eprintln!("Unrecognized flag feature {}", flag);
            }
        }

        let mut feature_sections = LinearMap::new();
        for (feature, section) in feature_sections_info {
            let offset = section.offset;
            let size = usize::try_from(section.size).map_err(|_| Error::SectionSizeTooBig)?;
            let mut data = vec![0; size];
            cursor.seek(SeekFrom::Start(offset))?;
            cursor.read_exact(&mut data)?;
            feature_sections.insert(feature, data);
        }

        let attributes =
            if let Some(event_desc_section) = feature_sections.get(&FlagFeature::EventDesc) {
                AttributeDescription::parse_event_desc_section::<_, T>(&event_desc_section[..])?
            } else if header.event_types_section.size != 0 {
                AttributeDescription::parse_event_types_section::<_, T>(
                    &mut cursor,
                    &header.event_types_section,
                    header.attr_size,
                )?
            } else {
                AttributeDescription::parse_attr_section::<_, T>(
                    &mut cursor,
                    &header.attr_section,
                    header.attr_size,
                )?
            };

        if attributes.is_empty() {
            return Err(Error::NoAttributes);
        }

        let mut event_id_to_attr_index = HashMap::new();
        for (attr_index, AttributeDescription { event_ids, .. }) in attributes.iter().enumerate() {
            for event_id in event_ids {
                event_id_to_attr_index.insert(*event_id, attr_index);
            }
        }

        if attributes.len() > 1 {
            // Make sure that all attributes have IDENTIFIER and the same SAMPLE_ID_ALL setting.
            // Otherwise we won't be able to know which attr a record belongs to; we need to know
            // the record's event ID for that, and we can only read the event ID if it's in the
            // same location regardless of attr.
            let has_sample_id_all = attributes[0].attr.flags.contains(AttrFlags::SAMPLE_ID_ALL);
            for (attr_index, AttributeDescription { attr, .. }) in attributes.iter().enumerate() {
                if !attr.sample_format.contains(SampleFormat::IDENTIFIER) {
                    return Err(Error::NoIdentifierDespiteMultiEvent(attr_index));
                }
                if attr.flags.contains(AttrFlags::SAMPLE_ID_ALL) != has_sample_id_all {
                    return Err(Error::InconsistentSampleIdAllWithMultiEvent(attr_index));
                }
            }
        }

        let parse_infos = attributes
            .iter()
            .map(|attr| RecordParseInfo::new(&attr.attr, endian))
            .collect();

        // Move the cursor to the start of the data section so that we can start
        // reading records from it.
        cursor.seek(SeekFrom::Start(header.data_section.offset))?;

        Ok(Self {
            reader: cursor,
            endian,
            attributes,
            parse_infos,
            feature_flags: header.flags,
            feature_sections,
            read_offset: 0,
            record_data_len: header.data_section.size,
            event_id_to_attr_index,
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
    pub fn attributes(&self) -> &[AttributeDescription] {
        &self.attributes
    }

    /// The set of feature flags used in this perf file.
    pub fn feature_flags(&self) -> FlagFeatureSet {
        self.feature_flags
    }

    /// The raw data of a feature section.
    pub fn feature_section(&self, feature: FlagFeature) -> Option<&[u8]> {
        self.feature_sections.get(&feature).map(Deref::deref)
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
        let mut cursor = section_data;
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

    /// The timestamp of the first and the last sample in this file.
    pub fn sample_time_range(&self) -> Result<Option<SampleTimeRange>, Error> {
        let section_data = match self.feature_section(FlagFeature::SampleTime) {
            Some(section) => section,
            None => return Ok(None),
        };
        let time_range = match self.endian {
            Endianness::LittleEndian => SampleTimeRange::parse::<_, LittleEndian>(section_data)?,
            Endianness::BigEndian => SampleTimeRange::parse::<_, BigEndian>(section_data)?,
        };
        Ok(Some(time_range))
    }

    /// Only call this for features whose section is just a perf_header_string.
    fn feature_string(&self, feature: FlagFeature) -> Result<Option<&str>, Error> {
        match self.feature_section(feature) {
            Some(section) => Ok(Some(self.read_string(section)?.0)),
            None => Ok(None),
        }
    }

    /// The hostname where the data was collected (`uname -n`).
    pub fn hostname(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::Hostname)
    }

    /// The OS release where the data was collected (`uname -r`).
    pub fn os_release(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::OsRelease)
    }

    /// The perf user tool version where the data was collected. This is the same
    /// as the version of the Linux source tree the perf tool was built from.
    pub fn perf_version(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::Version)
    }

    /// The CPU architecture (`uname -m`).
    pub fn arch(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::Arch)
    }

    /// A structure defining the number of CPUs.
    pub fn nr_cpus(&self) -> Result<Option<NrCpus>, Error> {
        self.feature_section(FlagFeature::NrCpus)
            .map(|section| {
                Ok(match self.endian {
                    Endianness::LittleEndian => NrCpus::parse::<_, LittleEndian>(section),
                    Endianness::BigEndian => NrCpus::parse::<_, BigEndian>(section),
                }?)
            })
            .transpose()
    }

    /// The description of the CPU. On x86 this is the model name
    /// from `/proc/cpuinfo`.
    pub fn cpu_desc(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::CpuDesc)
    }

    /// The exact CPU type. On x86 this is `vendor,family,model,stepping`.
    /// For example: `GenuineIntel,6,69,1`
    pub fn cpu_id(&self) -> Result<Option<&str>, Error> {
        self.feature_string(FlagFeature::CpuId)
    }

    /// If true, the data section contains data recorded from `perf stat record`.
    pub fn is_stats(&self) -> bool {
        self.feature_flags.has_flag(FlagFeature::Stat)
    }

    /// The perf arg-vector used to collect the data.
    pub fn cmdline(&self) -> Result<Option<Vec<&str>>, Error> {
        match self.feature_section(FlagFeature::Cmdline) {
            Some(section) => Ok(Some(self.read_string_list(section)?.0)),
            None => Ok(None),
        }
    }

    /// The total memory in kilobytes. (MemTotal from /proc/meminfo)
    pub fn total_mem(&self) -> Result<Option<u64>, Error> {
        let data = match self.feature_section(FlagFeature::TotalMem) {
            Some(data) => data,
            None => return Ok(None),
        };
        if data.len() < 8 {
            return Err(Error::FeatureSectionTooSmall);
        }
        let b = data;
        let data = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
        let mem = match self.endian {
            Endianness::LittleEndian => u64::from_le_bytes(data),
            Endianness::BigEndian => u64::from_be_bytes(data),
        };
        Ok(Some(mem))
    }

    fn read_string<'s>(&self, s: &'s [u8]) -> Result<(&'s str, &'s [u8]), Error> {
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
        if rest.len() < len {
            return Err(Error::StringLengthTooLong);
        }
        let (s, rest) = rest.split_at(len);
        let actual_len = memchr::memchr(0, s).unwrap_or(s.len());
        let s = std::str::from_utf8(&s[..actual_len]).map_err(|_| Error::StringUtf8)?;
        Ok((s, rest))
    }

    fn read_string_list<'s>(&self, s: &'s [u8]) -> Result<(Vec<&'s str>, &'s [u8]), Error> {
        if s.len() < 4 {
            return Err(Error::NotEnoughSpaceForStringListLen);
        }
        let (len_bytes, mut rest) = s.split_at(4);
        let len_bytes = [len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]];
        let len = match self.endian {
            Endianness::LittleEndian => u32::from_le_bytes(len_bytes),
            Endianness::BigEndian => u32::from_be_bytes(len_bytes),
        };
        let len = usize::try_from(len).map_err(|_| Error::StringListLengthBiggerThanUsize)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            let s;
            (s, rest) = self.read_string(rest)?;
            vec.push(s);
        }

        Ok((vec, rest))
    }

    /// Emits records in the correct order (sorted by time).
    ///
    /// It buffers records until it sees a FINISHED_ROUND record; then it sorts the
    /// buffered records and emits them one by one.
    pub fn next_record(&mut self) -> Result<Option<(usize, RawRecord)>, Error> {
        if self.remaining_pending_records.is_empty() {
            self.read_current_round()?;
        }
        if let Some(pending_record) = self.remaining_pending_records.pop_front() {
            return Ok(Some(self.convert_pending_record(pending_record)));
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

            let data = RawData::from(&buffer[..]);

            let attr_index = if self.attributes.len() > 1 {
                // We have IDENTIFIER (guaranteed by check in parser).
                let sample_id_all = self.attributes[0]
                    .attr
                    .flags
                    .contains(AttrFlags::SAMPLE_ID_ALL);
                get_record_event_identifier::<T>(record_type, data, sample_id_all)
                    .and_then(|event_id| self.event_id_to_attr_index.get(&event_id).cloned())
                    .unwrap_or(0)
            } else {
                0
            };

            let parse_info = self.parse_infos[attr_index];

            let misc = header.misc;
            let raw_event = RawRecord {
                record_type,
                misc,
                data,
                parse_info,
            };
            let timestamp = raw_event.timestamp();
            let sort_key = RecordSortKey { timestamp, offset };
            let pending_record = PendingRecord {
                sort_key,
                record_type,
                misc,
                buffer,
                attr_index,
            };
            self.remaining_pending_records.push_back(pending_record);
        }

        self.remaining_pending_records
            .make_contiguous()
            .sort_unstable_by_key(|r| r.sort_key);
        Ok(())
    }

    /// Converts pending_record into an RawRecord which references the data in self.current_event_body.
    fn convert_pending_record(&mut self, pending_record: PendingRecord) -> (usize, RawRecord) {
        let PendingRecord {
            record_type,
            misc,
            buffer,
            attr_index,
            ..
        } = pending_record;
        let prev_buffer = std::mem::replace(&mut self.current_event_body, buffer);
        self.buffers_for_recycling.push_back(prev_buffer);
        (
            attr_index,
            RawRecord {
                record_type,
                misc,
                data: RawData::from(&self.current_event_body[..]),
                parse_info: self.parse_infos[attr_index],
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRecord {
    sort_key: RecordSortKey,
    record_type: RecordType,
    misc: u16,
    buffer: Vec<u8>,
    attr_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RecordSortKey {
    timestamp: Option<u64>,
    offset: u64,
}
