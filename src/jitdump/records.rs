use core::str;

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linux_perf_event_reader::{Endianness, RawData};

use super::record::JitDumpRecordHeader;

/// A parsed `JIT_CODE_LOAD` record, for a single jitted function.
///
/// This carries the function name and the code bytes.
#[derive(Debug, Clone)]
pub struct JitCodeLoadRecord<'a> {
    /// The process ID of the runtime generating the jitted code.
    pub pid: u32,
    /// The thread ID of the runtime thread generating the jitted code.
    pub tid: u32,
    /// The virtual address where `code_bytes` starts in the memory of the process.
    pub vma: u64,
    /// The code start address for the jitted code. It is unclear in what cases this would differ from `vma`.
    pub code_addr: u64,
    /// A unique identifier for this piece of jitted code, to allow future `JitCodeMoveRecord`s to refer back to this record.
    pub code_index: u64,
    /// The function name, in ASCII.
    pub function_name: RawData<'a>,
    /// The jitted code, as raw bytes. These bytes can be decoded into assembly
    /// instructions of the CPU architecture given in the file header.
    pub code_bytes: RawData<'a>,
}

impl<'a> JitCodeLoadRecord<'a> {
    /// The offset, in bytes, between the start of the record header and
    /// the start of the function name.
    pub const NAME_OFFSET_FROM_RECORD_START: usize =
        JitDumpRecordHeader::SIZE + 4 + 4 + 8 + 8 + 8 + 8;

    pub fn parse(endian: Endianness, data: RawData<'a>) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }

    pub fn parse_impl<O: ByteOrder>(data: RawData<'a>) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let pid = cur.read_u32::<O>()?;
        let tid = cur.read_u32::<O>()?;
        let vma = cur.read_u64::<O>()?;
        let code_addr = cur.read_u64::<O>()?;
        let code_size = cur.read_u64::<O>()?;
        let code_index = cur.read_u64::<O>()?;
        let function_name = cur.read_string().ok_or(std::io::ErrorKind::UnexpectedEof)?;
        let code_bytes = cur.split_off_prefix(code_size as usize)?;
        Ok(Self {
            pid,
            tid,
            vma,
            code_addr,
            code_index,
            function_name,
            code_bytes,
        })
    }

    /// The offset, in bytes, between the start of the record header and
    /// the start of the code bytes.
    ///
    /// This can be different for each record because the code bytes are after
    /// the function name, so this offset depends on the length of the function
    /// name.
    pub fn code_bytes_offset_from_record_header_start(&self) -> usize {
        JitDumpRecordHeader::SIZE + 4 + 4 + 8 + 8 + 8 + 8 + self.function_name.len() + 1
    }
}

/*
+struct PerfJitInlineEntry {
+  uint64_t start_addr_;
+  uint64_t end_addr_;
+  uint32_t call_file_;
+  uint32_t call_line_;
+  uint32_t call_column_;
+  uint32_t inline_depth_;
+  // Followed by null-terminated func_name and call_file_name strings.
+};
+
+struct PerfJitCodeInline : PerfJitBase {
+  uint64_t code_addr_;
+  uint64_t nr_entry_;
+  // Followed by nr_entry_ instances of PerfJitInlineEntry.
+};

*/
#[derive(Debug, Clone)]
pub struct JitCodeInlineRecord<'a> {
    /// The address of the code bytes of the function for which the inline information is generated.
    pub code_addr: u64,
    /// The list of inline entries.
    pub entries: Vec<JitCodeInlineEntry<'a>>,
}

#[derive(Debug, Clone)]
pub struct JitCodeInlineEntry<'a> {
    /// The start address of the inlined function code bytes.
    pub start_addr: u64,
    /// The end address of the inlined function code bytes.
    pub end_addr: u64,
    /// The name of the inlined function, in ASCII.
    pub func_name: RawData<'a>,
    /// The file path where the inlined function is defined, in ASCII.
    pub call_file_name: RawData<'a>,
    /// The line number in the source file (1-based) where the inlined function is called.
    pub call_line: u32,
    /// The column number. Zero means "no column information", 1 means "beginning of the line".
    pub call_column: u32,
    /// The depth of inlining (0 means directly called, 1 means inlined into a directly called function, etc).
    pub inline_depth: u32,
}

impl<'a> JitCodeInlineRecord<'a> {
    pub fn parse(endian: Endianness, data: RawData<'a>) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }
    pub fn parse_impl<O: ByteOrder>(data: RawData<'a>) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let code_addr = cur.read_u64::<O>()?;
        let nr_entry = cur.read_u64::<O>()?;
        let mut entries = Vec::with_capacity(nr_entry as usize);
        for _ in 0..nr_entry {
            let start_addr = cur.read_u64::<O>()?;
            let end_addr = cur.read_u64::<O>()?;
            let call_file_offset = cur.read_u32::<O>()?;
            let call_line = cur.read_u32::<O>()?;
            let call_column = cur.read_u32::<O>()?;
            let inline_depth = cur.read_u32::<O>()?;
            let func_name = cur.read_string().ok_or(std::io::ErrorKind::UnexpectedEof)?;
            let call_file_name = cur.read_string().ok_or(std::io::ErrorKind::UnexpectedEof)?;
            entries.push(JitCodeInlineEntry {
                start_addr,
                end_addr,
                func_name,
                call_file_name,
                call_line,
                call_column,
                inline_depth,
            });
        }

        Ok(Self { code_addr, entries })
    }
}

/// A parsed `JIT_CODE_MOVE` record.
#[derive(Debug, Clone)]
pub struct JitCodeMoveRecord {
    /// The process ID of the runtime generating the jitted code.
    pub pid: u32,
    /// The thread ID of the runtime thread generating the jitted code.
    pub tid: u32,
    /// The new address where the jitted code starts in the virtual memory of the process.
    pub vma: u64,
    /// The old address of this function's code bytes.
    pub old_code_addr: u64,
    /// The new address of this function's code bytes. It is unclear in what cases this might be different from `vma`.
    pub new_code_addr: u64,
    /// The size in bytes of the jitted code.
    pub code_size: u64,
    /// The index referring to the `JIT_CODE_LOAD` record for this function with the same `code_index`.
    pub code_index: u64,
}

impl JitCodeMoveRecord {
    pub fn parse(endian: Endianness, data: RawData) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }

