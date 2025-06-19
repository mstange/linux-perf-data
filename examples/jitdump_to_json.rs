use linux_perf_data::jitdump::{JitDumpReader, JitDumpRecord};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use yaxpeax_arch::{Arch, DecodeError, Reader, U8Reader};

fn serialize_address_as_hex<S>(addr: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", addr))
}

fn read_file_contents(file_path: &str) -> Option<String> {
    match std::fs::read_to_string(file_path) {
        Ok(contents) => Some(contents),
        Err(_) => None, // File doesn't exist or can't be read
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DebugEntry {
    #[serde(serialize_with = "serialize_address_as_hex")]
    code_addr: u64,
    file_path: String,
    line: u32,
    column: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SourceFile {
    path: String,
    contents: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OutputData {
    functions: Vec<FunctionInfo>,
    source_files: Vec<SourceFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct InstructionInfo {
    offset: u32,
    instruction: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FunctionInfo {
    name: String,
    code_index: u64,
    pid: u32,
    tid: u32,
    #[serde(serialize_with = "serialize_address_as_hex")]
    address: u64,
    size: usize,
    timestamp: u64,
    debug_info: Vec<DebugEntry>,
    disassembly: Vec<InstructionInfo>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <function_name> [jitdump_file]", args[0]);
        std::process::exit(1);
    }
    
    let target_function = &args[1];
    let jitdump_file = args.get(2)
        .map(|s| s.as_str())
        .unwrap_or("/Users/mstange/Downloads/jit-34147.dump");

    let file = std::fs::File::open(jitdump_file)
        .unwrap_or_else(|e| {
            eprintln!("Failed to open {}: {}", jitdump_file, e);
            std::process::exit(1);
        });

    let mut reader = JitDumpReader::new(file).unwrap();
    let em_arch = reader.header().elf_machine_arch as u16;
    
    // Store debug info by code address
    let mut debug_info_map: HashMap<u64, Vec<DebugEntry>> = HashMap::new();
    let mut functions: Vec<FunctionInfo> = Vec::new();
    let mut source_files: HashMap<String, Option<String>> = HashMap::new();

    while let Ok(Some(record)) = reader.next_record() {
        let timestamp = record.timestamp;
        match record.parse().unwrap() {
            JitDumpRecord::CodeDebugInfo(debug_record) => {
                let debug_entries: Vec<DebugEntry> = debug_record.entries
                    .iter()
                    .map(|entry| {
                        let file_path = String::from_utf8_lossy(&entry.file_path.as_slice()).to_string();
                        
                        // Track unique source files
                        if !source_files.contains_key(&file_path) {
                            let file_contents = read_file_contents(&file_path);
                            source_files.insert(file_path.clone(), file_contents);
                        }
                        
                        DebugEntry {
                            code_addr: entry.code_addr,
                            file_path,
                            line: entry.line,
                            column: entry.column,
                        }
                    })
                    .collect();
                
                debug_info_map.insert(debug_record.code_addr, debug_entries);
            }
            JitDumpRecord::CodeLoad(load_record) => {
                let function_name = String::from_utf8_lossy(&load_record.function_name.as_slice()).to_string();
                
                // Check if this function matches our target
                if function_name.contains(target_function) {
                    let debug_entries = debug_info_map
                        .get(&load_record.code_addr)
                        .cloned()
                        .unwrap_or_default();
                    
                    let disassembly = disassemble_code(&load_record.code_bytes.as_slice(), em_arch)
                        .unwrap_or_else(|_| vec![InstructionInfo {
                            offset: 0,
                            instruction: "Failed to disassemble".to_string(),
                        }]);
                    
                    let function_info = FunctionInfo {
                        name: function_name,
                        code_index: load_record.code_index,
                        pid: load_record.pid,
                        tid: load_record.tid,
                        address: load_record.code_addr,
                        size: load_record.code_bytes.len(),
                        timestamp,
                        debug_info: debug_entries,
                        disassembly,
                    };
                    
                    functions.push(function_info);
                }
            }
            _ => {
                // Ignore other record types
            }
        }
    }

    if functions.is_empty() {
        eprintln!("No functions found matching '{}'", target_function);
        std::process::exit(1);
    }

    // Convert source files HashMap to Vec<SourceFile>
    let source_files_vec: Vec<SourceFile> = source_files
        .into_iter()
        .map(|(path, contents)| SourceFile { path, contents })
        .collect();

    // Create output data structure
    let output_data = OutputData {
        functions,
        source_files: source_files_vec,
    };

    // Output as JSON
    match serde_json::to_string_pretty(&output_data) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("Failed to serialize to JSON: {}", e);
            std::process::exit(1);
        }
    }
}

/// ARM
const EM_ARM: u16 = 40;
/// ARM AARCH64
const EM_AARCH64: u16 = 183;
/// Intel 80386
const EM_386: u16 = 3;
/// AMD x86-64 architecture
const EM_X86_64: u16 = 62;

fn disassemble_code(bytes: &[u8], elf_machine_arch: u16) -> Result<Vec<InstructionInfo>, String> {
    match elf_machine_arch {
        EM_386 => disassemble::<yaxpeax_x86::protected_mode::Arch>(bytes),
        EM_X86_64 => disassemble::<yaxpeax_x86::amd64::Arch>(bytes),
        EM_AARCH64 => disassemble::<yaxpeax_arm::armv8::a64::ARMv8>(bytes),
        EM_ARM => disassemble::<yaxpeax_arm::armv7::ARMv7>(bytes),
        _ => {
            Err(format!(
                "Unrecognized ELF machine architecture {elf_machine_arch}"
            ))
        }
    }
}

trait InstructionDecoding: Arch {
    const ADJUST_BY_AFTER_ERROR: usize;
    type InstructionDisplay<'a>: std::fmt::Display;
    fn make_decoder() -> Self::Decoder;
    fn inst_display(inst: &Self::Instruction) -> Self::InstructionDisplay<'_>;
}

impl InstructionDecoding for yaxpeax_x86::amd64::Arch {
    const ADJUST_BY_AFTER_ERROR: usize = 1;
    type InstructionDisplay<'a> = yaxpeax_x86::amd64::InstructionDisplayer<'a>;

    fn make_decoder() -> Self::Decoder {
        yaxpeax_x86::amd64::InstDecoder::default()
    }

    fn inst_display(inst: &Self::Instruction) -> Self::InstructionDisplay<'_> {
        inst.display_with(yaxpeax_x86::amd64::DisplayStyle::Intel)
    }
}

impl InstructionDecoding for yaxpeax_x86::protected_mode::Arch {
    const ADJUST_BY_AFTER_ERROR: usize = 1;
    type InstructionDisplay<'a> = &'a Self::Instruction;

    fn make_decoder() -> Self::Decoder {
        yaxpeax_x86::protected_mode::InstDecoder::default()
    }

    fn inst_display(inst: &Self::Instruction) -> Self::InstructionDisplay<'_> {
        inst
    }
}

impl InstructionDecoding for yaxpeax_arm::armv8::a64::ARMv8 {
    const ADJUST_BY_AFTER_ERROR: usize = 4;
    type InstructionDisplay<'a> = &'a Self::Instruction;

    fn make_decoder() -> Self::Decoder {
        yaxpeax_arm::armv8::a64::InstDecoder::default()
    }

    fn inst_display(inst: &Self::Instruction) -> Self::InstructionDisplay<'_> {
        inst
    }
}

