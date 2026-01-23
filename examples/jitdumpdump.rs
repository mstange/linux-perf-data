use linux_perf_data::jitdump::{JitDumpReader, JitDumpRecord};
use yaxpeax_arch::{Arch, DecodeError, Reader, U8Reader};

fn main() {
    let file = std::fs::File::open(
        std::env::args()
            .nth(1)
            .unwrap_or("/Users/mstange/Downloads/jit-34147.dump".into()),
    )
    .unwrap();

    let mut reader = JitDumpReader::new(file).unwrap();
    let em_arch = reader.header().elf_machine_arch as u16;

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
                let _ = decode_arch(&record.code_bytes.as_slice(), em_arch);
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
            JitDumpRecord::Inline(record) => {
                println!("{timestamp:016} INLINE INFO");
                println!("  address: {:#x}", record.code_addr);
                for entry in &record.entries {
                    println!(
                        "  {}:{}:{}",
                        std::str::from_utf8(&entry.func_name.as_slice()).unwrap(),
                        entry.line,
                        entry.column,
                    );
                }
            }
            JitDumpRecord::Other(record) => {
                println!("{timestamp:016} <unknown type {}>", record.record_type.0);
                println!();
            }
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

fn decode_arch(bytes: &[u8], elf_machine_arch: u16) -> Result<(), String> {
    match elf_machine_arch {
        EM_386 => decode::<yaxpeax_x86::protected_mode::Arch>(bytes),
        EM_X86_64 => decode::<yaxpeax_x86::amd64::Arch>(bytes),
        EM_AARCH64 => decode::<yaxpeax_arm::armv8::a64::ARMv8>(bytes),
        EM_ARM => decode::<yaxpeax_arm::armv7::ARMv7>(bytes),
        _ => {
            return Err(format!(
                "Unrecognized ELF machine architecture {elf_machine_arch}"
            ));
        }
    }
    Ok(())
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

fn decode<'a, A: InstructionDecoding>(bytes: &'a [u8])
where
    u64: From<A::Address>,
    U8Reader<'a>: yaxpeax_arch::Reader<A::Address, A::Word>,
{
    use yaxpeax_arch::Decoder;
    let mut reader = yaxpeax_arch::U8Reader::new(bytes);
    let decoder = A::make_decoder();
    let mut offset = 0;
    loop {
        let before = u64::from(reader.total_offset()) as u32;
        match decoder.decode(&mut reader) {
            Ok(inst) => {
                println!("{offset:6x}  {}", A::inst_display(&inst));
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

                println!(
                    "{offset:6x}  .byte {s:width$} # Invalid instruction {s2}: {e}",
                    width = A::ADJUST_BY_AFTER_ERROR * 6
                );

                offset += A::ADJUST_BY_AFTER_ERROR as u32;
                let Some(reader_bytes) = bytes.get(offset as usize..) else {
                    break;
                };
                reader = U8Reader::new(reader_bytes);
            }
        }
    }
    println!();
}
