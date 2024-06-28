use std::io::Read;

use byteorder::{ByteOrder, ReadBytesExt};
use linux_perf_event_reader::{constants::PERF_RECORD_MISC_BUILD_ID_SIZE, PerfEventHeader};

/// Old versions of perf did not write down the length of the build ID.
/// Detect the true length by removing 4-byte chunks of zeros from the end.
fn detect_build_id_len(build_id_bytes: &[u8]) -> u8 {
    let mut len = build_id_bytes.len();
    const CHUNK_SIZE: usize = 4;
    for chunk in build_id_bytes.chunks(CHUNK_SIZE).rev() {
        if chunk.iter().any(|b| *b != 0) {
            break;
        }
        len -= chunk.len();
    }
    len as u8
}

/// `build_id_event`
///
/// If PERF_RECORD_MISC_KERNEL is set in header.misc, then this
/// is the build id for the vmlinux image or a kmod.
#[derive(Debug, Clone)]
pub struct BuildIdEvent {
    pub header: PerfEventHeader,
    pub _pid: i32,
    pub build_id: Vec<u8>,
    pub file_path: Vec<u8>,
}

impl BuildIdEvent {
    pub fn parse<R: Read, T: ByteOrder>(mut reader: R) -> Result<Self, std::io::Error> {
        let header = PerfEventHeader::parse::<_, T>(&mut reader)?;
        let _pid = reader.read_i32::<T>()?;
        let mut build_id_bytes = [0; 24];
        reader.read_exact(&mut build_id_bytes)?;

        // Followed by file path for the remaining bytes. The total size of the record
        // is given by header.size.
        const BYTES_BEFORE_PATH: usize = PerfEventHeader::STRUCT_SIZE + 4 + 24;
        let path_len = usize::from(header.size).saturating_sub(BYTES_BEFORE_PATH);
        let mut path_bytes = vec![0; path_len];
        reader.read_exact(&mut path_bytes)?;

        let path_len = memchr::memchr(0, &path_bytes).unwrap_or(path_len);
        path_bytes.truncate(path_len);
        let file_path = path_bytes;

        // If PERF_RECORD_MISC_BUILD_ID_SIZE is set in header.misc, then build_id_bytes[20]
        // is the length of the build id (<= 20), and build_id_bytes[21..24] are unused.
        // Otherwise, the length of the build ID is unknown but at most 20, and has to be
        // detected by removing trailing 4-byte groups of zero bytes. (Regular build IDs
        // are 20 bytes long, so usually nothing gets removed.)
        // Simpleperf (as of June 2023) does not use PERF_RECORD_MISC_BUILD_ID_SIZE and fills
        // bytes 20..24 with uninitialized data, so those bytes have to be ignored.
        let build_id_len = if header.misc & PERF_RECORD_MISC_BUILD_ID_SIZE != 0 {
            build_id_bytes[20].min(20)
        } else {
            detect_build_id_len(&build_id_bytes[..20])
        };
        let build_id = build_id_bytes[..build_id_len as usize].to_owned();

        Ok(Self {
            header,
            _pid,
            build_id,
            file_path,
        })
    }
}
