//! 列舉可見的頂層 GUI 視窗。

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetWindow, GetWindowLongW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR};

#[derive(Clone, Debug)]
pub struct WinInfo {
    pub hwnd: isize,
    pub title: String,
    pub exe_path: String,
    pub monitor: isize,
    pub minimized: bool,
}

impl WinInfo {
    pub fn exe_name(&self) -> &str {
        self.exe_path.rsplit(['\\', '/']).next().unwrap_or(&self.exe_path)
    }
}

struct Ctx {
    own_pid: u32,
    out: Vec<WinInfo>,
}

pub fn hwnd_of(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn exe_path_of(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        if ok.is_ok() { String::from_utf16_lossy(&buf[..len as usize]) } else { String::new() }
    }
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    let ctx = unsafe { &mut *(lp.0 as *mut Ctx) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if GetWindow(hwnd, GW_OWNER).map(|h| !h.is_invalid()).unwrap_or(false) {
            return BOOL(1);
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 {
            return BOOL(1);
        }
        if ex & WS_EX_NOACTIVATE.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 {
            return BOOL(1);
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return BOOL(1);
        }
        let tlen = GetWindowTextLengthW(hwnd);
        if tlen <= 0 {
            return BOOL(1);
        }
        let mut tbuf = vec![0u16; tlen as usize + 1];
        let n = GetWindowTextW(hwnd, &mut tbuf);
        let title = String::from_utf16_lossy(&tbuf[..n.max(0) as usize]);
        if title.is_empty() {
            return BOOL(1);
        }
        let mut cbuf = [0u16; 64];
        let cn = GetClassNameW(hwnd, &mut cbuf);
        let class = String::from_utf16_lossy(&cbuf[..cn.max(0) as usize]);
        if class == "Progman" || class == "WorkerW" {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == ctx.own_pid {
            return BOOL(1);
        }
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        ctx.out.push(WinInfo {
            hwnd: hwnd.0 as isize,
            title,
            exe_path: exe_path_of(pid),
            monitor: mon.0 as isize,
            minimized: IsIconic(hwnd).as_bool(),
        });
    }
    BOOL(1)
}

pub fn list() -> Vec<WinInfo> {
    let mut ctx = Ctx { own_pid: std::process::id(), out: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.out
}
