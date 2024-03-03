use std::collections::HashMap;

use linux_perf_data::{PerfFileReader, PerfFileRecord};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: perfdatainfo <perf.data>");
    let file = std::fs::File::open(path).unwrap();
    let reader = std::io::BufReader::new(file);
    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = match PerfFileReader::parse_file(reader) {
        Ok(reader) => reader,
        Err(e) => {
            println!("ERROR when creating PerfFileReader: {:?}", e);
            return;
        }
    };

    let mut event_record_map = HashMap::new();
    let mut user_record_map = HashMap::new();

    while let Some(record) = record_iter.next_record(&mut perf_file).unwrap() {
        match record {
            PerfFileRecord::EventRecord { attr_index, record } => {
                let record_type = record.record_type;
                *event_record_map
                    .entry(attr_index)
                    .or_insert_with(HashMap::new)
                    .entry(record_type)
                    .or_insert(0) += 1;
                match record.parse() {
                    Ok(_parsed_record) => {
                        // println!(
                        //     "{:?} for event {}: {:?}",
                        //     record_type, attr_index, parsed_record
                        // );
                    }
                    Err(e) => {
                        println!(
                            "ERROR when parsing {:?} for event {}: {:?}",
                            record_type, attr_index, e
                        );
                    }
                }
            }
            PerfFileRecord::UserRecord(record) => {
                let record_type = record.record_type;
                *user_record_map.entry(record_type).or_insert(0) += 1;
                match record.parse() {
                    Ok(_parsed_record) => {
                        // println!("{:?}: {:?}", record_type, parsed_record);
                    }
                    Err(e) => {
                        println!("ERROR when parsing {:?}: {:?}", record_type, e);
                    }
                }
            }
        }
    }

    let mut event_record_map = event_record_map
        .into_iter()
        .map(|(attr_index, histogram)| {
            let sum = histogram.values().sum::<u64>();
            (attr_index, histogram, sum)
        })
        .collect::<Vec<_>>();
    event_record_map.sort_by_key(|(_attr_index, _histogram, sum)| -(*sum as i64));
    let sum = event_record_map
        .iter()
        .map(|(_attr_index, _histogram, sum)| sum)
        .sum::<u64>();

    println!("Event records: {sum} records");
    println!();

    for (attr_index, record_counts, sum) in event_record_map {
        let mut record_counts = record_counts.into_iter().collect::<Vec<_>>();
        record_counts.sort_by_key(|(_record_type, count)| -(*count as i64));
        println!(
            "  event {} ({}): {} records",
            attr_index,
            perf_file.event_attributes()[attr_index]
                .name()
                .unwrap_or("<no event name found>"),
            sum
        );
        for (record_type, count) in record_counts {
            println!("    {:?}: {}", record_type, count);
        }
        println!();
    }

    let mut user_record_counts = user_record_map.into_iter().collect::<Vec<_>>();
    user_record_counts.sort_by_key(|(_record_type, count)| -(*count as i64));
    let sum = user_record_counts
        .iter()
        .map(|(_record_type, count)| count)
        .sum::<u64>();

    println!("User records: {sum} records");
    println!();
    for (record_type, count) in user_record_counts {
        println!("  {:?}: {}", record_type, count);
    }
}
