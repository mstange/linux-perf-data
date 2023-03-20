[![crates.io page](https://img.shields.io/crates/v/linux-perf-data.svg)](https://crates.io/crates/linux-perf-data)
[![docs.rs page](https://docs.rs/linux-perf-data/badge.svg)](https://docs.rs/linux-perf-data/)

# linux-perf-data

A parser for the perf.data file format.

Files of this format consist of a header, a data section, and a few other
supplemental sections. The data section contains the main content of the
file: a sequence of records.

There are two types of records: event records from the kernel, and "user
records" from perf / simpleperf.

This crate also contains parsing code for jitdump files, which are used
in conjunction with perf.data files when profiling JIT runtimes.

# Example

```rust
use linux_perf_data::{AttributeDescription, PerfFileReader, PerfFileRecord};

let file = std::fs::File::open("perf.data")?;
let reader = std::io::BufReader::new(file);
let PerfFileReader { mut perf_file, mut record_iter } = PerfFileReader::parse_file(reader)?;
let event_names: Vec<_> =
    perf_file.event_attributes().iter().filter_map(AttributeDescription::name).collect();
println!("perf events: {}", event_names.join(", "));

while let Some(record) = record_iter.next_record(&mut perf_file)? {
    match record {
        PerfFileRecord::EventRecord { attr_index, record } => {
            let record_type = record.record_type;
            let parsed_record = record.parse()?;
            println!("{:?} for event {}: {:?}", record_type, attr_index, parsed_record);
        }
        PerfFileRecord::UserRecord(record) => {
            let record_type = record.record_type;
            let parsed_record = record.parse()?;
            println!("{:?}: {:?}", record_type, parsed_record);
        }
    }
}
```

## Jitdump example

```rust
use linux_perf_data::jitdump::{JitDumpReader, JitDumpRecord};

let file = std::fs::File::open("jit-12345.dump")?;
let mut reader = JitDumpReader::new(file)?;
println!("jitdump header: {:?}", reader.header());

while let Some(raw_record) = reader.next_record()? {
    let timestamp = raw_record.timestamp;
    match raw_record.parse()? {
        JitDumpRecord::CodeLoad(record) => {
            println!("{timestamp:016} LOAD {record:?}");
        }
        JitDumpRecord::CodeMove(record) => {
            println!("{timestamp:016} MOVE {record:?}");
        }
        JitDumpRecord::CodeDebugInfo(record) => {
            println!("{timestamp:016} DEBUG_INFO {record:?}");
        }
        JitDumpRecord::CodeClose => {
            println!("{timestamp:016} CLOSE");
        }
        JitDumpRecord::CodeUnwindingInfo(record) => {
            println!("{timestamp:016} UNWINDING_Info {record:?}");
        }
        JitDumpRecord::Other(record) => {
            println!("{timestamp:016} {} {record:?}", record.record_type.0);
        }
    }
}
```

## License

Licensed under either of

  * Apache License, Version 2.0 ([`LICENSE-APACHE`](./LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
  * MIT license ([`LICENSE-MIT`](./LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
