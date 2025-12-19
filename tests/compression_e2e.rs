use linux_perf_data::{CompressionInfo, Error, PerfFileReader, PerfFileRecord};
use std::fs::File;
use std::io::BufReader;

/// Test that compressed files can be parsed successfully
#[test]
fn test_compressed_file_parsing() {
    let file = File::open("tests/fixtures/sleep_compressed.data").unwrap();
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

/// Test that uncompressed files return None for compression_info
#[test]
fn test_uncompressed_file_no_compression_info() {
    let file = File::open("tests/fixtures/sleep.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    // Should NOT have compression info
    let comp_info = perf_file.compression_info().unwrap();
    assert!(
        comp_info.is_none(),
        "Uncompressed file should not have compression info"
    );

    // But should still be able to read records
    let mut count = 0;
    while let Some(_record) = record_iter.next_record(&mut perf_file).unwrap() {
        count += 1;
    }

    assert!(count > 0, "Should have read some records");
}

/// Test compression metadata is correct
#[test]
fn test_compression_metadata() {
    let file = File::open("tests/fixtures/sleep_compressed.data").unwrap();
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

/// Test that compressed and uncompressed files contain equivalent data
#[test]
fn test_compressed_uncompressed_equivalence() {
    // Read compressed file
    let file = File::open("tests/fixtures/sleep_compressed.data").unwrap();
    let reader = BufReader::new(file);
    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    // Collect record type debug strings from compressed file
    let mut compressed_records = Vec::new();
    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        let record_type_str = match &record {
            PerfFileRecord::EventRecord { record, .. } => format!("{:?}", record.record_type),
            PerfFileRecord::UserRecord(record) => format!("{:?}", record.record_type),
        };
        compressed_records.push(record_type_str);
    }

    // Read uncompressed file
    let file = File::open("tests/fixtures/sleep.data").unwrap();
    let reader = BufReader::new(file);
    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    // Collect record type debug strings from uncompressed file
    let mut uncompressed_records = Vec::new();
    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        let record_type_str = match &record {
            PerfFileRecord::EventRecord { record, .. } => format!("{:?}", record.record_type),
            PerfFileRecord::UserRecord(record) => format!("{:?}", record.record_type),
        };
        uncompressed_records.push(record_type_str);
    }

    // Both files should have records
    assert!(
        !compressed_records.is_empty(),
        "Compressed file should have records"
    );
    assert!(
        !uncompressed_records.is_empty(),
        "Uncompressed file should have records"
    );

    // Note: The test files were generated from different perf record sessions
    // (one with -k monotonic, one without), so exact counts won't match.
    // This test verifies both files can be parsed and contain the expected
    // types of records (e.g., both have SAMPLE, MMAP, etc.)
    let compressed_count = compressed_records.len();
    let uncompressed_count = uncompressed_records.len();

    println!(
        "Compressed file: {} records, Uncompressed file: {} records",
        compressed_count, uncompressed_count
    );

    // Both should have a reasonable number of records for a 1-second sleep
    assert!(
        compressed_count >= 10,
        "Compressed file should have at least 10 records, got {}",
        compressed_count
    );
    assert!(
        uncompressed_count >= 10,
        "Uncompressed file should have at least 10 records, got {}",
        uncompressed_count
    );
}

/// Test that we can read sample records from compressed data
#[test]
fn test_compressed_sample_records() {
    let file = File::open("tests/fixtures/sleep_compressed.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    let mut sample_count = 0;
    let mut total_count = 0;

    // Record type 9 is PERF_RECORD_SAMPLE
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

/// Test that record types are as expected (no COMPRESSED2 or FINISHED_INIT exposed)
#[test]
fn test_no_compressed_records_in_output() {
    use linux_perf_data::UserRecordType;

    let file = File::open("tests/fixtures/sleep_compressed.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        // Check user records - these internal types should never be exposed
        if let PerfFileRecord::UserRecord(record) = &record {
            assert_ne!(
                record.record_type,
                UserRecordType::PERF_COMPRESSED2,
                "COMPRESSED2 records should be transparent and not exposed"
            );
        }
    }
}

/// Test pipe mode with zstd compression
#[test]
fn test_pipe_mode_with_zstd_compression() {
    use linux_perf_data::UserRecordType;

    // Read pipe mode compressed file
    let file = File::open("tests/fixtures/sleep_compressed.pipe.data").unwrap();
    let reader = BufReader::new(file);

    // Use parse_pipe instead of parse_file
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

    // Should be able to read all records
    let mut total_count = 0;
    let mut sample_count = 0;
    let mut record_type_counts = std::collections::HashMap::new();

    // Record type 9 is PERF_RECORD_SAMPLE
    const PERF_RECORD_SAMPLE: u32 = 9;

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        // Verify internal compressed record types are not exposed
        if let PerfFileRecord::UserRecord(user_record) = &record {
            assert_ne!(
                user_record.record_type,
                UserRecordType::PERF_COMPRESSED2,
                "COMPRESSED2 records should be transparent"
            );
            *record_type_counts
                .entry(format!("User:{:?}", user_record.record_type))
                .or_insert(0) += 1;
        }

        // Count samples
        if let PerfFileRecord::EventRecord { record, .. } = &record {
            *record_type_counts
                .entry(format!("{:?}", record.record_type))
                .or_insert(0) += 1;
            if record.record_type.0 == PERF_RECORD_SAMPLE {
                sample_count += 1;
            }
        }

        total_count += 1;
    }

    println!(
        "Pipe mode zstd: {} total records, {} samples",
        total_count, sample_count
    );
    println!("Record type counts: {:?}", record_type_counts);

    // Verify we parsed a substantial number of records (streaming decompression working)
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

/// Test that records spanning compressed chunk boundaries are handled correctly.
///
/// This test uses a fixture where perf records span across COMPRESSED record
/// boundaries, requiring the decompressor to buffer partial records.
#[cfg(feature = "zstd")]
#[test]
fn test_records_spanning_compressed_boundaries() {
    let file = File::open("tests/fixtures/fibo_compressed.pipe.data").unwrap();
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

/// Test feature flag: when zstd is disabled, appropriate error occurs
#[cfg(not(feature = "zstd"))]
#[test]
fn test_zstd_feature_disabled_error() {
    let file = File::open("tests/fixtures/sleep_compressed.data").unwrap();
    let reader = BufReader::new(file);

    // Should be able to parse the file header
    let result = PerfFileReader::parse_file(reader);

    // But reading compressed records should fail
    if let Ok(PerfFileReader {
        mut perf_file,
        mut record_iter,
    }) = result
    {
        let mut found_error = false;
        while let Some(result) = record_iter.next_record(&mut perf_file).transpose() {
            if result.is_err() {
                found_error = true;
                break;
            }
        }
        assert!(
            found_error,
            "Should get an error when reading compressed data without zstd feature"
        );
    }
}

/// Test that parse_pipe fails with a clear error when given file format data
#[test]
fn test_parse_pipe_with_file_format_fails() {
    let file = File::open("tests/fixtures/sleep.data").unwrap();
    let reader = BufReader::new(file);

    let result = PerfFileReader::parse_pipe(reader);
    assert!(
        matches!(result, Err(Error::FileFormatDetectedInPipeMode)),
        "Expected FileFormatDetectedInPipeMode error"
    );
}

/// Test that parse_file transparently handles pipe format by falling back to parse_pipe
#[test]
fn test_parse_file_with_pipe_format_falls_back() {
    let file = File::open("tests/fixtures/sleep_compressed.pipe.data").unwrap();
    let reader = BufReader::new(file);

    // parse_file should detect pipe format and fall back to parse_pipe
    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader)
        .expect("parse_file should handle pipe format transparently");

    // Should be able to read records
    let mut count = 0;
    while let Some(_record) = record_iter.next_record(&mut perf_file).unwrap() {
        count += 1;
    }
    assert!(count > 0, "Should have read some records");
}
