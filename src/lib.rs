//! A parser for the perf.data file format.
//!
//! Files of this format consist of a header, a data section, and a few other
//! supplemental sections. The data section contains the main content of the
//! file: a sequence of records.
//!
//! There are two types of records: event records from the kernel, and "user
//! records" from perf / simpleperf.
//!
//! The [`jitdump`] module lets you parse jitdump files, which are used in
//! conjunction with perf.data files when profiling JIT runtimes.
//!
//! # Example
//!
//! ```
//! use linux_perf_data::{AttributeDescription, PerfFileReader, PerfFileRecord};
//!
//! # fn wrapper() -> Result<(), linux_perf_data::Error> {
//! let file = std::fs::File::open("perf.data")?;
//! let reader = std::io::BufReader::new(file);
//! let PerfFileReader { mut perf_file, mut record_iter } = PerfFileReader::parse_file(reader)?;
//! let event_names: Vec<_> =
//!     perf_file.event_attributes().iter().filter_map(AttributeDescription::name).collect();
//! println!("perf events: {}", event_names.join(", "));
//!
//! while let Some(record) = record_iter.next_record(&mut perf_file)? {
//!     match record {
//!         PerfFileRecord::EventRecord { attr_index, record } => {
//!             let record_type = record.record_type;
//!             let parsed_record = record.parse()?;
//!             println!("{:?} for event {}: {:?}", record_type, attr_index, parsed_record);
//!         }
//!         PerfFileRecord::UserRecord(record) => {
//!             let record_type = record.record_type;
//!             let parsed_record = record.parse()?;
//!             println!("{:?}: {:?}", record_type, parsed_record);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```

mod build_id_event;
mod constants;
mod dso_info;
mod dso_key;
mod error;
mod feature_sections;
mod features;
mod file_reader;
mod header;
pub mod jitdump;
mod perf_file;
mod record;
mod section;
mod simpleperf;
mod sorter;
mod thread_map;

/// This is a re-export of the linux-perf-event-reader crate. We use its types
/// in our public API.
pub use linux_perf_event_reader;

pub use linux_perf_event_reader::Endianness;

pub use dso_info::DsoInfo;
pub use dso_key::DsoKey;
pub use error::{Error, ReadError};
pub use feature_sections::{AttributeDescription, NrCpus, SampleTimeRange};
pub use features::{Feature, FeatureSet, FeatureSetIter};
pub use file_reader::{PerfFileReader, PerfRecordIter};
pub use perf_file::PerfFile;
pub use record::{PerfFileRecord, RawUserRecord, UserRecord, UserRecordType};
pub use thread_map::ThreadMap;
