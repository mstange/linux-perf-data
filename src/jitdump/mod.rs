//! Parsing code for [jitdump][jitdump] files.
//!
//! jitdump files usually have the name `jit-<pid>.dump`. They are associated
//! with a `perf.data` file via an `MMAP2` record. This means that the profiled
//! application which creates these files must also mmap them.
//!
//! The file contents are binary. The file starts with a file header. The header
//! is followed by a sequence of records.  Each record starts with a record header
//! with the record type, a timestamp, and the full size of the record.
//!
//! [jitdump]: https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/jitdump-specification.txt
//!
//! # Example
//!
//! ```
//! use linux_perf_data::jitdump::{JitDumpReader, JitDumpRecord};
//!
//! # fn wrapper() -> Result<(), Box<dyn std::error::Error>> {
//! let file = std::fs::File::open("jit-12345.dump")?;
//! let mut reader = JitDumpReader::new(file)?;
//! println!("jitdump header: {:?}", reader.header());
//!
//! while let Some(raw_record) = reader.next_record()? {
//!     let timestamp = raw_record.timestamp;
//!     match raw_record.parse()? {
//!         JitDumpRecord::CodeLoad(record) => {
//!             println!("{timestamp:016} LOAD {record:?}");
//!         }
//!         JitDumpRecord::CodeMove(record) => {
//!             println!("{timestamp:016} MOVE {record:?}");
//!         }
//!         JitDumpRecord::CodeDebugInfo(record) => {
//!             println!("{timestamp:016} DEBUG_INFO {record:?}");
//!         }
//!         JitDumpRecord::CodeClose => {
//!             println!("{timestamp:016} CLOSE");
//!         }
//!         JitDumpRecord::CodeUnwindingInfo(record) => {
//!             println!("{timestamp:016} UNWINDING_Info {record:?}");
//!         }
//!         JitDumpRecord::Other(record) => {
//!             println!("{timestamp:016} {} {record:?}", record.record_type.0);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```

mod buffered_reader;
mod error;
mod header;
mod jitdump_reader;
mod read_exact;
mod record;
mod records;

pub use error::*;
pub use header::*;
pub use jitdump_reader::*;
pub use record::*;
pub use records::*;
