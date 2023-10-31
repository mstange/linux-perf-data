use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use byteorder::{ByteOrder, ReadBytesExt};
use linear_map::LinearMap;
use linux_perf_event_reader::PerfEventAttr;

use super::section::PerfFileSection;
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
    pub fn parse_event_desc_section<R: Read, T: ByteOrder>(
        mut reader: R,
    ) -> Result<Vec<Self>, std::io::Error> {
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
        let nr = reader.read_u32::<T>()?;
        let mut attributes = Vec::with_capacity(nr as usize);
        let attr_size = reader.read_u32::<T>()?;
        for _ in 0..nr {
            let attr = PerfEventAttr::parse::<_, T>(&mut reader, Some(attr_size))?;
            let nr_ids = reader.read_u32::<T>()?;
            let event_string = HeaderString::parse::<_, T>(&mut reader)?;
            let mut ids = Vec::with_capacity(nr_ids as usize);
            for _ in 0..nr_ids {
                ids.push(reader.read_u64::<T>()?);
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
        mut cursor: C,
        event_types_section: &PerfFileSection,
        attr_size: u64,
    ) -> Result<Vec<Self>, Error> {
        cursor.seek(SeekFrom::Start(event_types_section.offset))?;

        // Each entry in the event_types section is a PerfEventAttr followed by a PerfFileSection.
        let entry_size = attr_size + PerfFileSection::STRUCT_SIZE;
        let entry_count = event_types_section.size / entry_size;
        let mut perf_event_event_type_info = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let attr = PerfEventAttr::parse::<_, T>(&mut cursor, Some(attr_size as u32))
                .map_err(|_| ReadError::PerfEventAttr)?;
            let event_ids = PerfFileSection::parse::<_, T>(&mut cursor)?;
            perf_event_event_type_info.push((attr, event_ids));
        }

        // Read the lists of event IDs for each event type.
        let mut attributes = Vec::new();
        for (attr, section) in perf_event_event_type_info {
            cursor.seek(SeekFrom::Start(section.offset))?;
            // This section is just a list of u64 event IDs.
            let id_count = section.size / 8;
            let mut event_ids = Vec::with_capacity(id_count as usize);
            for _ in 0..id_count {
                event_ids.push(cursor.read_u64::<T>()?);
            }
            attributes.push(AttributeDescription {
                attr,
                name: None,
                event_ids,
            });
        }
        Ok(attributes)
    }

    /// Parse the attr id section of a simpleperf perf.data file. This first parses the offset and
    /// size of the id section from the the corresponding attr section, then parses the id section
    /// itself. This assumes the cursor is currently located at the end of the specified attr in the
    /// attr section.
    fn parse_simpleperf_id_section<C: Read + Seek, T: ByteOrder>(
        cursor: &mut C,
        attr_size: u64,
    ) -> Result<Vec<u64>, Error> {
        // We require the actual size of the perf_event_attr, which is stored as a u32 at an offset
        // of 4 bytes from the start of the attr.
        cursor.seek(SeekFrom::Current(4 - attr_size as i64))?;
        let actual_size = cursor.read_u32::<T>()? as u64;
        // The offset and size of the ID section are stored as two u64s immediately after the actual
        // size of the attr.
        if actual_size > attr_size || attr_size - actual_size < 16 {
            return Err(ReadError::PerfEventAttr.into());
        }
        cursor.seek(SeekFrom::Current(actual_size as i64 - 8))?;
        let id_offset = cursor.read_u64::<T>()?;
        let id_size = cursor.read_u64::<T>()?;

        let num_event_ids = id_size as usize / std::mem::size_of::<u64>();
        let mut event_ids = Vec::with_capacity(num_event_ids);
        cursor.seek(SeekFrom::Start(id_offset))?;
        for _ in 0..num_event_ids {
            event_ids.push(cursor.read_u64::<T>()?);
        }
        Ok(event_ids)
    }

    /// Parse the `attr` section of a perf.data file into a Vec of `AttributeDescription` structs.
    /// This section is normally used as a last resort because it does not have any
    /// information about event IDs. If multiple events are observed, we will
    /// not be able to know which event record belongs to which attr. For profiles recorded
    /// using simpleperf, however, we may still be able to obtain this information.
    pub fn parse_attr_section<C: Read + Seek, T: ByteOrder>(
        mut cursor: C,
        attr_section: &PerfFileSection,
        attr_size: u64,
        simpleperf_meta_info: &Option<HashMap<&str, &str>>,
    ) -> Result<Vec<Self>, Error> {
        let names = simpleperf_meta_info
            .as_ref()
            .and_then(|info| info.get("event_type_info"))
            .map(|info| info.split('\n'))
            .into_iter()
            .flatten()
            .map(|info| info.split_once(',').map(|(name, _)| name))
            .collect::<Vec<Option<&str>>>();

        cursor.seek(SeekFrom::Start(attr_section.offset))?;
        let attr_count = attr_section.size / attr_size;
        let mut attributes = Vec::with_capacity(attr_count as usize);
        for i in 0..attr_count as usize {
            let attr = PerfEventAttr::parse::<_, T>(&mut cursor, Some(attr_size as u32))
                .map_err(|_| ReadError::PerfEventAttr)?;

            let name = names.get(i).and_then(Option::as_deref).map(str::to_string);

            let event_ids = if simpleperf_meta_info.is_some() {
                let pos = cursor.stream_position()?;
                let event_ids = Self::parse_simpleperf_id_section::<_, T>(&mut cursor, attr_size)?;
                cursor.seek(SeekFrom::Start(pos))?;
                event_ids
            } else {
                vec![]
            };

            attributes.push(AttributeDescription {
                attr,
                name,
                event_ids,
            });
        }
        Ok(attributes)
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
pub struct PmuMappings;

impl PmuMappings {
    pub fn parse<R: Read, T: ByteOrder>(
        mut reader: R,
    ) -> Result<LinearMap<u32, String>, std::io::Error> {
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
        Ok(vec.into_iter().collect())
    }
}
