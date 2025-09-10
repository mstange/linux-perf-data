use std::io::{Read, Seek, SeekFrom};

use byteorder::{ByteOrder, ReadBytesExt};
use linear_map::LinearMap;
use linux_perf_event_reader::PerfEventAttr;

use super::section::PerfFileSection;
use crate::simpleperf::SimplePerfEventType;
use crate::{Error, ReadError};

/// The number of available and online CPUs. (`nr_cpus`)
#[derive(Debug, Clone, Copy)]
pub struct NrCpus {
    /// CPUs not yet onlined
    pub nr_cpus_available: u32,
    pub nr_cpus_online: u32,
}

impl NrCpus {
    pub const STRUCT_SIZE: usize = 4 + 4;

    pub fn parse<R: Read, T: ByteOrder>(mut reader: R) -> Result<Self, std::io::Error> {
        let nr_cpus_available = reader.read_u32::<T>()?;
        let nr_cpus_online = reader.read_u32::<T>()?;
        Ok(Self {
            nr_cpus_available,
            nr_cpus_online,
        })
    }
}

/// The timestamps of the first and last sample.
#[derive(Debug, Clone, Copy)]
pub struct SampleTimeRange {
    pub first_sample_time: u64,
    pub last_sample_time: u64,
}

impl SampleTimeRange {
    pub const STRUCT_SIZE: usize = 8 + 8;

    pub fn parse<R: Read, T: ByteOrder>(mut reader: R) -> Result<Self, std::io::Error> {
        let first_sample_time = reader.read_u64::<T>()?;
        let last_sample_time = reader.read_u64::<T>()?;
        Ok(Self {
            first_sample_time,
            last_sample_time,
        })
    }
}

pub struct HeaderString;

impl HeaderString {
    /// Parse a string.
    pub fn parse<R: Read, T: ByteOrder>(mut reader: R) -> Result<Option<String>, std::io::Error> {
        let len = reader.read_u32::<T>()?;
        let mut s = vec![0; len as usize];
        reader.read_exact(&mut s)?;
        let actual_len = memchr::memchr(0, &s).unwrap_or(s.len());
        s.truncate(actual_len);
        Ok(String::from_utf8(s).ok())
    }
}

/// A single event attr with name and corresponding event IDs.
#[derive(Debug, Clone)]
pub struct AttributeDescription {
    pub attr: PerfEventAttr,
    pub name: Option<String>,
    pub event_ids: Vec<u64>,
}

impl AttributeDescription {
    /// Parse the `HEADER_EVENT_DESC` section of a perf.data file into a Vec of `AttributeDescription` structs.
    pub fn parse_event_desc_section<C: Read + Seek, T: ByteOrder>(
        mut cursor: C,
    ) -> Result<Vec<Self>, Error> {
        // ```c
        // struct {
        //   uint32_t nr; /* number of events */
        //   uint32_t attr_size; /* size of each perf_event_attr */
        //   struct {
        //     struct perf_event_attr attr;  /* size of attr_size */
        //     uint32_t nr_ids;
        //     struct perf_header_string event_string;
        //     uint64_t ids[nr_ids];
        //   } events[nr]; /* Variable length records */
        // };
        // ```
        let nr = cursor.read_u32::<T>()?;
        let mut attributes = Vec::with_capacity(nr as usize);
        let attr_size = cursor.read_u32::<T>()? as u64;
        for _ in 0..nr {
            let attr = Self::parse_single_attr::<_, T>(&mut cursor, attr_size)?;
            let nr_ids = cursor.read_u32::<T>()?;
            let event_string = HeaderString::parse::<_, T>(&mut cursor)?;
            let mut ids = Vec::with_capacity(nr_ids as usize);
            for _ in 0..nr_ids {
                ids.push(cursor.read_u64::<T>()?);
            }
            attributes.push(AttributeDescription {
                attr,
                name: event_string,
                event_ids: ids,
            });
        }
        Ok(attributes)
    }

