//! 列舉可見的頂層 GUI 視窗。

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromRect};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetWindow, GetWindowLongW, GetWindowPlacement,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, WINDOWPLACEMENT, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
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

/// 視窗「實際會出現」的矩形：最小化時 `GetWindowRect` 會回傳 (-25600, …) 之類的假座標，
/// 改取還原後的位置（`rcNormalPosition`）。
pub fn effective_rect(h: HWND) -> Option<RECT> {
    unsafe {
        if IsIconic(h).as_bool() {
            let mut wp = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            GetWindowPlacement(h, &mut wp).ok()?;
            Some(wp.rcNormalPosition)
        } else {
            let mut r = RECT::default();
            GetWindowRect(h, &mut r).ok()?;
            Some(r)
        }
    }
}

/// 視窗所在螢幕（依 [`effective_rect`] 判斷，最小化的視窗也能正確歸屬）。
pub fn monitor_of(h: HWND) -> isize {
    match effective_rect(h) {
        Some(r) => unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST).0 as isize },
        None => 0,
    }
}

/// 同一個行程中、目前隱藏但有標題的頂層視窗。
/// Delphi 等程式的工作列按鈕是一個隱形的代理視窗，真正的主視窗是隱藏的，
/// 搬移代理視窗時必須連同它們一起搬，否則下次顯示時仍會出現在原本（甚至已不存在）的位置。
pub fn hidden_siblings(pid: u32, except: HWND) -> Vec<HWND> {
    struct C {
        pid: u32,
        except: HWND,
        out: Vec<HWND>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let c = unsafe { &mut *(lp.0 as *mut C) };
        unsafe {
            if hwnd == c.except || IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            let mut p = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut p));
            if p != c.pid || GetWindowTextLengthW(hwnd) <= 0 {
                return BOOL(1);
            }
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 {
                return BOOL(1);
            }
            let mut r = RECT::default();
            if GetWindowRect(hwnd, &mut r).is_ok() && r.right - r.left > 1 && r.bottom - r.top > 1 {
                c.out.push(hwnd);
            }
        }
        BOOL(1)
    }
    let mut c = C { pid, except, out: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut c as *mut _ as isize));
    }
    c.out
}

pub fn pid_of(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
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
        ctx.out.push(WinInfo {
            hwnd: hwnd.0 as isize,
            title,
            exe_path: exe_path_of(pid),
            monitor: monitor_of(hwnd),
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
