use linux_perf_data::jitdump::{JitDumpReader, JitDumpRecord};

fn main() {
    let file = std::fs::File::open(
        std::env::args()
            .nth(1)
            .unwrap_or("/Users/mstange/Downloads/jit-34147.dump".into()),
    )
    .unwrap();

    let mut reader = JitDumpReader::new(file).unwrap();

    while let Ok(Some(record)) = reader.next_record() {
        let timestamp = record.timestamp;
        match record.parse().unwrap() {
            JitDumpRecord::CodeLoad(record) => {
                println!(
                    "{timestamp:016} LOAD {} (pid: {}, tid: {})",
                    record.code_index, record.pid, record.tid
                );
                println!(
                    "  address: {:#x}, size: {:#x}, name: {}",
                    record.code_addr,
                    record.code_bytes.len(),
                    std::str::from_utf8(&record.function_name.as_slice()).unwrap()
                );
                println!();
            }
            JitDumpRecord::CodeMove(record) => {
                println!(
                    "{timestamp:016} MOVE {} (pid: {}, tid: {})",
                    record.code_index, record.pid, record.tid
                );
                println!(
                    "  address: {:#x} -> {:#x}, size: {:#x}",
                    record.old_code_addr, record.new_code_addr, record.code_size
                );
                println!();
            }
            JitDumpRecord::CodeDebugInfo(record) => {
                println!("{timestamp:016} DEBUG INFO");
                println!("  address: {:#x}", record.code_addr);
                for entry in &record.entries {
                    println!(
                        "  {:#8x} {}:{}:{}",
                        entry.code_addr,
                        std::str::from_utf8(&entry.file_path.as_slice()).unwrap(),
                        entry.line,
                        entry.column
                    );
                }
                println!();
            }
            JitDumpRecord::CodeClose => {
                println!("{timestamp:016} CLOSE");
                println!();
            }
            JitDumpRecord::CodeUnwindingInfo(_record) => {
                println!("{timestamp:016} UNWINDING INFO");
                println!();
            }
            JitDumpRecord::Other(record) => {
                println!("{timestamp:016} <unknown type {}>", record.record_type.0);
                println!();
            }
        }
    }
}
