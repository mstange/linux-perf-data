//! Tests for PERF_RECORD_COMPRESSED (type 81) - the original format from Linux 5.2 (2019)

use linux_perf_data::{PerfFileReader, PerfFileRecord, UserRecordType};
use std::fs::File;
use std::io::BufReader;

/// Test that compressed (type 81) files can be parsed successfully
#[cfg(feature = "zstd")]
#[test]
fn test_file_parsing() {
    let file = File::open("tests/fixtures/sleep.compressed.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    // Should have compression info
    let comp_info = perf_file.compression_info().unwrap();
    assert!(
        comp_info.is_some(),
        "Compressed file should have compression info"
    );

    // Count records - should be able to read them all
    let mut count = 0;
    while let Some(_record) = record_iter.next_record(&mut perf_file).unwrap() {
        count += 1;
    }

    assert!(count > 0, "Should have read some records");
}

/// Test pipe mode with PERF_RECORD_COMPRESSED (type 81)
#[cfg(feature = "zstd")]
#[test]
fn test_pipe_mode() {
    // Read pipe mode compressed file (type 81)
    let file = File::open("tests/fixtures/sleep.compressed.pipe.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_pipe(reader).unwrap();

    // Should have compression info even in pipe mode
    let comp_info = perf_file.compression_info().unwrap();
    assert!(
        comp_info.is_some(),
        "Pipe mode should support compression info"
    );

    let mut total_count = 0;
    let mut sample_count = 0;
    const PERF_RECORD_SAMPLE: u32 = 9;

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        // Verify internal compressed record types are not exposed
        if let PerfFileRecord::UserRecord(user_record) = &record {
            assert_ne!(
                user_record.record_type,
                UserRecordType::PERF_COMPRESSED,
                "COMPRESSED records should be transparent"
            );
        }

        if let PerfFileRecord::EventRecord { record, .. } = &record {
            if record.record_type.0 == PERF_RECORD_SAMPLE {
                sample_count += 1;
            }
        }

        total_count += 1;
    }

    assert!(
        total_count > 0,
        "Should have read records from pipe mode, got {}",
        total_count
    );
    assert!(
        sample_count > 0,
        "Should have found sample records in pipe mode"
    );
}

/// Test that COMPRESSED records are not exposed in output
#[cfg(feature = "zstd")]
#[test]
fn test_records_not_exposed() {
    let file = File::open("tests/fixtures/sleep.compressed.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        if let PerfFileRecord::UserRecord(record) = &record {
            assert_ne!(
                record.record_type,
                UserRecordType::PERF_COMPRESSED,
                "COMPRESSED records should be transparent and not exposed"
            );
        }
    }
}
