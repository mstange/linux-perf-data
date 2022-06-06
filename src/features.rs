use std::fmt;

pub const HEADER_TRACING_DATA: u32 = 1;
pub const HEADER_BUILD_ID: u32 = 2;
pub const HEADER_HOSTNAME: u32 = 3;
pub const HEADER_OSRELEASE: u32 = 4;
pub const HEADER_VERSION: u32 = 5;
pub const HEADER_ARCH: u32 = 6;
pub const HEADER_NRCPUS: u32 = 7;
pub const HEADER_CPUDESC: u32 = 8;
pub const HEADER_CPUID: u32 = 9;
pub const HEADER_TOTAL_MEM: u32 = 10;
pub const HEADER_CMDLINE: u32 = 11;
pub const HEADER_EVENT_DESC: u32 = 12;
pub const HEADER_CPU_TOPOLOGY: u32 = 13;
pub const HEADER_NUMA_TOPOLOGY: u32 = 14;
pub const HEADER_BRANCH_STACK: u32 = 15;
pub const HEADER_PMU_MAPPINGS: u32 = 16;
pub const HEADER_GROUP_DESC: u32 = 17;
pub const HEADER_AUXTRACE: u32 = 18;
pub const HEADER_STAT: u32 = 19;
pub const HEADER_CACHE: u32 = 20;
pub const HEADER_SAMPLE_TIME: u32 = 21;
pub const HEADER_SAMPLE_TOPOLOGY: u32 = 22;
pub const HEADER_CLOCKID: u32 = 23;
pub const HEADER_DIR_FORMAT: u32 = 24;
pub const HEADER_BPF_PROG_INFO: u32 = 25;
pub const HEADER_BPF_BTF: u32 = 26;
pub const HEADER_COMPRESSED: u32 = 27;
pub const HEADER_CPU_PMU_CAPS: u32 = 28;
pub const HEADER_CLOCK_DATA: u32 = 29;
pub const HEADER_HYBRID_TOPOLOGY: u32 = 30;
pub const HEADER_HYBRID_CPU_PMU_CAPS: u32 = 31;

/// simpleperf `FEAT_META_INFO`
pub const HEADER_SIMPLEPERF_META_INFO: u32 = 128;
/// simpleperf `FEAT_DEBUG_UNWIND`
pub const HEADER_SIMPLEPERF_DEBUG_UNWIND: u32 = 129;
/// simpleperf `FEAT_DEBUG_UNWIND_FILE`
pub const HEADER_SIMPLEPERF_DEBUG_UNWIND_FILE: u32 = 130;
/// simpleperf `FEAT_FILE2`
pub const HEADER_SIMPLEPERF_FILE2: u32 = 131;

/// A piece of optional data stored in a perf.data file. Its data is contained in a
/// "feature section" at the end of the file.
///
/// For each used feature, a bit is set in the feature flags in the file header.
/// The feature sections are stored just after the file's data section; there's
/// one section for each enabled feature, ordered from low feature bit to high
/// feature bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    TracingData,
    BuildId,
    Hostname,
    OsRelease,
    Version,
    Arch,
    NrCpus,
    CpuDesc,
    CpuId,
    TotalMem,
    Cmdline,
    EventDesc,
    CpuTopology,
    NumaTopology,
    BranchStack,
    PmuMappings,
    GroupDesc,
    Auxtrace,
    Stat,
    Cache,
    SampleTime,
    SampleTopology,
    ClockId,
    DirFormat,
    BpfProgInfo,
    BpfBtf,
    Compressed,
    CpuPmuCaps,
    ClockData,
    HybridTopology,
    HybridCpuPmuCaps,
    SimpleperfMetaInfo,
    SimpleperfDebugUnwind,
    SimpleperfDebugUnwindFile,
    SimpleperfFile2,
}

impl Feature {
    pub fn from_int(i: u32) -> Option<Self> {
        let feature = match i {
            HEADER_TRACING_DATA => Self::TracingData,
            HEADER_BUILD_ID => Self::BuildId,
            HEADER_HOSTNAME => Self::Hostname,
            HEADER_OSRELEASE => Self::OsRelease,
            HEADER_VERSION => Self::Version,
            HEADER_ARCH => Self::Arch,
            HEADER_NRCPUS => Self::NrCpus,
            HEADER_CPUDESC => Self::CpuDesc,
            HEADER_CPUID => Self::CpuId,
            HEADER_TOTAL_MEM => Self::TotalMem,
            HEADER_CMDLINE => Self::Cmdline,
            HEADER_EVENT_DESC => Self::EventDesc,
            HEADER_CPU_TOPOLOGY => Self::CpuTopology,
            HEADER_NUMA_TOPOLOGY => Self::NumaTopology,
            HEADER_BRANCH_STACK => Self::BranchStack,
            HEADER_PMU_MAPPINGS => Self::PmuMappings,
            HEADER_GROUP_DESC => Self::GroupDesc,
            HEADER_AUXTRACE => Self::Auxtrace,
            HEADER_STAT => Self::Stat,
            HEADER_CACHE => Self::Cache,
            HEADER_SAMPLE_TIME => Self::SampleTime,
            HEADER_SAMPLE_TOPOLOGY => Self::SampleTopology,
            HEADER_CLOCKID => Self::ClockId,
            HEADER_DIR_FORMAT => Self::DirFormat,
            HEADER_BPF_PROG_INFO => Self::BpfProgInfo,
            HEADER_BPF_BTF => Self::BpfBtf,
            HEADER_COMPRESSED => Self::Compressed,
            HEADER_CPU_PMU_CAPS => Self::CpuPmuCaps,
            HEADER_CLOCK_DATA => Self::ClockData,
            HEADER_HYBRID_TOPOLOGY => Self::HybridTopology,
            HEADER_HYBRID_CPU_PMU_CAPS => Self::HybridCpuPmuCaps,
            HEADER_SIMPLEPERF_META_INFO => Self::SimpleperfMetaInfo,
            HEADER_SIMPLEPERF_DEBUG_UNWIND => Self::SimpleperfDebugUnwind,
            HEADER_SIMPLEPERF_DEBUG_UNWIND_FILE => Self::SimpleperfDebugUnwindFile,
            HEADER_SIMPLEPERF_FILE2 => Self::SimpleperfFile2,
            _ => return None,
        };
        Some(feature)
    }
}

