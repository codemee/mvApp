//! 螢幕列舉。

use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::BOOL;

/// 簡單矩形（left/top/right/bottom，螢幕像素座標）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rc {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rc {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

impl From<RECT> for Rc {
    fn from(r: RECT) -> Self {
        Rc { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

#[derive(Clone, Debug)]
pub struct Monitor {
    pub handle: isize,
    pub name: String,
    pub rect: Rc,
    pub work: Rc,
    pub primary: bool,
    pub dpi: u32,
}

unsafe extern "system" fn enum_cb(hm: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let list = unsafe { &mut *(lp.0 as *mut Vec<Monitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(hm, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if ok.as_bool() {
        let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
        let name = String::from_utf16_lossy(&info.szDevice[..len]);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = unsafe { GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
        list.push(Monitor {
            handle: hm.0 as isize,
            name,
            rect: info.monitorInfo.rcMonitor.into(),
            work: info.monitorInfo.rcWork.into(),
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
            dpi: dx,
        });
    }
    BOOL(1)
}

/// 列出所有使用中的螢幕，依實體位置由左至右、上至下排序。
pub fn list() -> Vec<Monitor> {
    let mut v: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(enum_cb), LPARAM(&mut v as *mut _ as isize));
    }
    v.sort_by_key(|m| (m.rect.left, m.rect.top));
    v
}
