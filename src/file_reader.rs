use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linear_map::LinearMap;
use linux_perf_event_reader::{
    get_record_id, get_record_identifier, get_record_timestamp, AttrFlags, Endianness,
    PerfEventHeader, RawData, RawEventRecord, RecordIdParseInfo, RecordParseInfo, RecordType,
    SampleFormat,
};

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};

use super::error::{Error, ReadError};
use super::feature_sections::AttributeDescription;
use super::features::Feature;
use super::header::PerfHeader;
use super::perf_file::PerfFile;
use super::record::{PerfFileRecord, RawUserRecord, UserRecordType};
use super::section::PerfFileSection;
use super::simpleperf;
use super::sorter::Sorter;

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
/// let PerfFileReader { mut perf_file, mut record_iter } = PerfFileReader::parse_file(reader)?;
/// let event_names: Vec<_> =
///     perf_file.event_attributes().iter().filter_map(AttributeDescription::name).collect();
/// println!("perf events: {}", event_names.join(", "));
///
/// while let Some(record) = record_iter.next_record(&mut perf_file)? {
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
    pub perf_file: PerfFile,
    pub record_iter: PerfRecordIter<R>,
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
            } else if let Some(simpleperf_meta_info) =
                feature_sections.get(&Feature::SIMPLEPERF_META_INFO)
            {
                let info_map = simpleperf::parse_meta_info_map(&simpleperf_meta_info[..])?;
                let event_types = simpleperf::get_event_types(&info_map)
                    .ok_or(Error::NoEventTypesInSimpleperfMetaInfo)?;
                AttributeDescription::parse_simpleperf_attr_section::<_, T>(
                    &mut cursor,
                    &header.attr_section,
                    header.attr_size,
                    &event_types,
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

        let perf_file = PerfFile {
            endian,
            features: header.features,
            feature_sections,
            attributes,
        };

        let record_iter = PerfRecordIter {
            reader: cursor,
            endian,
            id_parse_infos,
            parse_infos,
            event_id_to_attr_index,
            read_offset: 0,
            record_data_len: header.data_section.size,
            sorter: Sorter::new(),
            buffers_for_recycling: VecDeque::new(),
            current_event_body: Vec::new(),
        };

        Ok(Self {
            perf_file,
            record_iter,
        })
    }
}

/// An iterator which incrementally reads and sorts the records from a perf.data file.
pub struct PerfRecordIter<R: Read> {
    reader: R,
    endian: Endianness,
    read_offset: u64,
    record_data_len: u64,
    current_event_body: Vec<u8>,
    id_parse_infos: IdParseInfos,
    /// Guaranteed to have at least one element
    parse_infos: Vec<RecordParseInfo>,
    event_id_to_attr_index: HashMap<u64, usize>,
    sorter: Sorter<RecordSortKey, PendingRecord>,
    buffers_for_recycling: VecDeque<Vec<u8>>,
}

impl<R: Read> PerfRecordIter<R> {
    /// Iterates the records in this file. The records are emitted in the
    /// correct order, i.e. sorted by time.
    ///
    /// `next_record` does some internal buffering so that the sort order can
    /// be guaranteed. This buffering takes advantage of `FINISHED_ROUND`
    /// records so that we don't buffer more records than necessary.
    pub fn next_record(
        &mut self,
        _perf_file: &mut PerfFile,
    ) -> Result<Option<PerfFileRecord>, Error> {
        if !self.sorter.has_more() {
            self.read_next_round()?;
        }
        if let Some(pending_record) = self.sorter.get_next() {
            let record = self.convert_pending_record(pending_record);
            return Ok(Some(record));
        }
        Ok(None)
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
