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
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Feature(pub u32);

impl Feature {
    pub const TRACING_DATA: Self = Self(HEADER_TRACING_DATA);
    pub const BUILD_ID: Self = Self(HEADER_BUILD_ID);
    pub const HOSTNAME: Self = Self(HEADER_HOSTNAME);
    pub const OSRELEASE: Self = Self(HEADER_OSRELEASE);
    pub const VERSION: Self = Self(HEADER_VERSION);
    pub const ARCH: Self = Self(HEADER_ARCH);
    pub const NRCPUS: Self = Self(HEADER_NRCPUS);
    pub const CPUDESC: Self = Self(HEADER_CPUDESC);
    pub const CPUID: Self = Self(HEADER_CPUID);
    pub const TOTAL_MEM: Self = Self(HEADER_TOTAL_MEM);
    pub const CMDLINE: Self = Self(HEADER_CMDLINE);
    pub const EVENT_DESC: Self = Self(HEADER_EVENT_DESC);
    pub const CPU_TOPOLOGY: Self = Self(HEADER_CPU_TOPOLOGY);
    pub const NUMA_TOPOLOGY: Self = Self(HEADER_NUMA_TOPOLOGY);
    pub const BRANCH_STACK: Self = Self(HEADER_BRANCH_STACK);
    pub const PMU_MAPPINGS: Self = Self(HEADER_PMU_MAPPINGS);
    pub const GROUP_DESC: Self = Self(HEADER_GROUP_DESC);
    pub const AUXTRACE: Self = Self(HEADER_AUXTRACE);
    pub const STAT: Self = Self(HEADER_STAT);
    pub const CACHE: Self = Self(HEADER_CACHE);
    pub const SAMPLE_TIME: Self = Self(HEADER_SAMPLE_TIME);
    pub const SAMPLE_TOPOLOGY: Self = Self(HEADER_SAMPLE_TOPOLOGY);
    pub const CLOCKID: Self = Self(HEADER_CLOCKID);
    pub const DIR_FORMAT: Self = Self(HEADER_DIR_FORMAT);
    pub const BPF_PROG_INFO: Self = Self(HEADER_BPF_PROG_INFO);
    pub const BPF_BTF: Self = Self(HEADER_BPF_BTF);
    pub const COMPRESSED: Self = Self(HEADER_COMPRESSED);
    pub const CPU_PMU_CAPS: Self = Self(HEADER_CPU_PMU_CAPS);
    pub const CLOCK_DATA: Self = Self(HEADER_CLOCK_DATA);
    pub const HYBRID_TOPOLOGY: Self = Self(HEADER_HYBRID_TOPOLOGY);
    pub const HYBRID_CPU_PMU_CAPS: Self = Self(HEADER_HYBRID_CPU_PMU_CAPS);
    pub const SIMPLEPERF_META_INFO: Self = Self(HEADER_SIMPLEPERF_META_INFO);
    pub const SIMPLEPERF_DEBUG_UNWIND: Self = Self(HEADER_SIMPLEPERF_DEBUG_UNWIND);
    pub const SIMPLEPERF_DEBUG_UNWIND_FILE: Self = Self(HEADER_SIMPLEPERF_DEBUG_UNWIND_FILE);
    pub const SIMPLEPERF_FILE2: Self = Self(HEADER_SIMPLEPERF_FILE2);
}

