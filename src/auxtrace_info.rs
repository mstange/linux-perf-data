use byteorder::ByteOrder;
use linux_perf_event_reader::RawData;

/// Auxtrace Info record is defined as:
/// ```c
/// struct perf_record_auxtrace_info {
///   u32 type;
///   u32 reserved__;
///   u64 priv[];
/// };
/// ```
/// Elements up to `IntelPtPerCpuMmaps` must be present. Other elements are only present if the size
/// of the overall record is large enough.
#[allow(dead_code)]
#[derive(Clone, Copy)]
enum PrivField {
    IntelPtPmuType = 0,
    IntelPtTimeShift = 1,
    IntelPtTimeMult = 2,
    IntelPtTimeZero = 3,
    IntelPtCapUserTimeZero = 4,
    IntelPtTscBit = 5,
    IntelPtNoretcompBit = 6,
    IntelPtHaveSchedSwitch = 7,
    IntelPtSnapshotMode = 8,
    IntelPtPerCpuMmaps = 9,
    IntelPtMtcBit = 10,
    IntelPtMtcFreqBits = 11,
    IntelPtTscCtcN = 12,
    IntelPtTscCtcD = 13,
    IntelPtCycBit = 14,
    IntelPtMaxNonturboRatio = 15,
    IntelPtFilterStrLen = 16,
}

fn has_field(data: &RawData, field: PrivField) -> bool {
    data.len() >= 4 + 4 + (field as usize + 1) * 8
}

/// Auxtrace info record.
///
/// This is usually used with Intel PT and records metadata about how the Intel PT
/// trace was captured. Some fields will only exist if the corresponding feature was
/// enabled when the trace was captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxtraceInfo<'a> {
    /// The Auxtrace type
    pub type_: u32,
    /// The PMU type
    pub pmu_type: u32,
    /// The time shift used for converting TSC to nanoseconds
    pub time_shift: u16,
    /// The time multiplier used for converting TSC to nanoseconds
    pub time_mult: u32,
    /// The time offset used for converting TSC to nanoseconds
    pub time_zero: u64,
    /// Whether converting TSC to nanoseconds is available to userspace tools
    pub cap_time_zero: bool,
    /// Identifies which configuration bit is used to enable TSC
    pub tsc_bit: u64,
    /// Identifies which configuration bit is used to enable non-retcomp
    pub noretcomp_bit: u64,
    /// Whether PERF_RECORD_SWITCH events are recorded
    pub have_sched_switch: i64,
    /// Whether the trace was in snapshot mode
    pub snapshot_mode: u64,
    /// Whether the trace was using per-CPU memory maps
    pub percpu_maps: bool,
    /// Identifies which configuration bit is used to enable MTC
    pub mtc_bit: Option<u64>,
    /// Bit mask that identifies which bits are used to configure MTC period
    pub mtc_freq: Option<u64>,
    /// The value of cpuid[0x15].ebx
    pub tsc_ctc_numerator: Option<u32>,
    /// The value of cpuid[0x15].eax
    pub tsc_ctc_denominator: Option<u32>,
    /// Identifies which configuration bit is used to enable CYC
    pub cyc_bit: Option<u64>,
    /// CPU's maximum non-turbo ratio, in units of 100 MHz
    pub max_nonturbo_ratio: Option<u64>,
    /// The address filter that was provided on the command line
    pub filter_str: Option<RawData<'a>>,
}

