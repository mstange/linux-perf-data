use std::io::ErrorKind;

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linux_perf_event_reader::RawData;

use super::error::JitDumpError;

/// The jitdump header.
#[derive(Debug, Clone)]
pub struct JitDumpHeader {
    /// Four bytes tagging the file type and declaring the endianness of this file.
    /// When interpreted as a u32 in the correct endian, this is 0x4A695444.
    /// Represents the string "JiTD" in ASCII form.
    pub magic: [u8; 4],
    /// The format version. It is currently set to 1.
    pub version: u32,
    /// The size in bytes of file header.
    pub total_size: u32,
    /// ELF architecture encoding (ELF e_machine value as specified in /usr/include/elf.h)
    pub elf_machine_arch: u32,
    /// The process ID of the JIT runtime process.
    pub pid: u32,
    /// The timestamp of when the file was created.
    pub timestamp: u64,
    /// A bitmask of flags.
    pub flags: u64,
}

impl JitDumpHeader {
    pub const SIZE: usize = 40; // 40 bytes

    pub fn parse(mut data: RawData) -> Result<Self, JitDumpError> {
        let mut magic = [0; 4];
        data.read_exact(&mut magic)?;
        let header_result = match &magic {
            b"JiTD" => Self::parse_after_magic::<BigEndian>(magic, data),
            b"DTiJ" => Self::parse_after_magic::<LittleEndian>(magic, data),
            _ => return Err(JitDumpError::InvalidMagicBytes(magic)),
        };
        let header = match header_result {
            Ok(header) => header,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                return Err(JitDumpError::NotEnoughBytesForHeader)
            }
            Err(e) => panic!("Unexpected error type {e}"),
        };
        if header.total_size < Self::SIZE as u32 {
            return Err(JitDumpError::InvalidHeaderSize(header.total_size));
        }

        Ok(header)
    }

    pub fn parse_after_magic<O: ByteOrder>(
        magic: [u8; 4],
        data: RawData,
    ) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let version = cur.read_u32::<O>()?;
        let total_size = cur.read_u32::<O>()?;

        // Make sure we have total_size bytes available. `data` is right after the 4 magic bytes.
        let mut full_header = data;
        full_header.skip(total_size.saturating_sub(4) as usize)?;

        let elf_machine_arch = cur.read_u32::<O>()?;
        let _pad1 = cur.read_u32::<O>()?;
        let pid = cur.read_u32::<O>()?;
        let timestamp = cur.read_u64::<O>()?;
        let flags = cur.read_u64::<O>()?;
        Ok(Self {
            magic,
            version,
            total_size,
            elf_machine_arch,
            pid,
            timestamp,
            flags,
        })
    }
}
