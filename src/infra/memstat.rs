//! This process's own memory: what the panel's debug tab and the log show, and what
//! the memory investigations measured (.spec/GUIDE.md §25).

/// (working set, private bytes) of this process, in bytes.
#[cfg(windows)]
pub fn now() -> Option<(u64, u64)> {
    counters().map(|c| (c.0, c.1))
}

/// The highest working set this process has had, in bytes.
#[cfg(windows)]
pub fn peak() -> Option<u64> {
    counters().map(|c| c.2)
}

#[cfg(windows)]
fn counters() -> Option<(u64, u64, u64)> {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut _, c.cb) };
    (ok != 0).then_some((c.WorkingSetSize as u64, c.PrivateUsage as u64, c.PeakWorkingSetSize as u64))
}

#[cfg(not(windows))]
pub fn now() -> Option<(u64, u64)> {
    None
}

#[cfg(not(windows))]
pub fn peak() -> Option<u64> {
    None
}

/// `123 MB` for a byte count.
pub fn mb(b: u64) -> String {
    format!("{} MB", b / (1024 * 1024))
}
