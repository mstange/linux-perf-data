use byteorder::{BigEndian, ByteOrder, LittleEndian};
use linux_perf_event_reader::{Endianness, RawData};

/// A parsed `JIT_CODE_LOAD` record, for a single jitted function.
///
/// This carries the function name and the code bytes.
#[derive(Debug, Clone)]
pub struct JitCodeLoadRecord<'a> {
    /// The process ID of the runtime generating the jitted code.
    pub pid: u32,
    /// The thread ID of the runtime thread generating the jitted code.
    ///
    /// This is a `u64` because some runtimes (e.g. CPython on macOS, which uses
    /// `pthread_threadid_np`) write a 64-bit thread id, in a wider record layout
    /// that is detected during parsing (see `parse_impl`).
    pub tid: u64,
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
    pub fn parse(endian: Endianness, data: RawData<'a>) -> Result<Self, std::io::Error> {
        match endian {
            Endianness::LittleEndian => Self::parse_impl::<LittleEndian>(data),
            Endianness::BigEndian => Self::parse_impl::<BigEndian>(data),
        }
    }

    pub fn parse_impl<O: ByteOrder>(data: RawData<'a>) -> Result<Self, std::io::Error> {
        // Try the standard layout first (the common case); fall back to the wider
        // macOS layout if it isn't self-consistent with the record body length.
        // The two layouts differ only in where the name starts, so exactly one of
        // them makes `prefix + name_len + 1 + code_size == body_len` hold.
        //
        // A read error on the standard (narrower) layout means the record is too
        // short even for the minimal layout, i.e. genuinely truncated — propagate
        // it rather than masking it as `InvalidData`. The wider layout only reads
        // *more* bytes, so a read error there just means "not this layout".
        if let Some(record) = Self::try_parse::<O>(data, false)? {
            return Ok(record);
        }
        if let Ok(Some(record)) = Self::try_parse::<O>(data, true) {
            return Ok(record);
        }
        Err(std::io::ErrorKind::InvalidData.into())
    }

    /// Parse one layout. `Ok(Some)` = parsed and self-consistent, `Ok(None)` =
    /// fields read but the layout doesn't fit the body (try the other one),
    /// `Err` = a genuine read error such as `UnexpectedEof`.
    fn try_parse<O: ByteOrder>(
        data: RawData<'a>,
        macos_wide_layout: bool,
    ) -> Result<Option<Self>, std::io::Error> {
        let body_len = data.len();
        let mut cur = data;
        let pid = cur.read_u32::<O>()?;
        let tid = if macos_wide_layout {
            let _pad = cur.read_u32::<O>()?;
            cur.read_u64::<O>()?
        } else {
            u64::from(cur.read_u32::<O>()?)
        };
        let vma = cur.read_u64::<O>()?;
        let code_addr = cur.read_u64::<O>()?;
        let code_size = cur.read_u64::<O>()?;
        let code_index = cur.read_u64::<O>()?;
        let Some(function_name) = cur.read_string() else {
            return Ok(None);
        };

        // Validate this layout against the known body length before trusting it:
        // `cur` now points just past the name's NUL, so the bytes consumed so far
        // plus the code must exactly fill the body. A mismatch means we picked the
        // wrong layout.
        let consumed = body_len - cur.len();
        if consumed.checked_add(code_size as usize) != Some(body_len) {
            return Ok(None);
        }
        let code_bytes = cur.split_off_prefix(code_size as usize)?;
        Ok(Some(Self {
            pid,
            tid,
            vma,
            code_addr,
            code_index,
            function_name,
            code_bytes,
        }))
    }
}