impl From<Feature> for u32 {
    fn from(feature: Feature) -> Self {
        match feature {
            Feature::TracingData => HEADER_TRACING_DATA,
            Feature::BuildId => HEADER_BUILD_ID,
            Feature::Hostname => HEADER_HOSTNAME,
            Feature::OsRelease => HEADER_OSRELEASE,
            Feature::Version => HEADER_VERSION,
            Feature::Arch => HEADER_ARCH,
            Feature::NrCpus => HEADER_NRCPUS,
            Feature::CpuDesc => HEADER_CPUDESC,
            Feature::CpuId => HEADER_CPUID,
            Feature::TotalMem => HEADER_TOTAL_MEM,
            Feature::Cmdline => HEADER_CMDLINE,
            Feature::EventDesc => HEADER_EVENT_DESC,
            Feature::CpuTopology => HEADER_CPU_TOPOLOGY,
            Feature::NumaTopology => HEADER_NUMA_TOPOLOGY,
            Feature::BranchStack => HEADER_BRANCH_STACK,
            Feature::PmuMappings => HEADER_PMU_MAPPINGS,
            Feature::GroupDesc => HEADER_GROUP_DESC,
            Feature::Auxtrace => HEADER_AUXTRACE,
            Feature::Stat => HEADER_STAT,
            Feature::Cache => HEADER_CACHE,
            Feature::SampleTime => HEADER_SAMPLE_TIME,
            Feature::SampleTopology => HEADER_SAMPLE_TOPOLOGY,
            Feature::ClockId => HEADER_CLOCKID,
            Feature::DirFormat => HEADER_DIR_FORMAT,
            Feature::BpfProgInfo => HEADER_BPF_PROG_INFO,
            Feature::BpfBtf => HEADER_BPF_BTF,
            Feature::Compressed => HEADER_COMPRESSED,
            Feature::CpuPmuCaps => HEADER_CPU_PMU_CAPS,
            Feature::ClockData => HEADER_CLOCK_DATA,
            Feature::HybridTopology => HEADER_HYBRID_TOPOLOGY,
            Feature::HybridCpuPmuCaps => HEADER_HYBRID_CPU_PMU_CAPS,
            Feature::SimpleperfMetaInfo => HEADER_SIMPLEPERF_META_INFO,
            Feature::SimpleperfDebugUnwind => HEADER_SIMPLEPERF_DEBUG_UNWIND,
            Feature::SimpleperfDebugUnwindFile => HEADER_SIMPLEPERF_DEBUG_UNWIND_FILE,
            Feature::SimpleperfFile2 => HEADER_SIMPLEPERF_FILE2,
        }
    }
}

/// The set of features used in the perf file. The perf file contains one
/// feature section for each feature. This set is provided in the perf
/// file header.
///
/// The set has room for 4 * 64 = 256 feature bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureSet(pub [u64; 4]);

impl FeatureSet {
    /// The number of features in this set.
    pub fn len(&self) -> usize {
        let b = &self.0;
        let len = b[0].count_ones() + b[1].count_ones() + b[2].count_ones() + b[3].count_ones();
        len as usize
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0 == [0, 0, 0, 0]
    }

    /// Emits all features in this set, from low to high.
    pub fn iter(&self) -> FeatureSetIterAll {
        FeatureSetIterAll {
            current_feature: 0,
            set: *self,
        }
    }

    /// Checks if the feature is contained in this set.
    #[inline]
    pub fn has_feature(&self, feature: impl Into<u32>) -> bool {
        let feature: u32 = feature.into();
        if feature >= 256 {
            return false;
        }
        let features_chunk_index = (feature / 64) as usize;
        let feature_bit = feature % 64;
        let features_chunk = self.0[features_chunk_index];
        (features_chunk & (1 << feature_bit)) != 0
    }
}

impl fmt::Debug for FeatureSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut set = f.debug_set();
        for i in self.iter() {
            if let Some(feature) = Feature::from_int(i) {
                set.entry(&feature);
            } else {
                set.entry(&format_args!("Unknown({})", i));
            }
        }
        set.finish()
    }
}

pub struct FeatureSetIterAll {
    current_feature: u32,
    set: FeatureSet,
}

impl Iterator for FeatureSetIterAll {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_feature < 256 {
            let feature = self.current_feature;
            self.current_feature += 1;

            if self.set.has_feature(feature) {
                return Some(feature);
            }
        }
        None
    }
}