    /// Parse the `event_types` section of a perf.data file into a Vec of `AttributeDescription` structs.
    /// This section was used in the past but is no longer used.
    /// Only call this function if event_types_section.size is non-zero.
    pub fn parse_event_types_section<C: Read + Seek, T: ByteOrder>(
        cursor: C,
        event_types_section: &PerfFileSection,
        attr_size: u64,
    ) -> Result<Vec<Self>, Error> {
        // In the event_types section, each attribute takes up attr_size bytes and is followed
        // by a PerfFileSection struct (16 bytes).
        Self::parse_sequence_of_attr_and_id_section::<C, T>(
            cursor,
            event_types_section,
            attr_size,
            None,
        )
    }

    /// Parse the `attr` section of a perf.data file into a Vec of `AttributeDescription` structs,
    /// for files from Simpleperf. These files pack event ID information into the `attr` section
    /// and contain event names in the `SIMPLEPERF_META_INFO` section.
    pub fn parse_simpleperf_attr_section<C: Read + Seek, T: ByteOrder>(
        cursor: C,
        attr_section: &PerfFileSection,
        attr_size: u64,
        event_types: &[SimplePerfEventType],
    ) -> Result<Vec<Self>, Error> {
        if attr_size < PerfFileSection::STRUCT_SIZE {
            return Err(ReadError::PerfEventAttr.into());
        }
        // Simpleperf reports an attr_size which is 16 bytes larger than the size that's used
        // for the perf_event_attr data. These 16 extra bytes carry the (offset, size) of the
        // per-event event IDs section.
        // So the format of the attr section in the simpleperf is very similar to the format of the
        // event_types section in old perf.data files, with the only difference being that the
        // id_section information is "inside" the attr_size rather than outside it.
        let attr_size_without_id_section = attr_size - PerfFileSection::STRUCT_SIZE;
        let event_names: Vec<_> = event_types.iter().map(|t| t.name.as_str()).collect();
        Self::parse_sequence_of_attr_and_id_section::<C, T>(
            cursor,
            attr_section,
            attr_size_without_id_section,
            Some(&event_names),
        )
    }

    /// Used for parsing the `event_types` section (old Linux perf) and for parsing the `attr` section (Simpleperf).
    fn parse_sequence_of_attr_and_id_section<C: Read + Seek, T: ByteOrder>(
        mut cursor: C,
        section: &PerfFileSection,
        attr_size: u64,
        event_names: Option<&[&str]>,
    ) -> Result<Vec<Self>, Error> {
        cursor.seek(SeekFrom::Start(section.offset))?;

        // Each entry in the event_types section is a PerfEventAttr followed by a PerfFileSection.
        let entry_size = attr_size + PerfFileSection::STRUCT_SIZE;
        let entry_count = section.size / entry_size;
        let mut perf_event_event_type_info = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let attr = Self::parse_single_attr::<_, T>(&mut cursor, attr_size)?;
            let event_ids = PerfFileSection::parse::<_, T>(&mut cursor)?;
            perf_event_event_type_info.push((attr, event_ids));
        }

