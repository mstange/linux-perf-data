use linux_perf_event_reader::CpuMode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DsoKey {
    Kernel,
    GuestKernel,
    Vdso32,
    VdsoX32,
    Vdso64,
    Vsyscall,
    KernelModule(String),
    User(String, Vec<u8>),
}

impl DsoKey {
    pub fn detect(path: &[u8], cpu_mode: CpuMode) -> Option<Self> {
        if path == b"//anon" || path == b"[stack]" || path == b"[heap]" || path == b"[vvar]" {
            return None;
        }

        if path.starts_with(b"[kernel.kallsyms]") {
            let dso_key = if cpu_mode == CpuMode::GuestKernel {
                DsoKey::GuestKernel
            } else {
                DsoKey::Kernel
            };
            return Some(dso_key);
        }
        if path.starts_with(b"[guest.kernel.kallsyms") {
            return Some(DsoKey::GuestKernel);
        }
        if path == b"[vdso32]" {
            return Some(DsoKey::Vdso32);
        }
        if path == b"[vdsox32]" {
            return Some(DsoKey::VdsoX32);
        }
        if path == b"[vdso]" {
            // TODO: I think this could also be Vdso32 when recording on a 32 bit machine.
            return Some(DsoKey::Vdso64);
        }
        if path == b"[vsyscall]" {
            return Some(DsoKey::Vsyscall);
        }
        if (cpu_mode == CpuMode::Kernel || cpu_mode == CpuMode::GuestKernel)
            && path.starts_with(b"[")
        {
            return Some(DsoKey::KernelModule(String::from_utf8_lossy(path).into()));
        }

        let filename = if let Some(final_slash_pos) = path.iter().rposition(|b| *b == b'/') {
            &path[final_slash_pos + 1..]
        } else {
            path
        };

        let dso_key = match (cpu_mode, filename.strip_suffix(b".ko")) {
            (CpuMode::Kernel | CpuMode::GuestKernel, Some(kmod_name)) => {
                // "/lib/modules/5.13.0-35-generic/kernel/sound/core/snd-seq-device.ko" -> "[snd-seq-device]"
                let kmod_name = String::from_utf8_lossy(kmod_name);
                DsoKey::KernelModule(format!("[{}]", kmod_name))
            }
            (CpuMode::Kernel, _) => DsoKey::Kernel,
            (CpuMode::GuestKernel, _) => DsoKey::GuestKernel,
            (CpuMode::User | CpuMode::GuestUser, _) => {
                DsoKey::User(String::from_utf8_lossy(filename).into(), path.to_owned())
            }
            _ => return None,
        };
        Some(dso_key)
    }

    pub fn name(&self) -> &str {
        match self {
            DsoKey::Kernel => "[kernel.kallsyms]",
            DsoKey::GuestKernel => "[guest.kernel.kallsyms]",
            DsoKey::Vdso32 => "[vdso32]",
            DsoKey::VdsoX32 => "[vdsox32]",
            DsoKey::Vdso64 => "[vdso]",
            DsoKey::Vsyscall => "[vsyscall]",
            DsoKey::KernelModule(name) => name,
            DsoKey::User(name, _) => name,
        }
    }
}
