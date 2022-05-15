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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagFeature {
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
}

impl FlagFeature {
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
            _ => return None,
        };
        Some(feature)
    }
}

/// The set of feature flags used in the perf file. The perf file contains has one
/// "feature flag section" for each of the flags. This set is provided in the perf
/// file header.
///
/// The set has room for 4 * 64 = 256 header flag bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlagFeatureSet(pub [u64; 4]);

impl FlagFeatureSet {
    /// Emits all feature flags which are set, from low to high.
    pub fn iter(&self) -> FlagFeatureSetIterAll {
        FlagFeatureSetIterAll {
            bit: 0,
            flags: self.0,
        }
    }
}

impl fmt::Debug for FlagFeatureSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut set = f.debug_set();
        for i in self.iter() {
            if let Some(feature) = FlagFeature::from_int(i) {
                set.entry(&feature);
            } else {
                set.entry(&format_args!("Unknown({})", i));
            }
        }
        set.finish()
    }
}

pub struct FlagFeatureSetIterAll {
    bit: u32,
    flags: [u64; 4],
}

impl Iterator for FlagFeatureSetIterAll {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.bit < 256 {
            let bit = self.bit;
            self.bit += 1;

            let flags_chunk_index = (bit / 64) as usize;
            let bit_index = bit % 64;
            let flags_chunk = self.flags[flags_chunk_index];
            let flag_is_set = (flags_chunk & (1 << bit_index)) != 0;
            if flag_is_set {
                return Some(bit);
            }
        }
        None
    }
}
