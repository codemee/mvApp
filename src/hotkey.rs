//! 全域快捷鍵：在任何程式中按下即可叫出主視窗（預設 Ctrl+Shift+M）。
//!
//! `RegisterHotKey` 要求註冊與接收 `WM_HOTKEY` 在同一條執行緒，而 eframe 的事件迴圈不會
//! 處理執行緒訊息，所以另開一條專用執行緒跑自己的訊息迴圈；變更快捷鍵時以
//! `PostThreadMessageW` 通知它重新註冊，並同步等待結果（成功／已被其他程式佔用）。

use std::sync::{OnceLock, mpsc};
use std::time::Duration;

use eframe::egui;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY,
};

use crate::tray;

/// 一組快捷鍵：修飾鍵＋一個主鍵（Windows 虛擬鍵碼）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub vk: u32,
}

/// 預設：Ctrl+Shift+M。
pub const DEFAULT: Hotkey = Hotkey { ctrl: true, alt: false, shift: true, vk: 0x4D };

impl Hotkey {
    /// 至少要有 Ctrl 或 Alt；只用 Shift 或不用修飾鍵會讓一般打字無法使用該鍵。
    pub fn is_valid(&self) -> bool {
        (self.ctrl || self.alt) && key_name(self.vk).is_some()
    }

    /// 顯示用文字，例如 `Ctrl+Shift+M`；同時也是設定檔的存放格式。
    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(&key_name(self.vk).unwrap_or_default());
        s
    }

    /// 解析 [`Hotkey::label`] 的格式（不分大小寫）；不合法時回傳 `None`。
    pub fn parse(text: &str) -> Option<Self> {
        let mut hk = Hotkey { ctrl: false, alt: false, shift: false, vk: 0 };
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" => hk.ctrl = true,
                "alt" => hk.alt = true,
                "shift" => hk.shift = true,
                _ if hk.vk == 0 => hk.vk = key_from_name(part)?,
                _ => return None, // 出現第二個主鍵
            }
        }
        hk.is_valid().then_some(hk)
    }

    /// 由 egui 的按鍵事件組成快捷鍵；主鍵不在支援範圍（A–Z、0–9、F1–F12）時回傳 `None`。
    pub fn from_egui(key: egui::Key, m: egui::Modifiers) -> Option<Self> {
        let vk = key_from_name(key.name())?;
        Some(Hotkey { ctrl: m.ctrl, alt: m.alt, shift: m.shift, vk })
    }

    fn modifiers(&self) -> HOT_KEY_MODIFIERS {
        let mut m = MOD_NOREPEAT;
        if self.ctrl {
            m |= MOD_CONTROL;
        }
        if self.alt {
            m |= MOD_ALT;
        }
        if self.shift {
            m |= MOD_SHIFT;
        }
        m
    }
}

/// 虛擬鍵碼 → 顯示名稱（A–Z、0–9、F1–F12）。
fn key_name(vk: u32) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(String::from),
        0x70..=0x7B => Some(format!("F{}", vk - 0x6F)),
        _ => None,
    }
}

/// 名稱 → 虛擬鍵碼，[`key_name`] 的反向。
fn key_from_name(name: &str) -> Option<u32> {
    let up = name.to_ascii_uppercase();
    let mut chars = up.chars();
    match (chars.next()?, chars.next()) {
        (c @ ('A'..='Z' | '0'..='9'), None) => Some(c as u32),
        ('F', Some(_)) => {
            let n: u32 = up[1..].parse().ok()?;
            (1..=12).contains(&n).then_some(0x6F + n)
        }
        _ => None,
    }
}

const HOTKEY_ID: i32 = 1;