        // Read the lists of event IDs for each event type.
        let mut attributes = Vec::new();
        for (event_index, (attr, section)) in perf_event_event_type_info.into_iter().enumerate() {
            cursor.seek(SeekFrom::Start(section.offset))?;
            // This section is just a list of u64 event IDs.
            let id_count = section.size / 8;
            let mut event_ids = Vec::with_capacity(id_count as usize);
            for _ in 0..id_count {
                event_ids.push(cursor.read_u64::<T>()?);
            }
            let name = if let Some(names) = event_names {
                names.get(event_index).map(|s| s.to_string())
            } else {
                None
            };
            attributes.push(AttributeDescription {
                attr,
                name,
                event_ids,
            });
        }
        Ok(attributes)
    }

    /// Parse the `attr` section of a perf.data file into a Vec of `AttributeDescription` structs.
    /// This section is used as a last resort because it does not have any
    /// information about event IDs. If multiple events are observed, we will
    /// not be able to know which event record belongs to which attr.
    pub fn parse_attr_section<C: Read + Seek, T: ByteOrder>(
        mut cursor: C,
        attr_section: &PerfFileSection,
        attr_size: u64,
    ) -> Result<Vec<Self>, Error> {
        cursor.seek(SeekFrom::Start(attr_section.offset))?;
        let attr_count = attr_section.size / attr_size;
        let mut attributes = Vec::with_capacity(attr_count as usize);
        for _ in 0..attr_count {
            let attr = Self::parse_single_attr::<_, T>(&mut cursor, attr_size)?;
            attributes.push(AttributeDescription {
                attr,
                name: None,
                event_ids: vec![],
            });
        }
        Ok(attributes)
    }

    fn parse_single_attr<C: Read + Seek, T: ByteOrder>(
        mut cursor: C,
        attr_size: u64,
    ) -> Result<PerfEventAttr, Error> {
        let (attr, size) =
            PerfEventAttr::parse::<_, T>(&mut cursor).map_err(|_| ReadError::PerfEventAttr)?;
        if size > attr_size {
            return Err(Error::InconsistentAttributeSizes(size, attr_size));
        }
        if size < attr_size {
            let remaining_bytes = attr_size - size;
            cursor.seek(SeekFrom::Current(remaining_bytes as i64))?;
        }
        Ok(attr)
    }

    /// The event attributes.
    pub fn attributes(&self) -> &PerfEventAttr {
        &self.attr
    }

    /// The event name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The IDs for this event.
    pub fn ids(&self) -> &[u64] {
        &self.event_ids
    }
}

/// The names of the dynamic PMU types used in [`PerfEventType::DynamicPmu`](linux_perf_event_reader::PerfEventType::DynamicPmu).
///
/// For example, this allows you to find out whether a `DynamicPmu`
/// perf event is a kprobe or a uprobe, which then lets you interpret
/// the meaning of the config fields.
pub struct PmuMappings(pub LinearMap<u32, String>);

impl PmuMappings {
    pub fn parse<R: Read, T: ByteOrder>(mut reader: R) -> Result<Self, std::io::Error> {
        // struct {
        //     uint32_t nr;
        //     struct pmu {
        //        uint32_t pmu_type;
        //        struct perf_header_string pmu_name;
        //     } [nr]; /* Variable length records */
        // };
        let nr = reader.read_u32::<T>()?;
        let mut vec = Vec::with_capacity(nr as usize);
        for _ in 0..nr {
            let pmu_type = reader.read_u32::<T>()?;
            if let Some(pmu_name) = HeaderString::parse::<_, T>(&mut reader)? {
                vec.push((pmu_type, pmu_name));
            }
        }
        vec.sort_by_key(|item| item.0);
        Ok(Self(vec.into_iter().collect()))
    }
}

/// The clock data header contains information about the clock used to
/// record timestamps in the perf.data file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockData {
    /// Clock ID used for timestamps
    pub clockid: u32,
    /// Wall clock time in nanoseconds since Unix epoch.
    pub wall_clock_ns: u64,
    /// Clock ID time in nanoseconds at the same instant as `wall_clock_ns`.
    pub clockid_time_ns: u64,
}

impl ClockData {
    pub const STRUCT_SIZE: usize = 4 + 4 + 8 + 8;

    pub fn parse<R: Read, T: ByteOrder>(mut data: R) -> Result<Self, std::io::Error> {
        let version = data.read_u32::<T>()?;
        if version != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported clock data version: {version}"),
            ));
        }
        let clockid = data.read_u32::<T>()?;
        let wall_clock_ns = data.read_u64::<T>()?;
        let clockid_time_ns = data.read_u64::<T>()?;

        Ok(Self {
            clockid,
            wall_clock_ns,
            clockid_time_ns,
        })
    }
}
