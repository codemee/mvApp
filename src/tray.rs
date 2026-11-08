//! 系統匣：圖示、右鍵選單（結束）、雙按顯示主視窗，以及「關閉＝縮回系統匣」。

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;

use eframe::egui;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{COLORREF, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongW, IsIconic, LWA_ALPHA, PostMessageW, SW_HIDE, SW_RESTORE, SW_SHOW,
    SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongW, ShowWindow, WM_CLOSE,
    WS_EX_LAYERED,
};

use crate::autostart;
use crate::i18n::Tr;
use crate::windows_list::hwnd_of;

/// 主視窗 HWND（事件處理函式可能在 egui 更新迴圈之外被呼叫，因此直接用 Win32 操作視窗）。
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
/// 使用者已從系統匣選單要求結束。
static QUIT: AtomicBool = AtomicBool::new(false);

pub fn set_main_hwnd(h: isize) {
    MAIN_HWND.store(h, Ordering::SeqCst);
}

pub fn quit_requested() -> bool {
    QUIT.load(Ordering::SeqCst)
}

/// 啟動期間視窗是否被設為全透明（用來遮住 eframe 第一幀後強制顯示的那一瞬間）。
static STARTUP_INVISIBLE: AtomicBool = AtomicBool::new(false);

/// 把主視窗設為全透明；eframe 在第一幀後顯示視窗時，使用者也看不到。
/// 之後 `hide_main` / `show_main` 會自動還原。
pub fn make_invisible_for_startup() {
    let h = MAIN_HWND.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    let hw = hwnd_of(h);
    unsafe {
        let ex = GetWindowLongW(hw, GWL_EXSTYLE);
        SetWindowLongW(hw, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as i32);
        if SetLayeredWindowAttributes(hw, COLORREF(0), 0, LWA_ALPHA).is_ok() {
            STARTUP_INVISIBLE.store(true, Ordering::SeqCst);
        } else {
            SetWindowLongW(hw, GWL_EXSTYLE, ex); // 失敗就還原，退回「會閃一下」的行為
        }
    }
}

/// 還原不透明（移除 layered 樣式）。
fn restore_opaque() {
    if STARTUP_INVISIBLE.swap(false, Ordering::SeqCst) {
        let h = MAIN_HWND.load(Ordering::SeqCst);
        if h != 0 {
            let hw = hwnd_of(h);
            unsafe {
                let ex = GetWindowLongW(hw, GWL_EXSTYLE);
                SetWindowLongW(hw, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as i32));
            }
        }
    }
}

/// 顯示並喚到前景。
pub fn show_main() {
    let h = MAIN_HWND.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    restore_opaque();
    let hw = hwnd_of(h);
    unsafe {
        if IsIconic(hw).as_bool() {
            let _ = ShowWindow(hw, SW_RESTORE);
        } else {
            let _ = ShowWindow(hw, SW_SHOW);
        }
        let _ = SetForegroundWindow(hw);
    }
}

/// 隱藏主視窗（縮回系統匣）。
pub fn hide_main() {
    let h = MAIN_HWND.load(Ordering::SeqCst);
    if h != 0 {
        unsafe {
            let _ = ShowWindow(hwnd_of(h), SW_HIDE);
        }
        restore_opaque();
    }
}

/// 要求結束：先走正常關閉流程；若視窗隱藏時事件迴圈沒有處理，稍後強制結束。
fn request_quit() {
    QUIT.store(true, Ordering::SeqCst);
    let h = MAIN_HWND.load(Ordering::SeqCst);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(hwnd_of(h)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(1500));
        std::process::exit(0);
    });
}

pub struct Tray {
    icon: TrayIcon,
    quit_item: MenuItem,
    autostart_item: CheckMenuItem,
}

impl Tray {
    pub fn new(ctx: &egui::Context, icon_png: &[u8], tr: Tr) -> Result<Self, String> {
        // 優先使用 exe 內嵌 ICO 的原生尺寸（16/20/24/32…），縮小後才不會線條破碎；
        // 資源不存在時才退回 PNG。資源序號 2 對應 app.rc 的 ICON。
        let icon = match Icon::from_resource(2, None) {
            Ok(i) => i,
            Err(_) => {
                let img = eframe::icon_data::from_png_bytes(icon_png).map_err(|e| e.to_string())?;
                Icon::from_rgba(img.rgba, img.width, img.height).map_err(|e| e.to_string())?
            }
        };

        // 若已啟用開機自動執行但路徑過期，順手更新
        autostart::refresh_path_if_stale();

        let autostart_item = CheckMenuItem::new(tr.tray_autostart(), true, autostart::is_enabled(), None);
        let quit_item = MenuItem::new(tr.tray_quit(), true, None);
        let menu = Menu::new();
        menu.append(&autostart_item).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;
        let quit_id = quit_item.id().clone();
        let autostart_id = autostart_item.id().clone();

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip(tr.app_title())
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;

        let c = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = e {
                show_main();
                c.request_repaint();
            }
        }));
        let c = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == quit_id {
                request_quit();
                c.request_repaint();
            } else if e.id == autostart_id {
                // 以登錄檔的實際狀態為準來切換（選單的勾選狀態由 muda 自動翻轉，兩者會一致）
                let want = !autostart::is_enabled();
                if autostart::set_enabled(want).is_err() {
                    eprintln!("無法變更開機自動執行設定");
                }
            }
        }));

        Ok(Tray { icon: tray, quit_item, autostart_item })
    }

    /// 語言改變時更新選單文字與提示。
    pub fn set_lang(&self, tr: Tr) {
        self.quit_item.set_text(tr.tray_quit());
        self.autostart_item.set_text(tr.tray_autostart());
        let _ = self.icon.set_tooltip(Some(tr.app_title()));
    }
}
