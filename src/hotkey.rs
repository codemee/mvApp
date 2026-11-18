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
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
    UnregisterHotKey,
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

    /// 是否為常用的編輯／系統快捷鍵（只擋「單純 Ctrl＋鍵」與 Alt+F4 這類最容易誤設的組合）。
    /// 設成全域快捷鍵會讓其他程式裡的這些功能失效，所以不允許；帶 Shift 的變體不受限。
    pub fn is_common(&self) -> bool {
        if self.shift {
            return false;
        }
        match (self.ctrl, self.alt) {
            // Ctrl+字母：全選、複製、貼上、剪下、復原、取消復原、儲存、列印、尋找、開啟、新增、關閉、新分頁、重新整理
            (true, false) => match self.vk {
                0x41 | 0x43 | 0x56 | 0x58 | 0x5A | 0x59 | 0x53 | 0x50 | 0x46 | 0x4F | 0x4E | 0x57 | 0x54 | 0x52 => true,
                0x30..=0x39 => true, // Ctrl+數字：瀏覽器切換分頁、縮放
                0x73 | 0x74 => true, // Ctrl+F4 關閉分頁、Ctrl+F5 重新整理
                _ => false,
            },
            (false, true) => self.vk == 0x73, // Alt+F4 關閉視窗
            _ => false,
        }
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
        (hk.is_valid() && !hk.is_common()).then_some(hk)
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

/// 各虛擬鍵上一次偵測到的按下狀態（用來判斷「剛按下」）。
pub type KeyState = [bool; 256];

fn is_down(vk: u32) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
}

/// 支援作為主鍵的虛擬鍵碼：0–9、A–Z、F1–F12。
fn supported_vks() -> impl Iterator<Item = u32> {
    (0x30..=0x39).chain(0x41..=0x5A).chain(0x70..=0x7B)
}

/// 目前的按鍵狀態快照；開始錄製時先拍一張，避免把「已經按著」的鍵當成新按下。
pub fn snapshot() -> KeyState {
    let mut s = [false; 256];
    for vk in supported_vks() {
        s[vk as usize] = is_down(vk);
    }
    s
}

/// 直接向系統查詢實際按鍵，回傳這次「剛按下」的組合（含當下的 Ctrl／Alt／Shift）。
///
/// 錄製快捷鍵不能只靠 egui 的按鍵事件：egui-winit 會把 Ctrl+X/C/V 轉成剪貼簿事件，
/// 且剪貼簿沒有可貼上的內容時連事件都不會送，這幾組就完全收不到。
pub fn poll_pressed(prev: &mut KeyState) -> Option<Hotkey> {
    let mut found = None;
    for vk in supported_vks() {
        let down = is_down(vk);
        if down && !prev[vk as usize] && found.is_none() {
            found = Some(Hotkey { ctrl: is_down(0x11), alt: is_down(0x12), shift: is_down(0x10), vk });
        }
        prev[vk as usize] = down;
    }
    found
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
        assert_eq!(Hotkey::parse("Alt+0").unwrap().vk, 0x30);
    }

    #[test]
    fn common_shortcuts_are_rejected() {
        for s in ["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+A", "Ctrl+S", "Ctrl+1", "Ctrl+F4", "Alt+F4"] {
            assert_eq!(Hotkey::parse(s), None, "{s} 應被拒絕");
        }
        // 帶 Shift 的變體、或其他組合不受限
        for s in ["Ctrl+Shift+M", "Ctrl+Shift+C", "Ctrl+Alt+C", "Alt+C", "Ctrl+M", "Ctrl+F6"] {
            assert!(Hotkey::parse(s).is_some(), "{s} 應被接受");
        }
        assert!(Hotkey::parse("Ctrl+C").is_none());
        assert!(!DEFAULT.is_common());
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