impl InstructionDecoding for yaxpeax_arm::armv7::ARMv7 {
    const ADJUST_BY_AFTER_ERROR: usize = 2;
    type InstructionDisplay<'a> = &'a Self::Instruction;

    fn make_decoder() -> Self::Decoder {
        // Assume thumb. The Jitdump format doesn't seem to have a way of indicating
        // ARM or thumb mode for 32 bit arm functions.
        yaxpeax_arm::armv7::InstDecoder::default_thumb()
    }

    fn inst_display(inst: &Self::Instruction) -> Self::InstructionDisplay<'_> {
        inst
    }
}

fn disassemble<'a, A: InstructionDecoding>(bytes: &'a [u8]) -> Result<Vec<InstructionInfo>, String>
where
    u64: From<A::Address>,
    U8Reader<'a>: yaxpeax_arch::Reader<A::Address, A::Word>,
{
    use yaxpeax_arch::Decoder;
    let mut reader = yaxpeax_arch::U8Reader::new(bytes);
    let decoder = A::make_decoder();
    let mut offset = 0;
    let mut instructions = Vec::new();
    
    loop {
        let before = u64::from(reader.total_offset()) as u32;
        match decoder.decode(&mut reader) {
            Ok(inst) => {
                instructions.push(InstructionInfo {
                    offset,
                    instruction: format!("{}", A::inst_display(&inst)),
                });
                let after = u64::from(reader.total_offset()) as u32;
                offset += after - before;
            }
            Err(e) => {
                if e.data_exhausted() {
                    break;
                }

                let remaining_bytes = &bytes[offset as usize..];
                let s = remaining_bytes
                    .iter()
                    .take(A::ADJUST_BY_AFTER_ERROR)
                    .map(|b| format!("{b:#02x}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let s2 = remaining_bytes
                    .iter()
                    .take(A::ADJUST_BY_AFTER_ERROR)
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ");

                instructions.push(InstructionInfo {
                    offset,
                    instruction: format!(
                        ".byte {s:width$} # Invalid instruction {s2}: {e}",
                        width = A::ADJUST_BY_AFTER_ERROR * 6
                    ),
                });

                offset += A::ADJUST_BY_AFTER_ERROR as u32;
                let Some(reader_bytes) = bytes.get(offset as usize..) else {
                    break;
                };
                reader = U8Reader::new(reader_bytes);
            }
        }
    }
    
    Ok(instructions)
} 