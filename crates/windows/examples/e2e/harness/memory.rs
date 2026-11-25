struct Memory {
    working_mb: usize,
    peak_mb: usize,
    private_mb: usize,
}

fn memory() -> Memory {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
    }
    Memory {
        working_mb: counters.WorkingSetSize >> 20,
        peak_mb: counters.PeakWorkingSetSize >> 20,
        private_mb: counters.PagefileUsage >> 20,
    }
}

pub(crate) fn print_memory(stage: &str) {
    let m = memory();
    println!(
        "[MEM] {stage}: working set {} MB, peak {} MB, private {} MB",
        m.working_mb, m.peak_mb, m.private_mb
    );
}