struct Worker {
    tid: u32,
    tx: mpsc::Sender<(Option<Hotkey>, mpsc::Sender<bool>)>,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

/// 啟動快捷鍵執行緒（只需呼叫一次）。按下快捷鍵時顯示主視窗並請 egui 重繪。
pub fn start(ctx: &egui::Context) {
    let ctx = ctx.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<u32>();
    let (tx, rx) = mpsc::channel::<(Option<Hotkey>, mpsc::Sender<bool>)>();
    std::thread::spawn(move || unsafe {
        let mut msg = MSG::default();
        // 先讓系統替這條執行緒建立訊息佇列，之後 PostThreadMessageW 才不會失敗
        let _ = PeekMessageW(&mut msg, None, WM_APP, WM_APP, PM_NOREMOVE);
        let _ = ready_tx.send(GetCurrentThreadId());
        let mut registered = false;
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            match msg.message {
                WM_HOTKEY => {
                    tray::show_main();
                    ctx.request_repaint();
                }
                WM_APP => {
                    while let Ok((hk, reply)) = rx.try_recv() {
                        if registered {
                            let _ = UnregisterHotKey(None, HOTKEY_ID);
                            registered = false;
                        }
                        let ok = match hk {
                            None => true,
                            Some(h) => {
                                registered = RegisterHotKey(None, HOTKEY_ID, h.modifiers(), h.vk).is_ok();
                                registered
                            }
                        };
                        let _ = reply.send(ok);
                    }
                }
                _ => {}
            }
        }
    });
    if let Ok(tid) = ready_rx.recv() {
        let _ = WORKER.set(Worker { tid, tx });
    }
}

/// 註冊新的快捷鍵（先取消舊的）；`None` 表示只取消。
/// 回傳 `false` 代表註冊失敗，通常是該組合已被其他程式佔用。
pub fn set(hk: Option<Hotkey>) -> bool {
    let Some(w) = WORKER.get() else { return false };
    let (reply_tx, reply_rx) = mpsc::channel();
    if w.tx.send((hk, reply_tx)).is_err() {
        return false;
    }
    unsafe {
        let _ = PostThreadMessageW(w.tid, WM_APP, WPARAM(0), LPARAM(0));
    }
    reply_rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_label_roundtrip() {
        assert_eq!(DEFAULT.label(), "Ctrl+Shift+M");
        assert_eq!(Hotkey::parse("Ctrl+Shift+M"), Some(DEFAULT));
        assert_eq!(Hotkey::parse(" ctrl + shift + m "), Some(DEFAULT));
    }

    #[test]
    fn parse_function_and_digit_keys() {
        let f5 = Hotkey::parse("Alt+F5").unwrap();
        assert_eq!((f5.alt, f5.vk), (true, 0x74));
        assert_eq!(f5.label(), "Alt+F5");
        assert_eq!(Hotkey::parse("Ctrl+0").unwrap().vk, 0x30);
    }

    #[test]
    fn parse_rejects_invalid() {
        assert_eq!(Hotkey::parse("M"), None); // 沒有修飾鍵
        assert_eq!(Hotkey::parse("Shift+M"), None); // 只有 Shift
        assert_eq!(Hotkey::parse("Ctrl+F13"), None);
        assert_eq!(Hotkey::parse("Ctrl+A+B"), None);
        assert_eq!(Hotkey::parse("Ctrl+"), None);
        assert_eq!(Hotkey::parse(""), None);
    }

    #[test]
    fn egui_keys_map_to_vk() {
        let ctrl = egui::Modifiers { ctrl: true, ..Default::default() };
        assert_eq!(Hotkey::from_egui(egui::Key::A, ctrl).unwrap().vk, 0x41);
        assert_eq!(Hotkey::from_egui(egui::Key::Z, ctrl).unwrap().vk, 0x5A);
        assert_eq!(Hotkey::from_egui(egui::Key::Num0, ctrl).unwrap().vk, 0x30);
        assert_eq!(Hotkey::from_egui(egui::Key::Num9, ctrl).unwrap().vk, 0x39);
        assert_eq!(Hotkey::from_egui(egui::Key::F1, ctrl).unwrap().vk, 0x70);
        assert_eq!(Hotkey::from_egui(egui::Key::F12, ctrl).unwrap().vk, 0x7B);
        assert_eq!(Hotkey::from_egui(egui::Key::Escape, ctrl), None);
        assert_eq!(Hotkey::from_egui(egui::Key::Space, ctrl), None);
    }
}
