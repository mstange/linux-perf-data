//! A parser for the perf.data file format.
//!
//! Files of this format consist of a header, a data section, and a few other
//! supplemental sections. The data section contains the main content of the
//! file: a sequence of records.
//!
//! There are two types of records: event records from the kernel, and "user
//! records" from perf / simpleperf.
//!
//! # Example
//!
//! ```
//! use linux_perf_data::{AttributeDescription, PerfFileReader, PerfFileRecord};
//!
//! # fn wrapper() -> Result<(), linux_perf_data::Error> {
//! let file = std::fs::File::open("perf.data")?;
//! let reader = std::io::BufReader::new(file);
//! let mut perf_file = PerfFileReader::parse_file(reader)?;
//! let event_names: Vec<_> =
//!     perf_file.event_attributes().iter().filter_map(AttributeDescription::name).collect();
//! println!("perf events: {}", event_names.join(", "));
//!
//! while let Some(record) = perf_file.next_record()? {
//!     match record {
//!         PerfFileRecord::EventRecord { attr_index, record } => {
//!             let record_type = record.record_type;
//!             let parsed_record = record.parse()?;
//!             println!("{:?} for event {}: {:?}", record_type, attr_index, parsed_record);
//!         }
//!         PerfFileRecord::UserRecord(record) => {
//!             let record_type = record.record_type;
//!             let parsed_record = record.parse()?;
//!             println!("{:?}: {:?}", record_type, parsed_record);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```

mod build_id_event;
mod constants;
mod dso_key;
mod error;
mod feature_sections;
mod features;
mod perf_file;
mod record;
mod sorter;
mod thread_map;

pub use dso_key::DsoKey;
pub use error::{Error, ReadError};
pub use feature_sections::{AttributeDescription, NrCpus, SampleTimeRange};
pub use features::{Feature, FeatureSet, FeatureSetIter};
pub use record::{PerfFileRecord, RawUserRecord, UserRecord, UserRecordType};
pub use thread_map::ThreadMap;

/// This is a re-export of the linux-perf-event-reader crate. We use its types
/// in our public API.
pub use linux_perf_event_reader;

pub use linux_perf_event_reader::Endianness;

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::ops::Deref;

use build_id_event::BuildIdEvent;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linear_map::LinearMap;
use linux_perf_event_reader::records::{
    get_record_id, get_record_identifier, get_record_timestamp, RawEventRecord, RecordIdParseInfo,
    RecordParseInfo,
};
use linux_perf_event_reader::{
    AttrFlags, CpuMode, PerfEventHeader, RawData, RecordType, SampleFormat,
};
use perf_file::{PerfFileSection, PerfHeader};
use sorter::Sorter;