    pub fn parse_impl<O: ByteOrder>(data: RawData) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let pid = cur.read_u32::<O>()?;
        let tid = cur.read_u32::<O>()?;
        let vma = cur.read_u64::<O>()?;
        let old_code_addr = cur.read_u64::<O>()?;
        let new_code_addr = cur.read_u64::<O>()?;
        let code_size = cur.read_u64::<O>()?;
        let code_index = cur.read_u64::<O>()?;
        Ok(Self {
            pid,
            tid,
            vma,
            old_code_addr,
            new_code_addr,
            code_size,
            code_index,
        })
    }
}

/// A parsed `JIT_CODE_DEBUG_INFO` record, mapping addresses to source lines.
#[derive(Debug, Clone)]
pub struct JitCodeDebugInfoRecord<'a> {
    /// The address of the code bytes of the function for which the debug information is generated.
    pub code_addr: u64,
    /// The list of line entries, sorted by address.
    pub entries: Vec<JitCodeDebugInfoEntry<'a>>,
}

/// An entry for a single code location (file, line, column). Used inside a [`JitCodeDebugInfoRecord`].
///
/// Each entry describes a contiguous range of code bytes: this entry's address to the next
/// entry's address, or to the end of the function if this is the last entry.
/// address
#[derive(Debug, Clone)]
pub struct JitCodeDebugInfoEntry<'a> {
    /// The start address of the range of code bytes which this entry describes.
    ///
    /// The range goes to the next entry, or to the end of the function if this is the last entry.
    pub code_addr: u64,
    /// The line number in the source file (1-based) for this entry.
    pub line: u32,
    /// The column number. Zero means "no column information", 1 means "beginning of the line".
    pub column: u32,
    /// The path of the source code file, in ASCII.
    pub file_path: RawData<'a>,
}

impl<'a> JitCodeDebugInfoRecord<'a> {
    pub fn parse(endian: Endianness, data: RawData<'a>) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }

    pub fn parse_impl<O: ByteOrder>(data: RawData<'a>) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let code_addr = cur.read_u64::<O>()?;
        let nr_entry = cur.read_u64::<O>()?;
        let mut entries = Vec::with_capacity(nr_entry as usize);
        for _ in 0..nr_entry {
            let code_addr = cur.read_u64::<O>()?;
            let line = cur.read_u32::<O>()?;
            let column = cur.read_u32::<O>()?;
            let file_path = cur.read_string().ok_or(std::io::ErrorKind::UnexpectedEof)?;
            entries.push(JitCodeDebugInfoEntry {
                code_addr,
                line,
                column,
                file_path,
            });
        }

        Ok(Self { code_addr, entries })
    }

    pub fn lookup(&self, addr: u64) -> Option<&JitCodeDebugInfoEntry<'a>> {
        let index = match self
            .entries
            .binary_search_by_key(&addr, |entry| entry.code_addr)
        {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        Some(&self.entries[index])
    }
}

/// A parsed `JIT_CODE_UNWINDING_INFO` record, with `eh_frame` data for a single jitted function.
#[derive(Debug, Clone)]
pub struct JitCodeUnwindingInfoRecord<'a> {
    /// The size of the unwinding data mapped in memory. This is either zero or equal to `eh_frame_header.len() + eh_frame.len()`.
    pub mapped_size: u64,
    /// The eh_frame_hdr data. This provides an index for the eh_frame data.
    pub eh_frame_hdr: RawData<'a>,
    /// The eh_frame data.
    pub eh_frame: RawData<'a>,
}

impl<'a> JitCodeUnwindingInfoRecord<'a> {
    pub fn parse(endian: Endianness, data: RawData<'a>) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }

    pub fn parse_impl<O: ByteOrder>(data: RawData<'a>) -> Result<Self, std::io::Error> {
        let mut cur = data;
        let unwind_data_size = cur.read_u64::<O>()?;
        let eh_frame_hdr_size = cur.read_u64::<O>()? as usize;
        let mapped_size = cur.read_u64::<O>()?;
        let mut unwind_data = cur.split_off_prefix(unwind_data_size as usize)?;
        let eh_frame_hdr = unwind_data.split_off_prefix(eh_frame_hdr_size)?;
        let eh_frame = unwind_data;
        Ok(Self {
            mapped_size,
            eh_frame_hdr,
            eh_frame,
        })
    }
}
