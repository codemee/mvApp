//! 單一執行個體：同一個 Windows 登入工作階段內只允許執行一份。
//!
//! 作法：用具名 Mutex 判斷是否已有實例；第二次啟動時，透過具名 Event 通知第一個實例
//! 把主視窗叫出來，然後自己結束。名稱前綴 `Local\` 表示只限目前登入工作階段，
//! 不同使用者（或遠端工作階段）各自可以執行一份。

use std::ffi::c_void;

use eframe::egui;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
};
use windows::core::w;

use crate::tray;

/// 持有這個物件代表「我是第一個實例」。
/// 兩個 HANDLE 刻意不關閉：要一直持有到程式結束，Mutex 才會持續生效。
pub struct Guard {
    show_event: HANDLE,
}

pub enum Acquire {
    /// 這是第一個實例，可以繼續啟動。
    First(Guard),
    /// 已有其他實例在執行。
    AlreadyRunning,
}

/// 嘗試取得單一執行個體的所有權。
pub fn acquire() -> Acquire {
    unsafe {
        let mutex = CreateMutexW(None, false, w!(r"Local\mvApp.SingleInstance"));
        // CreateMutexW 在 Mutex 已存在時仍會成功，要立刻檢查 LastError 才知道
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        if mutex.is_err() {
            // 建立不了 Mutex（極少見）：寧可讓程式照常啟動，也不要因此打不開
            return Acquire::First(Guard { show_event: HANDLE::default() });
        }
        if already {
            return Acquire::AlreadyRunning;
        }
        // 自動重設的事件：第二個實例 SetEvent 一次，第一個實例就被喚醒一次
        let event = CreateEventW(None, false, false, w!(r"Local\mvApp.ShowWindow")).unwrap_or_default();
        Acquire::First(Guard { show_event: event })
    }
}

/// 通知已在執行的實例：把主視窗顯示出來。
pub fn notify_existing() {
    unsafe {
        if let Ok(ev) = OpenEventW(EVENT_MODIFY_STATE, false, w!(r"Local\mvApp.ShowWindow")) {
            let _ = SetEvent(ev);
        }
    }
}

impl Guard {
    /// 開一條背景執行緒，等待第二個實例的通知，收到就顯示主視窗。
    pub fn spawn_listener(&self, ctx: &egui::Context) {
        if self.show_event.is_invalid() {
            return;
        }
        // HANDLE 內含指標、不能直接跨執行緒；改傳數值，進執行緒再還原
        let raw = self.show_event.0 as usize;
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("single-instance-listener".into()).spawn(move || {
            let ev = HANDLE(raw as *mut c_void);
            loop {
                let r = unsafe { WaitForSingleObject(ev, INFINITE) };
                if r.0 != 0 {
                    break; // 等待失敗（事件被關閉等），結束監聽
                }
                tray::show_main();
                ctx.request_repaint();
            }
        });
    }
}
