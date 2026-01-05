//! Tests for PERF_RECORD_COMPRESSED2 (type 83) - the newer format from May 2025
//! that fixes 8-byte alignment issues with the original format.

use linux_perf_data::{CompressionInfo, PerfFileReader, PerfFileRecord, UserRecordType};
use std::fs::File;
use std::io::BufReader;

/// Test that compressed2 files can be parsed successfully
#[test]
fn test_file_parsing() {
    let file = File::open("tests/fixtures/sleep.compressed2.data").unwrap();
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

/// Test compression metadata is correct
#[test]
fn test_compression_metadata() {
    let file = File::open("tests/fixtures/sleep.compressed2.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        perf_file,
        record_iter: _,
    } = PerfFileReader::parse_file(reader).unwrap();

    let comp_info = perf_file
        .compression_info()
        .unwrap()
        .expect("Compressed file should have compression info");

    // Should be using Zstd compression
    assert_eq!(
        comp_info.type_,
        CompressionInfo::ZSTD_TYPE,
        "Should be using Zstd compression"
    );

    // Compression level should be in valid Zstd range (1-22, or 0 for default)
    assert!(
        comp_info.level <= 22,
        "Compression level should be <= 22, got {}",
        comp_info.level
    );

    // Ratio and mmap_len should be non-zero
    assert!(comp_info.ratio > 0, "Compression ratio should be non-zero");
    assert!(comp_info.mmap_len > 0, "mmap_len should be non-zero");
}

/// Test that we can read sample records from compressed2 data
#[test]
fn test_sample_records() {
    let file = File::open("tests/fixtures/sleep.compressed2.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    let mut sample_count = 0;
    let mut total_count = 0;
    const PERF_RECORD_SAMPLE: u32 = 9;

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        if let PerfFileRecord::EventRecord { record, .. } = record {
            if record.record_type.0 == PERF_RECORD_SAMPLE {
                sample_count += 1;
            }
        }
        total_count += 1;
    }

    assert!(total_count > 0, "Should have processed some records");
    assert!(
        sample_count > 0,
        "Should have found at least one SAMPLE record"
    );
}

/// Test that COMPRESSED2 records are not exposed in output
#[test]
fn test_records_not_exposed() {
    let file = File::open("tests/fixtures/sleep.compressed2.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        if let PerfFileRecord::UserRecord(record) = &record {
            assert_ne!(
                record.record_type,
                UserRecordType::PERF_COMPRESSED2,
                "COMPRESSED2 records should be transparent and not exposed"
            );
        }
    }
}

/// Test pipe mode with zstd compression (compressed2 format)
#[test]
fn test_pipe_mode() {
    // Read pipe mode compressed2 file
    let file = File::open("tests/fixtures/sleep.compressed2.pipe.data").unwrap();
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

    if let Some(info) = comp_info {
        assert_eq!(
            info.type_,
            CompressionInfo::ZSTD_TYPE,
            "Should be using Zstd compression"
        );
        assert!(info.level <= 22, "Compression level should be valid");
    }

    let mut total_count = 0;
    let mut sample_count = 0;
    const PERF_RECORD_SAMPLE: u32 = 9;

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        // Verify internal compressed record types are not exposed
        if let PerfFileRecord::UserRecord(user_record) = &record {
            assert_ne!(
                user_record.record_type,
                UserRecordType::PERF_COMPRESSED2,
                "COMPRESSED2 records should be transparent"
            );
        }

        if let PerfFileRecord::EventRecord { record, .. } = &record {
            if record.record_type.0 == PERF_RECORD_SAMPLE {
                sample_count += 1;
            }
        }

        total_count += 1;
    }

    // Verify we parsed a substantial number of records
    assert!(
        total_count >= 100,
        "Should have read at least 100 records from pipe mode, got {}",
        total_count
    );
    assert!(
        sample_count > 0,
        "Should have found sample records in pipe mode"
    );
}

/// Test that records spanning compressed2 chunk boundaries are handled correctly.
///
/// This test uses a fixture where perf records span across COMPRESSED2 record
/// boundaries, requiring the decompressor to buffer partial records.
#[cfg(feature = "zstd")]
#[test]
fn test_records_spanning_boundaries() {
    let file = File::open("tests/fixtures/fibo.compressed2.pipe.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_pipe(reader).unwrap();

    const PERF_RECORD_SAMPLE: u32 = 9;

    let mut total_count = 0;
    let mut sample_count = 0;

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        if let PerfFileRecord::EventRecord { record, .. } = &record {
            if record.record_type.0 == PERF_RECORD_SAMPLE {
                sample_count += 1;
            }
        }
        total_count += 1;
    }

    // This file has records that span compressed chunk boundaries.
    // Without proper partial record handling, we'd get fewer records or errors.
    assert!(
        total_count > 1000,
        "Expected >1000 records (got {}), partial record handling may be broken",
        total_count
    );
    assert!(
        sample_count > 500,
        "Expected >500 samples (got {}), partial record handling may be broken",
        sample_count
    );
}
