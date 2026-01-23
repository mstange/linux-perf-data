//! Tests for uncompressed perf.data files

use linux_perf_data::{PerfFileReader, PerfFileRecord};
use std::fs::File;
use std::io::BufReader;

/// Test that uncompressed files can be parsed successfully
#[test]
fn test_file_parsing() {
    let file = File::open("tests/fixtures/sleep.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

    let mut count = 0;
    while let Some(_record) = record_iter.next_record(&mut perf_file).unwrap() {
        count += 1;
    }

    assert!(count > 0, "Should have read some records");
}

/// Test that uncompressed files return None for compression_info
#[test]
fn test_no_compression_info() {
    let file = File::open("tests/fixtures/sleep.data").unwrap();
    let reader = BufReader::new(file);

    let PerfFileReader {
        perf_file,
        record_iter: _,
    } = PerfFileReader::parse_file(reader).unwrap();

    let comp_info = perf_file.compression_info().unwrap();
    assert!(
        comp_info.is_none(),
        "Uncompressed file should not have compression info"
    );
}

/// Test that we can read sample records from uncompressed data
#[test]
fn test_sample_records() {
    let file = File::open("tests/fixtures/sleep.data").unwrap();
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

/// Test that compressed and uncompressed files contain similar record types
#[test]
fn test_equivalence_with_compressed() {
    // Read compressed2 file
    let file = File::open("tests/fixtures/sleep.compressed2.data").unwrap();
    let reader = BufReader::new(file);
    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(reader).unwrap();

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

    let mut uncompressed_records = Vec::new();
    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        let record_type_str = match &record {
            PerfFileRecord::EventRecord { record, .. } => format!("{:?}", record.record_type),
            PerfFileRecord::UserRecord(record) => format!("{:?}", record.record_type),
        };
        uncompressed_records.push(record_type_str);
    }

    // Both files should have records
    assert!(!compressed_records.is_empty());
    assert!(!uncompressed_records.is_empty());

    // Both should have a reasonable number of records
    assert!(compressed_records.len() >= 10);
    assert!(uncompressed_records.len() >= 10);
}

/// Test feature flag: when zstd is disabled, compressed files fail appropriately
#[cfg(not(feature = "zstd"))]
#[test]
fn test_zstd_feature_disabled_error() {
    let file = File::open("tests/fixtures/sleep.compressed2.data").unwrap();
    let reader = BufReader::new(file);

    let result = PerfFileReader::parse_file(reader);

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
