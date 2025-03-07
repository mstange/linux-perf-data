use std::fmt;

use byteorder::{ByteOrder, NativeEndian};

use linux_perf_event_reader::{is_swapped_endian, RawData};

/// A list of threads, usually without names.
///
/// It's not clear to me what the point of this list is. It doesn't even give you the
/// pid of the process that each thread belongs to. And unless you use `perf stat`,
/// it doesn't seem to have thread names either.
///
/// So it seems like all the useful information is instead in the PERF_RECORD_COMM
/// records which get synthesized at the start of a file for `perf record -p <pid>`.
/// It seems you're better of just reading those, instead of looking at the thread map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMap<'a> {
    swap_endian: bool,
    data: RawData<'a>,
}

const THREAD_ENTRY_SIZE: usize = 8 + 16;

impl<'a> ThreadMap<'a> {
    pub fn parse<T: ByteOrder>(mut data: RawData<'a>) -> Result<Self, std::io::Error> {
        let len = data.read_u64::<T>()?;
        let len = usize::try_from(len).map_err(|_| std::io::ErrorKind::InvalidData)?;
        let datalen = len
            .checked_mul(THREAD_ENTRY_SIZE)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        let data = data.split_off_prefix(datalen)?;
        Ok(Self {
            swap_endian: is_swapped_endian::<T>(),
            data,
        })
    }

    pub fn len(&self) -> usize {
        self.data.len() / THREAD_ENTRY_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> ThreadMapIter<'a> {
        ThreadMapIter {
            swap_endian: self.swap_endian,
            index: 0,
            len: self.len(),
            data: self.data,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ThreadMapEntry<'a> {
    /// The tid of this thread.
    pub tid: u64,
    /// The name is usually empty, unfortunately. It looks like `thread_map__read_comms`
    /// only gets called by `perf stat`, not by `perf record`.
    pub name: RawData<'a>,
}

impl fmt::Debug for ThreadMapEntry<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use std::str;

        let mut map = fmt.debug_map();
        map.entry(&"tid", &self.tid);

        if let Ok(string) = str::from_utf8(&self.name.as_slice()) {
            map.entry(&"name", &string);
        } else {
            map.entry(&"name", &self.name);
        }

        map.finish()
    }
}

pub struct ThreadMapIter<'a> {
    swap_endian: bool,
    data: RawData<'a>,
    index: usize,
    len: usize,
}

impl<'a> Iterator for ThreadMapIter<'a> {
    type Item = ThreadMapEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }

        let mut tid = self.data.read_u64::<NativeEndian>().unwrap();
        if self.swap_endian {
            tid = tid.swap_bytes();
        }
        let mut name = self.data.split_off_prefix(16).unwrap();
        let name = name.read_string().unwrap_or(name);
        self.index += 1;
        Some(ThreadMapEntry { tid, name })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.len))
    }
}

#[cfg(test)]
mod test {
    use byteorder::LittleEndian;
    use linux_perf_event_reader::RawData;

    use super::ThreadMap;

    #[test]
    fn parse_one() {
        let data = RawData::Single(&[
            1, 0, 0, 0, 0, 0, 0, 0, 108, 71, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ]);
        let map = ThreadMap::parse::<LittleEndian>(data).unwrap();
        assert_eq!(map.len(), 1);
        let vec: Vec<_> = map.iter().collect();
        assert_eq!(vec.len(), 1);
        assert_eq!(vec[0].tid, 542572);
        assert_eq!(&vec[0].name.as_slice()[..], b"");
    }

    #[test]
    fn parse_big() {
        let data = RawData::Single(&[
            12, 0, 0, 0, 0, 0, 0, 0, 165, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 169, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 171, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 172,
            115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 187, 115, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 188, 115, 1, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 189, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 190, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 191, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 194, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            197, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 199, 115,
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        let map = ThreadMap::parse::<LittleEndian>(data).unwrap();
        assert_eq!(map.len(), 12);
        let vec: Vec<_> = map.iter().collect();
        assert_eq!(vec.len(), 12);
        assert_eq!(vec[8].tid, 95167);
        assert_eq!(&vec[8].name.as_slice()[..], b"");
    }
}
