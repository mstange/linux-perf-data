use linux_perf_event_reader::RawData;
use std::io::{Read, Seek, SeekFrom};

use super::read_exact::ReadExactOrUntilEof;

/// A wrapper for file which allows reading a file in chunks while also
/// referencing the internal buffer bytes. Optimized for low memory use
/// and minimal copies.
///
/// Works with `Read` implementations which "grow", i.e. read() might return
/// 0 during one call and >0 during the next call (because the file now contains
/// more bytes).
#[derive(Debug, Clone)]
pub struct BufferedReader<R: Read> {
    reader: R,
    /// Always stays the same size, and is always the destination of reader reads
    fixed_buf: Vec<u8>,
    /// Used when records straddle fixed_buf chunks, grown to accomodate record size
    dynamic_buf: Vec<u8>,
    read_pos: ReadPos,
    write_pos: usize,
}

#[derive(Debug, Clone)]
enum ReadPos {
    AtPosInFixedBuf(usize),
    AtPosInDynamicBuf(usize),
}

impl<R: Read> BufferedReader<R> {
    pub fn new_with_partially_read_buffer(
        reader: R,
        buf: Vec<u8>,
        consumed_len: usize,
        write_pos: usize,
    ) -> Self {
        assert!(consumed_len < buf.len());
        Self {
            reader,
            fixed_buf: buf,
            read_pos: ReadPos::AtPosInFixedBuf(consumed_len),
            write_pos,
            dynamic_buf: Vec::new(),
        }
    }

    pub fn consume_data(&mut self, len: usize) -> Result<Option<RawData<'_>>, std::io::Error> {
        let available_data_len = self.available_data_len();
        if available_data_len < len {
            let extra_needed_data = len - available_data_len;
            if !self.read_n_more_bytes(extra_needed_data)? {
                return Ok(None);
            }
        }
        let (data, new_read_pos) = match self.read_pos {
            ReadPos::AtPosInFixedBuf(fixed_buf_read_pos) => {
                let new_fixed_buf_read_pos = fixed_buf_read_pos + len;
                assert!(new_fixed_buf_read_pos <= self.write_pos);
                let data = &self.fixed_buf[fixed_buf_read_pos..new_fixed_buf_read_pos];
                (
                    RawData::Single(data),
                    ReadPos::AtPosInFixedBuf(new_fixed_buf_read_pos),
                )
            }
            ReadPos::AtPosInDynamicBuf(dynamic_buf_read_pos) => {
                let remaining_dynamic_buf_len = self.dynamic_buf.len() - dynamic_buf_read_pos;
                if len < remaining_dynamic_buf_len {
                    let new_dynamic_buf_read_pos = dynamic_buf_read_pos + len;
                    let data = &self.dynamic_buf[dynamic_buf_read_pos..new_dynamic_buf_read_pos];
                    (
                        RawData::Single(data),
                        ReadPos::AtPosInDynamicBuf(new_dynamic_buf_read_pos),
                    )
                } else {
                    let unread_dynamic_buf_data = &self.dynamic_buf[dynamic_buf_read_pos..];
                    let new_fixed_buf_read_pos = len - remaining_dynamic_buf_len;
                    assert!(new_fixed_buf_read_pos <= self.write_pos);
                    let unread_fixed_buf_data = &self.fixed_buf[..new_fixed_buf_read_pos];
                    (
                        RawData::Split(unread_dynamic_buf_data, unread_fixed_buf_data),
                        ReadPos::AtPosInFixedBuf(new_fixed_buf_read_pos),
                    )
                }
            }
        };
        self.read_pos = new_read_pos;
        Ok(Some(data))
    }

    fn available_data_len(&self) -> usize {
        match self.read_pos {
            ReadPos::AtPosInFixedBuf(fixed_buf_read_pos) => self.write_pos - fixed_buf_read_pos,
            ReadPos::AtPosInDynamicBuf(dynamic_buf_read_pos) => {
                let unread_dynamic_buf_data_len = self.dynamic_buf.len() - dynamic_buf_read_pos;
                let unread_fixed_buf_data_len = self.write_pos;
                unread_dynamic_buf_data_len + unread_fixed_buf_data_len
            }
        }
    }

    fn read_n_more_bytes(&mut self, n: usize) -> Result<bool, std::io::Error> {
        let mut extra_bytes_achieved = 0;
        while extra_bytes_achieved < n {
            if self.write_pos < self.fixed_buf.len() {
                // We have space in fixed_buf to read into. Do so.
                let extra_len = self
                    .reader
                    .read_exact_or_until_eof(&mut self.fixed_buf[self.write_pos..])?;
                self.write_pos += extra_len;
                extra_bytes_achieved += extra_len;
                if self.write_pos < self.fixed_buf.len() {
                    // We've hit EOF.
                    break;
                }
            } else {
                // No space in fixed_buf. Move the current stuff to dynamic_buf.
                match self.read_pos {
                    ReadPos::AtPosInFixedBuf(fixed_buf_read_pos) => {
                        self.dynamic_buf.clear();
                        self.dynamic_buf
                            .extend_from_slice(&self.fixed_buf[fixed_buf_read_pos..]);
                    }
                    ReadPos::AtPosInDynamicBuf(dynamic_buf_read_pos) => {
                        self.dynamic_buf.drain(0..dynamic_buf_read_pos);
                        self.dynamic_buf.extend_from_slice(&self.fixed_buf);
                    }
                }
                self.read_pos = ReadPos::AtPosInDynamicBuf(0);
                self.write_pos = 0; // self.fixed_buf is now fully available for writing.
            }
        }

        Ok(extra_bytes_achieved >= n)
    }
}

impl<R: Read + Seek> BufferedReader<R> {
    pub fn skip_bytes(&mut self, len: usize) -> Result<(), std::io::Error> {
        let available_data_len = self.available_data_len();
        if available_data_len < len {
            let extra_bytes_to_skip = len - available_data_len;
            self.reader
                .seek(SeekFrom::Current(extra_bytes_to_skip as i64))?;
            self.read_pos = ReadPos::AtPosInFixedBuf(0);
            self.write_pos = 0;
            return Ok(());
        }
        match self.read_pos {
            ReadPos::AtPosInFixedBuf(fixed_buf_read_pos) => {
                let new_fixed_buf_read_pos = fixed_buf_read_pos + len;
                assert!(new_fixed_buf_read_pos <= self.write_pos);
                self.read_pos = ReadPos::AtPosInFixedBuf(new_fixed_buf_read_pos);
            }
            ReadPos::AtPosInDynamicBuf(dynamic_buf_read_pos) => {
                let remaining_dynamic_buf_len = self.dynamic_buf.len() - dynamic_buf_read_pos;
                if len < remaining_dynamic_buf_len {
                    let new_dynamic_buf_read_pos = dynamic_buf_read_pos + len;
                    self.read_pos = ReadPos::AtPosInDynamicBuf(new_dynamic_buf_read_pos);
                } else {
                    let new_fixed_buf_read_pos = len - remaining_dynamic_buf_len;
                    assert!(new_fixed_buf_read_pos <= self.write_pos);
                    self.read_pos = ReadPos::AtPosInFixedBuf(new_fixed_buf_read_pos);
                }
            }
        }
        Ok(())
    }
}