/// A parser for the perf.data file format.
///
/// # Example
///
/// ```
/// use linux_perf_data::{AttributeDescription, PerfFileReader, PerfFileRecord};
///
/// # fn wrapper() -> Result<(), linux_perf_data::Error> {
/// let file = std::fs::File::open("perf.data")?;
/// let reader = std::io::BufReader::new(file);
/// let mut perf_file = PerfFileReader::parse_file(reader)?;
/// let event_names: Vec<_> =
///     perf_file.event_attributes().iter().filter_map(AttributeDescription::name).collect();
/// println!("perf events: {}", event_names.join(", "));
///
/// while let Some(record) = perf_file.next_record()? {
///     match record {
///         PerfFileRecord::EventRecord { attr_index, record } => {
///             let record_type = record.record_type;
///             let parsed_record = record.parse()?;
///             println!("{:?} for event {}: {:?}", record_type, attr_index, parsed_record);
///         }
///         PerfFileRecord::UserRecord(record) => {
///             let record_type = record.record_type;
///             let parsed_record = record.parse()?;
///             println!("{:?}: {:?}", record_type, parsed_record);
///         }
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct PerfFileReader<R: Read> {
    reader: R,
    endian: Endianness,
    features: FeatureSet,
    feature_sections: LinearMap<Feature, Vec<u8>>,
    read_offset: u64,
    record_data_len: u64,
    current_event_body: Vec<u8>,
    /// Guaranteed to have at least one element
    attributes: Vec<AttributeDescription>,
    id_parse_infos: IdParseInfos,
    /// Guaranteed to have at least one element
    parse_infos: Vec<RecordParseInfo>,
    event_id_to_attr_index: HashMap<u64, usize>,
    sorter: Sorter<RecordSortKey, PendingRecord>,
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
        // Read the section information for each feature, starting just after the data section.
        let feature_pos = header.data_section.offset + header.data_section.size;
        cursor.seek(SeekFrom::Start(feature_pos))?;
        let mut feature_sections_info = Vec::new();
        for feature in header.features.iter() {
            let section = PerfFileSection::parse::<_, T>(&mut cursor)?;
            feature_sections_info.push((feature, section));
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
            if let Some(event_desc_section) = feature_sections.get(&Feature::EVENT_DESC) {
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

        let mut event_id_to_attr_index = HashMap::new();
        for (attr_index, AttributeDescription { event_ids, .. }) in attributes.iter().enumerate() {
            for event_id in event_ids {
                event_id_to_attr_index.insert(*event_id, attr_index);
            }
        }

        let parse_infos: Vec<_> = attributes
            .iter()
            .map(|attr| RecordParseInfo::new(&attr.attr, endian))
            .collect();

        let first_attr = attributes.first().ok_or(Error::NoAttributes)?;

        let first_has_sample_id_all = first_attr.attr.flags.contains(AttrFlags::SAMPLE_ID_ALL);
        let (first_parse_info, remaining_parse_infos) = parse_infos.split_first().unwrap();

        let id_parse_infos = if remaining_parse_infos.is_empty() {
            IdParseInfos::OnlyOneEvent
        } else if remaining_parse_infos
            .iter()
            .all(|parse_info| parse_info.id_parse_info == first_parse_info.id_parse_info)
        {
            IdParseInfos::Same(first_parse_info.id_parse_info)
        } else {
            // Make sure that all attributes have IDENTIFIER and the same SAMPLE_ID_ALL setting.
            // Otherwise we won't be able to know which attr a record belongs to; we need to know
            // the record's ID for that, and we can only read the ID if it's in the same location
            // regardless of attr.
            // In theory we could make the requirements weaker, and take the record type into
            // account for disambiguation. For example, if there are two events, but one of them
            // only creates SAMPLE records and the other only non-SAMPLE records, we don't
            // necessarily need IDENTIFIER in order to be able to read the record ID.
            for (attr_index, AttributeDescription { attr, .. }) in attributes.iter().enumerate() {
                if !attr.sample_format.contains(SampleFormat::IDENTIFIER) {
                    return Err(Error::NoIdentifierDespiteMultiEvent(attr_index));
                }
                if attr.flags.contains(AttrFlags::SAMPLE_ID_ALL) != first_has_sample_id_all {
                    return Err(Error::InconsistentSampleIdAllWithMultiEvent(attr_index));
                }
            }

            IdParseInfos::PerAttribute(first_has_sample_id_all)
        };

        // Move the cursor to the start of the data section so that we can start
        // reading records from it.
        cursor.seek(SeekFrom::Start(header.data_section.offset))?;

        Ok(Self {
            reader: cursor,
            endian,
            attributes,
            parse_infos,
            id_parse_infos,
            features: header.features,
            feature_sections,
            read_offset: 0,
            record_data_len: header.data_section.size,
            event_id_to_attr_index,
            sorter: Sorter::new(),
            buffers_for_recycling: VecDeque::new(),
            current_event_body: Vec::new(),
        })
    }
}

impl<R: Read> PerfFileReader<R> {
    /// The attributes which were requested for each perf event, along with the IDs.
    pub fn event_attributes(&self) -> &[AttributeDescription] {
        &self.attributes
    }

    /// Iterates the records in this file. The records are emitted in the
    /// correct order, i.e. sorted by time.
    ///
    /// `next_record` does some internal buffering so that the sort order can
    /// be guaranteed. This buffering takes advantage of `FINISHED_ROUND`
    /// records so that we don't buffer more records than necessary.
    pub fn next_record(&mut self) -> Result<Option<PerfFileRecord>, Error> {
        if !self.sorter.has_more() {
            self.read_next_round()?;
        }
        if let Some(pending_record) = self.sorter.get_next() {
            return Ok(Some(self.convert_pending_record(pending_record)));
        }
        Ok(None)
    }

    /// Returns a map of build ID entries. `perf record` creates these records for any DSOs
    /// which it thinks have been "hit" in the profile. They supplement Mmap records, which
    /// usually don't come with build IDs.
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
    pub fn build_ids(&self) -> Result<HashMap<DsoKey, DsoInfo>, Error> {
        let section_data = match self.feature_section_data(Feature::BUILD_ID) {
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
            build_ids.insert(dso_key, DsoInfo { path, build_id });
        }
        Ok(build_ids)
    }

    /// The timestamp of the first and the last sample in this file.
    pub fn sample_time_range(&self) -> Result<Option<SampleTimeRange>, Error> {
        let section_data = match self.feature_section_data(Feature::SAMPLE_TIME) {
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
    fn feature_string(&self, feature: Feature) -> Result<Option<&str>, Error> {
        match self.feature_section_data(feature) {
            Some(section) => Ok(Some(self.read_string(section)?.0)),
            None => Ok(None),
        }
    }

    /// The hostname where the data was collected (`uname -n`).
    pub fn hostname(&self) -> Result<Option<&str>, Error> {
        self.feature_string(Feature::HOSTNAME)
    }

    /// The OS release where the data was collected (`uname -r`).
    pub fn os_release(&self) -> Result<Option<&str>, Error> {
        self.feature_string(Feature::OSRELEASE)
    }

    /// The perf user tool version where the data was collected. This is the same
    /// as the version of the Linux source tree the perf tool was built from.
    pub fn perf_version(&self) -> Result<Option<&str>, Error> {
        self.feature_string(Feature::VERSION)
    }

    /// The CPU architecture (`uname -m`).
    pub fn arch(&self) -> Result<Option<&str>, Error> {
        self.feature_string(Feature::ARCH)
    }

    /// A structure defining the number of CPUs.
    pub fn nr_cpus(&self) -> Result<Option<NrCpus>, Error> {
        self.feature_section_data(Feature::NRCPUS)
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
        self.feature_string(Feature::CPUDESC)
    }

    /// The exact CPU type. On x86 this is `vendor,family,model,stepping`.
    /// For example: `GenuineIntel,6,69,1`
    pub fn cpu_id(&self) -> Result<Option<&str>, Error> {
        self.feature_string(Feature::CPUID)
    }

    /// If true, the data section contains data recorded from `perf stat record`.
    pub fn is_stats(&self) -> bool {
        self.features.has_feature(Feature::STAT)
    }

    /// The perf arg-vector used to collect the data.
    pub fn cmdline(&self) -> Result<Option<Vec<&str>>, Error> {
        match self.feature_section_data(Feature::CMDLINE) {
            Some(section) => Ok(Some(self.read_string_list(section)?.0)),
            None => Ok(None),
        }
    }

    /// The total memory in kilobytes. (MemTotal from /proc/meminfo)
    pub fn total_mem(&self) -> Result<Option<u64>, Error> {
        let data = match self.feature_section_data(Feature::TOTAL_MEM) {
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

    /// The set of features used in this perf file.
    pub fn features(&self) -> FeatureSet {
        self.features
    }

    /// The raw data of a feature section.
    pub fn feature_section_data(&self, feature: Feature) -> Option<&[u8]> {
        self.feature_sections.get(&feature).map(Deref::deref)
    }

    /// The file endian.
    pub fn endian(&self) -> Endianness {
        self.endian
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

    /// Reads events into self.sorter until a FINISHED_ROUND record is found
    /// and self.sorter is non-empty, or until we've run out of records to read.
    fn read_next_round(&mut self) -> Result<(), Error> {
        if self.endian == Endianness::LittleEndian {
            self.read_next_round_impl::<byteorder::LittleEndian>()
        } else {
            self.read_next_round_impl::<byteorder::BigEndian>()
        }
    }

    /// Reads events into self.sorter until a FINISHED_ROUND record is found
    /// and self.sorter is non-empty, or until we've run out of records to read.
    fn read_next_round_impl<T: ByteOrder>(&mut self) -> Result<(), Error> {
        while self.read_offset < self.record_data_len {
            let offset = self.read_offset;
            let header = PerfEventHeader::parse::<_, T>(&mut self.reader)?;
            let size = header.size as usize;
            if size < PerfEventHeader::STRUCT_SIZE {
                return Err(Error::InvalidPerfEventSize);
            }
            self.read_offset += u64::from(header.size);

            if UserRecordType::try_from(RecordType(header.type_))
                == Some(UserRecordType::PERF_FINISHED_ROUND)
            {
                self.sorter.finish_round();
                if self.sorter.has_more() {
                    // The sorter is non-empty. We're done.
                    return Ok(());
                }

                // Keep going so that we never exit the loop with sorter
                // being empty, unless we've truly run out of data to read.
                continue;
            }

            let event_body_len = size - PerfEventHeader::STRUCT_SIZE;
            let mut buffer = self.buffers_for_recycling.pop_front().unwrap_or_default();
            buffer.resize(event_body_len, 0);
            self.reader
                .read_exact(&mut buffer)
                .map_err(|_| ReadError::PerfEventData)?;

            let data = RawData::from(&buffer[..]);

            let record_type = RecordType(header.type_);
            let (attr_index, timestamp) = if record_type.is_builtin_type() {
                let attr_index = match &self.id_parse_infos {
                    IdParseInfos::OnlyOneEvent => 0,
                    IdParseInfos::Same(id_parse_info) => {
                        get_record_id::<T>(record_type, data, id_parse_info)
                            .and_then(|id| self.event_id_to_attr_index.get(&id).cloned())
                            .unwrap_or(0)
                    }
                    IdParseInfos::PerAttribute(sample_id_all) => {
                        // We have IDENTIFIER (guaranteed by PerAttribute).
                        get_record_identifier::<T>(record_type, data, *sample_id_all)
                            .and_then(|id| self.event_id_to_attr_index.get(&id).cloned())
                            .unwrap_or(0)
                    }
                };
                let parse_info = self.parse_infos[attr_index];
                let timestamp = get_record_timestamp::<T>(record_type, data, &parse_info);
                (Some(attr_index), timestamp)
            } else {
                // user type
                (None, None)
            };

            let sort_key = RecordSortKey { timestamp, offset };
            let misc = header.misc;
            let pending_record = PendingRecord {
                record_type,
                misc,
                buffer,
                attr_index,
            };
            self.sorter.insert_unordered(sort_key, pending_record);
        }

        // Everything has been read.
        self.sorter.finish();

        Ok(())
    }

    /// Converts pending_record into an RawRecord which references the data in self.current_event_body.
    fn convert_pending_record(&mut self, pending_record: PendingRecord) -> PerfFileRecord {
        let PendingRecord {
            record_type,
            misc,
            buffer,
            attr_index,
            ..
        } = pending_record;
        let prev_buffer = std::mem::replace(&mut self.current_event_body, buffer);
        self.buffers_for_recycling.push_back(prev_buffer);

        let data = RawData::from(&self.current_event_body[..]);

        if let Some(record_type) = UserRecordType::try_from(record_type) {
            let endian = self.endian;
            PerfFileRecord::UserRecord(RawUserRecord {
                record_type,
                misc,
                data,
                endian,
            })
        } else {
            let attr_index = attr_index.unwrap();
            let parse_info = self.parse_infos[attr_index];
            let record = RawEventRecord {
                record_type,
                misc,
                data,
                parse_info,
            };
            PerfFileRecord::EventRecord { attr_index, record }
        }
    }
}

/// The file path and the build ID of a DSO.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DsoInfo {
    /// The file path. Can be an absolute path or a special string
    /// of various forms, e.g. `[vdso]`.
    pub path: Vec<u8>,
    /// The build ID.
    pub build_id: Vec<u8>,
}

#[derive(Debug, Clone)]
enum IdParseInfos {
    /// There is only one event.
    OnlyOneEvent,
    /// There are multiple events, but all events are parsed the same way.
    Same(RecordIdParseInfo),
    /// All elements are guaranteed to have [`SampleFormat::IDENTIFIER`] set in `attr.sample_format`.
    /// The inner element indicates sample_id_all.
    PerAttribute(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRecord {
    record_type: RecordType,
    misc: u16,
    buffer: Vec<u8>,
    attr_index: Option<usize>,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RecordSortKey {
    timestamp: Option<u64>,
    offset: u64,
}