impl fmt::Debug for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::TRACING_DATA => "TRACING_DATA".fmt(f),
            Self::BUILD_ID => "BUILD_ID".fmt(f),
            Self::HOSTNAME => "HOSTNAME".fmt(f),
            Self::OSRELEASE => "OSRELEASE".fmt(f),
            Self::VERSION => "VERSION".fmt(f),
            Self::ARCH => "ARCH".fmt(f),
            Self::NRCPUS => "NRCPUS".fmt(f),
            Self::CPUDESC => "CPUDESC".fmt(f),
            Self::CPUID => "CPUID".fmt(f),
            Self::TOTAL_MEM => "TOTAL_MEM".fmt(f),
            Self::CMDLINE => "CMDLINE".fmt(f),
            Self::EVENT_DESC => "EVENT_DESC".fmt(f),
            Self::CPU_TOPOLOGY => "CPU_TOPOLOGY".fmt(f),
            Self::NUMA_TOPOLOGY => "NUMA_TOPOLOGY".fmt(f),
            Self::BRANCH_STACK => "BRANCH_STACK".fmt(f),
            Self::PMU_MAPPINGS => "PMU_MAPPINGS".fmt(f),
            Self::GROUP_DESC => "GROUP_DESC".fmt(f),
            Self::AUXTRACE => "AUXTRACE".fmt(f),
            Self::STAT => "STAT".fmt(f),
            Self::CACHE => "CACHE".fmt(f),
            Self::SAMPLE_TIME => "SAMPLE_TIME".fmt(f),
            Self::SAMPLE_TOPOLOGY => "SAMPLE_TOPOLOGY".fmt(f),
            Self::CLOCKID => "CLOCKID".fmt(f),
            Self::DIR_FORMAT => "DIR_FORMAT".fmt(f),
            Self::BPF_PROG_INFO => "BPF_PROG_INFO".fmt(f),
            Self::BPF_BTF => "BPF_BTF".fmt(f),
            Self::COMPRESSED => "COMPRESSED".fmt(f),
            Self::CPU_PMU_CAPS => "CPU_PMU_CAPS".fmt(f),
            Self::CLOCK_DATA => "CLOCK_DATA".fmt(f),
            Self::HYBRID_TOPOLOGY => "HYBRID_TOPOLOGY".fmt(f),
            Self::HYBRID_CPU_PMU_CAPS => "HYBRID_CPU_PMU_CAPS".fmt(f),
            Self::SIMPLEPERF_META_INFO => "SIMPLEPERF_META_INFO".fmt(f),
            Self::SIMPLEPERF_DEBUG_UNWIND => "SIMPLEPERF_DEBUG_UNWIND".fmt(f),
            Self::SIMPLEPERF_DEBUG_UNWIND_FILE => "SIMPLEPERF_DEBUG_UNWIND_FILE".fmt(f),
            Self::SIMPLEPERF_FILE2 => "SIMPLEPERF_FILE2".fmt(f),
            _ => f.write_fmt(format_args!("Unknown Feature {}", &self.0)),
        }
    }
}

/// The set of features used in the perf file. The perf file contains one
/// feature section for each feature.
///
/// This set is provided in the perf file header.
/// It has room for 4 * 64 = 256 feature bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureSet(pub [u64; 4]);

impl FeatureSet {
    pub const MAX_BITS: u32 = 64 * 4;

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

    /// Returns an iterator over all features in this set, from low to high.
    pub fn iter(&self) -> FeatureSetIter {
        FeatureSetIter {
            current_feature: Feature(0),
            set: *self,
        }
    }

    /// Checks if the feature is contained in this set.
    #[inline]
    pub fn has_feature(&self, feature: Feature) -> bool {
        if feature.0 >= 256 {
            return false;
        }
        let features_chunk_index = (feature.0 / 64) as usize;
        let feature_bit = feature.0 % 64;
        let features_chunk = self.0[features_chunk_index];
        (features_chunk & (1 << feature_bit)) != 0
    }
}

impl fmt::Debug for FeatureSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut set = f.debug_set();
        for feature in self.iter() {
            set.entry(&feature);
        }
        set.finish()
    }
}

/// An iterator over all the features that are included in a [`FeatureSet`],
/// ordered from low to high feature bit.
///
/// The iteration order is the order in which the feature sections are stored
/// in a perf.data file.
pub struct FeatureSetIter {
    current_feature: Feature,
    set: FeatureSet,
}

impl Iterator for FeatureSetIter {
    type Item = Feature;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_feature.0 < FeatureSet::MAX_BITS {
            let feature = self.current_feature;
            self.current_feature.0 += 1;

            if self.set.has_feature(feature) {
                return Some(feature);
            }
        }
        None
    }
}
