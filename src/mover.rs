//! 視窗搬移邏輯。

use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsIconic, IsZoomed, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SWP_NOACTIVATE,
    SWP_NOZORDER, SetWindowPos, ShowWindow,
};

use crate::monitors::{Monitor, Rc};
use crate::windows_list::hwnd_of;

/// 搬移失敗原因（顯示文字由 UI 依語言決定）。
#[derive(Clone, Debug)]
pub enum MoveError {
    /// 找不到視窗目前所在的螢幕。
    NoSourceMonitor,
    /// 讀取視窗位置失敗。
    ReadRect,
    /// 系統拒絕搬移（常見於以系統管理員身分執行的程式）。
    Denied(String),
}

/// 把 `r`（位於 `src` 工作區）依相對位置與大小換算到 `dst` 工作區，並夾限在 `dst` 內。
pub fn map_rect(src: Rc, dst: Rc, r: Rc) -> Rc {
    let (sw, sh) = (src.width().max(1) as f64, src.height().max(1) as f64);
    let (dw, dh) = (dst.width() as f64, dst.height() as f64);
    let w = ((r.width() as f64) * dw / sw).round().clamp(1.0, dw);
    let h = ((r.height() as f64) * dh / sh).round().clamp(1.0, dh);
    let x = dst.left as f64 + ((r.left - src.left) as f64) * dw / sw;
    let y = dst.top as f64 + ((r.top - src.top) as f64) * dh / sh;
    let x = x.round().clamp(dst.left as f64, (dst.right as f64 - w).max(dst.left as f64));
    let y = y.round().clamp(dst.top as f64, (dst.bottom as f64 - h).max(dst.top as f64));
    Rc { left: x as i32, top: y as i32, right: (x + w) as i32, bottom: (y + h) as i32 }
}

/// 把視窗搬到目標螢幕。
pub fn move_window(hwnd: isize, dst: &Monitor, monitors: &[Monitor]) -> Result<(), MoveError> {
    let h = hwnd_of(hwnd);
    let cur = unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) };
    let src = monitors
        .iter()
        .find(|m| m.handle == cur.0 as isize)
        .ok_or(MoveError::NoSourceMonitor)?;
    if src.handle == dst.handle {
        return Ok(());
    }
    unsafe {
        let was_min = IsIconic(h).as_bool();
        if was_min {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let was_max = IsZoomed(h).as_bool();
        if was_max {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        GetWindowRect(h, &mut rect).map_err(|_| MoveError::ReadRect)?;
        let n = map_rect(src.work, dst.work, rect.into());
        // 跨 DPI 螢幕時，系統可能在第一次移動後重新調整大小，故設定兩次
        let mut res = Ok(());
        for _ in 0..2 {
            res = SetWindowPos(
                h,
                None,
                n.left,
                n.top,
                n.width(),
                n.height(),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if res.is_err() {
                break;
            }
        }
        if was_max {
            let _ = ShowWindow(h, SW_MAXIMIZE);
        }
        if was_min {
            let _ = ShowWindow(h, SW_MINIMIZE);
        }
        if let Err(e) = res {
            return Err(MoveError::Denied(e.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rc(l: i32, t: i32, r: i32, b: i32) -> Rc {
        Rc { left: l, top: t, right: r, bottom: b }
    }

    #[test]
    fn same_size_monitor_keeps_relative_position() {
        let src = rc(0, 0, 1920, 1080);
        let dst = rc(1920, 0, 3840, 1080);
        let r = map_rect(src, dst, rc(100, 50, 900, 650));
        assert_eq!(r, rc(2020, 50, 2820, 650));
    }

    #[test]
    fn scales_to_larger_monitor() {
        let src = rc(0, 0, 1000, 500);
        let dst = rc(1000, 0, 3000, 1000);
        let r = map_rect(src, dst, rc(100, 100, 600, 400));
        assert_eq!(r, rc(1200, 200, 2200, 800));
    }

    #[test]
    fn clamps_into_target_work_area() {
        let src = rc(0, 0, 1920, 1080);
        let dst = rc(1920, 0, 2920, 600);
        let r = map_rect(src, dst, rc(-500, -200, 2500, 1500));
        assert!(r.left >= dst.left && r.right <= dst.right);
        assert!(r.top >= dst.top && r.bottom <= dst.bottom);
    }
}
