use std::collections::HashMap;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use linux_perf_event_reader::Endianness;
use prost::Message;

use crate::Error;

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

/// Constants used in [`SimpleperfFileRecord`]'s `type` property.
pub mod simpleperf_dso_type {
    pub const DSO_KERNEL: u32 = 0;
    pub const DSO_KERNEL_MODULE: u32 = 1;
    pub const DSO_ELF_FILE: u32 = 2;
    /// For files containing dex files, like .vdex files.
    pub const DSO_DEX_FILE: u32 = 3;
    pub const DSO_SYMBOL_MAP_FILE: u32 = 4;
    pub const DSO_UNKNOWN_FILE: u32 = 5;
}

/// Used in the `SIMPLEPERF_FILE2` section.
///
/// Carries symbol tables that were obtained on the device.
#[derive(Clone, PartialEq, Eq, ::prost_derive::Message)]
pub struct SimpleperfFileRecord {
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    /// Uses constants from [`simpleperf_dso_type`].
    #[prost(uint32, tag = "2")]
    pub r#type: u32,
    #[prost(uint64, tag = "3")]
    pub min_vaddr: u64,
    #[prost(message, repeated, tag = "4")]
    pub symbol: ::prost::alloc::vec::Vec<SimpleperfSymbol>,
    #[prost(oneof = "SimpleperfTypeSpecificInfo", tags = "5, 6, 7")]
    pub type_specific_msg: ::core::option::Option<SimpleperfTypeSpecificInfo>,
}

/// A single symbol, contained in the symbol table inside a [`SimpleperfFileRecord`].
#[derive(Clone, PartialEq, Eq, ::prost_derive::Message)]
pub struct SimpleperfSymbol {
    #[prost(uint64, tag = "1")]
    pub vaddr: u64,
    #[prost(uint32, tag = "2")]
    pub len: u32,
    #[prost(string, tag = "3")]
    pub name: ::prost::alloc::string::String,
}

/// DEX-specific info inside a [`SimpleperfFileRecord`].
#[derive(Clone, PartialEq, Eq, ::prost_derive::Message)]
pub struct SimpleperfDexFileInfo {
    #[prost(uint64, repeated, tag = "1")]
    pub dex_file_offset: ::prost::alloc::vec::Vec<u64>,
}

/// ELF object specific info inside a [`SimpleperfFileRecord`].
#[derive(Clone, PartialEq, Eq, ::prost_derive::Message)]
pub struct SimpleperfElfFileInfo {
    #[prost(uint64, tag = "1")]
    pub file_offset_of_min_vaddr: u64,
}

/// Kernel module specific info inside a [`SimpleperfFileRecord`].
#[derive(Clone, PartialEq, Eq, ::prost_derive::Message)]
pub struct SimpleperfKernelModuleInfo {
    #[prost(uint64, tag = "1")]
    pub memory_offset_of_min_vaddr: u64,
}

/// Type-specif info inside a [`SimpleperfFileRecord`].
#[derive(Clone, PartialEq, Eq, ::prost_derive::Oneof)]
pub enum SimpleperfTypeSpecificInfo {
    /// Only when type = DSO_DEX_FILE
    #[prost(message, tag = "5")]
    SimpleperfDexFileInfo(SimpleperfDexFileInfo),
    /// Only when type = DSO_ELF_FILE
    #[prost(message, tag = "6")]
    ElfFile(SimpleperfElfFileInfo),
    /// Only when type = DSO_KERNEL_MODULE
    #[prost(message, tag = "7")]
    KernelModule(SimpleperfKernelModuleInfo),
}

pub fn parse_file2_section(
    mut bytes: &[u8],
    endian: Endianness,
) -> Result<Vec<SimpleperfFileRecord>, Error> {
    let mut files = Vec::new();
    // `bytes` contains the sequence of encoded SimpleperfFileRecord.
    // Each record is proceded by a u32 which is the length in bytes
    // of the protobuf-encoded representation.
    while !bytes.is_empty() {
        let len = match endian {
            Endianness::LittleEndian => bytes.read_u32::<LittleEndian>()?,
            Endianness::BigEndian => bytes.read_u32::<BigEndian>()?,
        };
        let len = len as usize;
        let file_data = bytes.get(..len).ok_or(Error::FeatureSectionTooSmall)?;
        bytes = &bytes[len..];
        let file = SimpleperfFileRecord::decode(file_data)
            .map_err(Error::ProtobufParsingSimpleperfFileSection)?;
        files.push(file);
    }
    Ok(files)
}