impl<'a> AuxtraceInfo<'a> {
    pub fn parse<T: ByteOrder>(mut data: RawData<'a>) -> Result<Self, std::io::Error> {
        let has_cyc_bit = has_field(&data, PrivField::IntelPtCycBit);
        let has_max_nonturbo_ratio = has_field(&data, PrivField::IntelPtMaxNonturboRatio);
        let has_filter_str = has_field(&data, PrivField::IntelPtFilterStrLen);

        let type_ = data.read_u32::<T>()?;
        let _reserved = data.read_u32::<T>()?;
        let pmu_type = data.read_u64::<T>()? as u32;
        let time_shift = data.read_u64::<T>()? as u16;
        let time_mult = data.read_u64::<T>()? as u32;
        let time_zero = data.read_u64::<T>()?;
        let cap_time_zero = data.read_u64::<T>()? != 0;
        let tsc_bit = data.read_u64::<T>()?;
        let noretcomp_bit = data.read_u64::<T>()?;
        let have_sched_switch = data.read_u64::<T>()? as i64;
        let snapshot_mode = data.read_u64::<T>()?;
        let percpu_maps = data.read_u64::<T>()? != 0;

        let mut mtc_bit = None;
        let mut mtc_freq = None;
        let mut tsc_ctc_numerator = None;
        let mut tsc_ctc_denominator = None;
        let mut cyc_bit = None;

        if has_cyc_bit {
            mtc_bit = Some(data.read_u64::<T>()?);
            mtc_freq = Some(data.read_u64::<T>()?);
            tsc_ctc_numerator = Some(data.read_u64::<T>()? as u32);
            tsc_ctc_denominator = Some(data.read_u64::<T>()? as u32);
            cyc_bit = Some(data.read_u64::<T>()?);
        }

        let mut max_nonturbo_ratio = None;

        if has_max_nonturbo_ratio {
            max_nonturbo_ratio = Some(data.read_u64::<T>()?);
        }

        let mut filter_str = None;

        if has_filter_str {
            let _filter_str_len = data.read_u64::<T>()? as usize;
            filter_str = Some(data);
        }

        Ok(Self {
            type_,
            pmu_type,
            time_shift,
            time_mult,
            time_zero,
            cap_time_zero,
            tsc_bit,
            noretcomp_bit,
            have_sched_switch,
            snapshot_mode,
            percpu_maps,
            mtc_bit,
            mtc_freq,
            tsc_ctc_numerator,
            tsc_ctc_denominator,
            cyc_bit,
            max_nonturbo_ratio,
            filter_str,
        })
    }
}

#[cfg(test)]
mod test {

    use byteorder::LittleEndian;
    use linux_perf_event_reader::RawData;

    use super::AuxtraceInfo;

    #[test]
    fn parse() {
        let data = RawData::Single(&[
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x1f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe7, 0x79, 0x9e, 0x2f,
            0x00, 0x00, 0x00, 0x00, 0xb3, 0xf3, 0x73, 0x44, 0xfd, 0xff, 0xff, 0xff, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0,
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x8c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x66, 0x69, 0x6c, 0x74, 0x65, 0x72, 0x20, 0x30, 0x78, 0x34,
            0x30, 0x30, 0x30, 0x30, 0x30, 0x2f, 0x30, 0x78, 0x31, 0x30, 0x30, 0x30, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        let info = AuxtraceInfo::parse::<LittleEndian>(data).unwrap();
        assert_eq!(info.type_, 1);
        assert_eq!(info.pmu_type, 12);
        assert_eq!(info.time_shift, 31);
        assert_eq!(info.time_mult, 798915047);
        assert_eq!(info.time_zero, 18446744061973099443);
        assert_eq!(info.cap_time_zero, true);
        assert_eq!(info.tsc_bit, 0x400);
        assert_eq!(info.noretcomp_bit, 0x800);
        assert_eq!(info.have_sched_switch, 2);
        assert_eq!(info.snapshot_mode, 0);
        assert_eq!(info.percpu_maps, true);
        assert_eq!(info.mtc_bit, Some(0x200));
        assert_eq!(info.mtc_freq, Some(0x3c000));
        assert_eq!(info.tsc_ctc_numerator, Some(140));
        assert_eq!(info.tsc_ctc_denominator, Some(2));
        assert_eq!(info.cyc_bit, Some(0x2));
        assert_eq!(info.max_nonturbo_ratio, Some(27));
        assert!(info.filter_str.is_some());
        let filter_str = info.filter_str.unwrap().read_string().unwrap().as_slice();
        let str = str::from_utf8(&filter_str).unwrap();
        assert_eq!(str, "filter 0x400000/0x1000");
    }
}
