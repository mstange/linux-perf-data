use byteorder::{BigEndian, LittleEndian};
use linear_map::LinearMap;
use linux_perf_event_reader::{CpuMode, Endianness};

use std::collections::HashMap;
use std::ops::Deref;

use super::build_id_event::BuildIdEvent;
use super::dso_info::DsoInfo;
use super::dso_key::DsoKey;
use super::error::Error;
use super::feature_sections::{AttributeDescription, NrCpus, PmuMappings, SampleTimeRange};
use super::features::{Feature, FeatureSet};
use super::simpleperf;

/// Contains the information from the perf.data file header and feature sections.
pub struct PerfFile {
    pub(crate) endian: Endianness,
    pub(crate) features: FeatureSet,
    pub(crate) feature_sections: LinearMap<Feature, Vec<u8>>,
    /// Guaranteed to have at least one element
    pub(crate) attributes: Vec<AttributeDescription>,
}

impl PerfFile {
    /// The attributes which were requested for each perf event, along with the IDs.
    pub fn event_attributes(&self) -> &[AttributeDescription] {
        &self.attributes
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

    /// The meta info map, if this is a Simpleperf profile.
    pub fn simpleperf_meta_info(&self) -> Result<Option<HashMap<&str, &str>>, Error> {
        match self.feature_section_data(Feature::SIMPLEPERF_META_INFO) {
            Some(section) => Ok(Some(simpleperf::parse_meta_info_map(section)?)),
            None => Ok(None),
        }
    }

    /// The names of the dynamic PMU types used in [`PerfEventType::DynamicPmu`](linux_perf_event_reader::PerfEventType::DynamicPmu).
    ///
    /// This mapping allows you to interpret the perf event type field of the perf event
    /// attributes returned by [`PerfFile::event_attributes`].
    ///
    /// For example, let's say you observed a kprobe or a uprobe. The perf event will be
    /// of type `DynamicPmu`, and its dynamic PMU type ID might be 6 or 7.
    ///
    /// Just by seeing this 6 or 7 you don't know for sure what type of event it is.
    /// But the `pmu_mappings()` map will have a 6 => "kprobe" and a 7 => "uprobe" entry.
    /// Once you see those entries, you can be sure what you're dealing with.
    ///
    /// This map also contains the values "software", "tracepoint", and "breakpoint"; those
    /// always have the IDs 1, 2 and 5, respectively.
    ///
    /// Additionally, the map contains the CPU-specific dynamic entries. For example, an Intel
    /// CPU might have IDs for the names "cpu", "intel_bts", "intel_pt", "msr", "uncore_imc",
    /// "uncore_cbox_0", ..., "uncore_cbox_7", "uncore_arb", "cstate_core", "cstate_pkg", "power",
    /// "i915".
    pub fn pmu_mappings(&self) -> Result<Option<LinearMap<u32, String>>, Error> {
        self.feature_section_data(Feature::PMU_MAPPINGS)
            .map(|section| {
                Ok(match self.endian {
                    Endianness::LittleEndian => PmuMappings::parse::<_, LittleEndian>(section),
                    Endianness::BigEndian => PmuMappings::parse::<_, BigEndian>(section),
                }?)
            })
            .transpose()
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
        let s = std::str::from_utf8(&s[..actual_len])?;
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
}