/// A parsed `JIT_CODE_MOVE` record.
#[derive(Debug, Clone)]
pub struct JitCodeMoveRecord {
    /// The process ID of the runtime generating the jitted code.
    pub pid: u32,
    /// The thread ID of the runtime thread generating the jitted code.
    ///
    /// This is a `u64` for the same reason as [`JitCodeLoadRecord::tid`].
    pub tid: u64,
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
        // This record is fixed-size with no trailing data, so the correct layout
        // is the one whose fields exactly consume the body (48 bytes for the
        // standard u32 tid, 56 for the wider macOS u64 tid + padding).
        //
        // As in `JitCodeLoadRecord`, a read error on the narrower standard layout
        // is genuine truncation and is propagated; the wider layout's read errors
        // just mean "not this layout".
        if let Some(record) = Self::try_parse::<O>(data, false)? {
            return Ok(record);
        }
        if let Ok(Some(record)) = Self::try_parse::<O>(data, true) {
            return Ok(record);
        }
        Err(std::io::ErrorKind::InvalidData.into())
    }

    /// Parse one layout. `Ok(Some)` = parsed and the body is fully consumed,
    /// `Ok(None)` = wrong layout (trailing bytes left over), `Err` = a genuine
    /// read error such as `UnexpectedEof`.
    fn try_parse<O: ByteOrder>(
        data: RawData,
        macos_wide_layout: bool,
    ) -> Result<Option<Self>, std::io::Error> {
        let mut cur = data;
        let pid = cur.read_u32::<O>()?;
        let tid = if macos_wide_layout {
            let _pad = cur.read_u32::<O>()?;
            cur.read_u64::<O>()?
        } else {
            u64::from(cur.read_u32::<O>()?)
        };
        let vma = cur.read_u64::<O>()?;
        let old_code_addr = cur.read_u64::<O>()?;
        let new_code_addr = cur.read_u64::<O>()?;
        let code_size = cur.read_u64::<O>()?;
        let code_index = cur.read_u64::<O>()?;

        // The record must be fully consumed; otherwise we picked the wrong layout.
        if !cur.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            pid,
            tid,
            vma,
            old_code_addr,
            new_code_addr,
            code_size,
            code_index,
        }))
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

#[cfg(test)]
mod tests {
    use crate::jitdump::{JitDumpReader, JitDumpRecord};
    use std::fs::File;

    fn parse_jitdump(path: &str) -> (usize, usize) {
        let mut reader = JitDumpReader::new(File::open(path).unwrap()).unwrap();
        let (mut loads, mut unwinds) = (0, 0);
        while let Some(raw) = reader.next_record().unwrap() {
            match raw.parse().unwrap() {
                JitDumpRecord::CodeLoad(_) => loads += 1,
                JitDumpRecord::CodeUnwindingInfo(_) => unwinds += 1,
                _ => {}
            }
        }
        (loads, unwinds)
    }

    /// A real cpython-on-Linux x86_64 jitdump capture in the **standard** perf
    /// layout (u32 tid, no alignment padding).
    #[test]
    fn parses_standard_layout_python_jitdump() {
        let (loads, unwinds) = parse_jitdump(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/jit-python-linux-x86_64.dump"
        ));
        assert_eq!(loads, 195);
        assert_eq!(unwinds, 195);
    }

    /// A real jitdump from cpython-3.15.0a7 on **macOS arm64**, where CPython
    /// declares `thread_id` as `uint64_t`, so every `CODE_LOAD` uses the wider
    /// layout (u64 tid + 4 bytes of alignment padding), shifting the name and
    /// code bytes by 8 vs the perf jitdump spec. The previous u32-only parser
    /// misread `code_size` as the (4 GB) code address and failed on this file.
    #[test]
    fn parses_macos_wide_layout_python_jitdump() {
        let (loads, unwinds) = parse_jitdump(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/jit-python-macos-arm64.dump"
        ));
        assert_eq!(loads, 195);
        assert_eq!(unwinds, 195);
    }

    /// A record body too short for even the standard layout must surface the
    /// underlying `UnexpectedEof` rather than being masked as `InvalidData` by
    /// the layout-detection fallback.
    #[test]
    fn truncated_records_report_eof() {
        use super::{JitCodeLoadRecord, JitCodeMoveRecord};
        use linux_perf_event_reader::{Endianness, RawData};

        let short = [0u8; 10]; // not enough for pid + tid + vma
        for kind in [
            JitCodeLoadRecord::parse(Endianness::LittleEndian, RawData::Single(&short))
                .unwrap_err()
                .kind(),
            JitCodeMoveRecord::parse(Endianness::LittleEndian, RawData::Single(&short))
                .unwrap_err()
                .kind(),
        ] {
            assert_eq!(kind, std::io::ErrorKind::UnexpectedEof);
        }
    }
}
