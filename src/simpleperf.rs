use std::collections::HashMap;

pub struct SimplePerfEventType {
    pub name: String,
    pub type_: u64,
    pub config: u64,
}

impl SimplePerfEventType {
    pub fn new(name: String, type_: u64, config: u64) -> Self {
        Self {
            name,
            type_,
            config,
        }
    }
}

/// Parse a nul-byte-separated list of (key, value) pairs into a string map.
///
/// Simpleperf assembles the info map contents here: https://cs.android.com/android/platform/superproject/+/main:system/extras/simpleperf/cmd_record.cpp;l=2109-2157;drc=aec31f83f65ac7c58e67c9605d9cc438545f5c94
///
/// # Example:
///
/// ```plaintext
/// {
///     "android_sdk_version": "33",
///     "android_build_type": "user",
///     "event_type_info": "cpu-clock,1,0\nsched:sched_switch,2,45",
///     "trace_offcpu": "true",
///     "app_type": "debuggable",
///     "product_props": "samsung:SM-A515F:a51nseea",
///     "clockid": "monotonic",
///     "system_wide_collection": "false",
///     "android_version": "13",
///     "kernel_version": "4.14.113-25950142",
///     "android_build_fingerprint": "samsung/a51nseea/a51:13/TP1A.220624.014/A515FXXU7HWF1:user/release-keys",
///     "app_package_name": "org.mozilla.geckoview_example",
///     "kernel_symbols_available": "false",
///     "timestamp": "1696864401",
///     "simpleperf_version": "1.build.7848450",
/// }
/// ```
pub fn parse_meta_info_map(bytes: &[u8]) -> Result<HashMap<&str, &str>, std::str::Utf8Error> {
    let iter = bytes.split(|c| *c == b'\0');
    let keys = iter.clone().step_by(2);
    let values = iter.skip(1).step_by(2);
    let mut map = HashMap::new();
    for (key, value) in keys.zip(values) {
        let key = std::str::from_utf8(key)?;
        let value = std::str::from_utf8(value)?;
        map.insert(key, value);
    }
    Ok(map)
}

pub fn get_event_types(meta_info_map: &HashMap<&str, &str>) -> Option<Vec<SimplePerfEventType>> {
    let event_type_info = meta_info_map.get("event_type_info")?;
    let mut event_types = Vec::new();
    for line in event_type_info.split('\n') {
        let mut parts = line.split(',');
        let name = parts.next()?.to_string();
        let type_ = parts.next()?.parse().ok()?;
        let config = parts.next()?.parse().ok()?;
        event_types.push(SimplePerfEventType::new(name, type_, config));
    }
    Some(event_types)
}
