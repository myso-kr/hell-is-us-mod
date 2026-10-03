//! Detect the running game and open it for reading and writing.

use crate::mem::Memory;
use std::ffi::c_void;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Process32FirstW, Process32NextW, MODULEENTRY32W, PROCESSENTRY32W,
    TH32CS_SNAPMODULE, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

pub const EXE_NAME: &str = "HellIsUs-Win64-Shipping.exe";

pub struct Game {
    pub pid: u32,
    pub base: u64,
    pub size: u64,
    pub exe: PathBuf,
    handle: HANDLE,
}

impl Game {
    /// `Ok(None)` when the game is not running.
    pub fn find() -> Result<Option<Game>, String> {
        let Some(pid) = find_pid()? else { return Ok(None) };
        let (base, size, exe) = main_module(pid)?;
        let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION;
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            return Err(trf!("게임 프로세스를 열 수 없음: {a0}", a0 = std::io::Error::last_os_error()));
        }
        Ok(Some(Game { pid, base, size, exe, handle }))
    }

    pub fn module(&self) -> (u64, u64) {
        (self.base, self.size)
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

impl Memory for Game {
    fn read(&self, addr: u64, buf: &mut [u8]) -> bool {
        let mut n = 0usize;
        let ok = unsafe {
            ReadProcessMemory(self.handle, addr as *const c_void, buf.as_mut_ptr().cast(), buf.len(), &mut n)
        };
        ok != 0 && n == buf.len()
    }

    fn write(&self, addr: u64, data: &[u8]) -> bool {
        let mut n = 0usize;
        let ok =
            unsafe { WriteProcessMemory(self.handle, addr as *const c_void, data.as_ptr().cast(), data.len(), &mut n) };
        ok != 0 && n == data.len()
    }
}

fn wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}

struct Snapshot(HANDLE);

impl Snapshot {
    fn new(flags: u32, pid: u32) -> Result<Snapshot, String> {
        let h = unsafe { CreateToolhelp32Snapshot(flags, pid) };
        if h == INVALID_HANDLE_VALUE {
            return Err(trf!("프로세스 목록을 얻지 못함: {a0}", a0 = std::io::Error::last_os_error()));
        }
        Ok(Snapshot(h))
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn find_pid() -> Result<Option<u32>, String> {
    let snap = Snapshot::new(TH32CS_SNAPPROCESS, 0)?;
    let mut e: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(snap.0, &mut e) };
    while ok != 0 {
        if wide(&e.szExeFile).eq_ignore_ascii_case(EXE_NAME) {
            return Ok(Some(e.th32ProcessID));
        }
        ok = unsafe { Process32NextW(snap.0, &mut e) };
    }
    Ok(None)
}

/// The first module of a process is its executable.
fn main_module(pid: u32) -> Result<(u64, u64, PathBuf), String> {
    let snap = Snapshot::new(TH32CS_SNAPMODULE, pid)?;
    let mut e: MODULEENTRY32W = unsafe { std::mem::zeroed() };
    e.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
    if unsafe { Module32FirstW(snap.0, &mut e) } == 0 {
        return Err(trf!("게임 모듈 목록을 얻지 못함: {a0}", a0 = std::io::Error::last_os_error()));
    }
    Ok((e.modBaseAddr as u64, e.modBaseSize as u64, PathBuf::from(wide(&e.szExePath))))
}

/// Ctrl+C sets this rather than killing the process, so a hold can put back what it
/// changed before exiting.
pub static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn catch_ctrl_c() {
    unsafe extern "system" fn on(_: u32) -> i32 {
        STOP.store(true, std::sync::atomic::Ordering::SeqCst);
        1
    }
    unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on), 1) };
}
