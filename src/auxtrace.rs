use byteorder::ByteOrder;
use linux_perf_event_reader::RawData;

/// Auxtrace data record.
///
/// This is usually used with Intel PT, which records the pure Intel PT data.
pub struct Auxtrace<'a> {
    /// Size of `aux_data`
    pub size: u64,
    /// Offset of this trace in the total trace
    pub offset: u64,
    /// Reference ID
    pub reference: u64,
    /// Index of this trace in the total trace
    pub index: u32,
    /// ID of thread that contributes to this trace
    pub tid: u32,
    /// ID of CPU that contributes to this trace
    pub cpu: u32,
    /// Pure data. The length of this data is guaranteed to be consistent
    /// with the `size` field.
    pub aux_data: RawData<'a>,
}

impl<'a> Auxtrace<'a> {
    pub fn parse<T: ByteOrder>(mut data: RawData<'a>) -> Result<Self, std::io::Error> {
        let size = data.read_u64::<T>()?;
        let offset = data.read_u64::<T>()?;
        let reference = data.read_u64::<T>()?;
        let index = data.read_u32::<T>()?;
        let tid = data.read_u32::<T>()?;
        let cpu = data.read_u32::<T>()?;
        let _reserved = data.read_u32::<T>()?;
        let aux_data = data;
        Ok(Self {
            size,
            offset,
            reference,
            index,
            tid,
            cpu,
            aux_data,
        })
    }
}

#[cfg(test)]
mod test {
    use byteorder::LittleEndian;
    use linux_perf_event_reader::RawData;

    use super::Auxtrace;

    #[test]
    fn parse() {
        let data = RawData::Single(&[
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xf9, 0xcd, 0x2a, 0x49, 0x28, 0x04, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00,
            0x7a, 0x24, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        let auxtrace = Auxtrace::parse::<LittleEndian>(data).unwrap();
        assert_eq!(auxtrace.size, 0x01);
        assert_eq!(auxtrace.offset, 0x00);
        assert_eq!(auxtrace.reference, 0x428492acdf9);
        assert_eq!(auxtrace.index, 0x8);
        assert_eq!(auxtrace.tid, 0x247a);
        assert_eq!(auxtrace.cpu, 8);
        assert_eq!(auxtrace.aux_data.len(), auxtrace.size as usize);
    }
}
