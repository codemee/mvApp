//! 介面語系（繁體中文 / English）與系統語系偵測。

use windows::Win32::Globalization::GetUserDefaultLocaleName;

use crate::mover::MoveError;

/// 應用程式顯示名稱。需與 build.rs 的 `APP_NAME` 一致（後者寫入 exe 的版本資訊）。
pub const APP_NAME: &str = "mvApp-應用程式歸位器";
/// 版本號，取自 Cargo.toml。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 實際使用的語言。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
}

/// 使用者的語言偏好（可為「隨系統」）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LangPref {
    System,
    Zh,
    En,
}

impl LangPref {
    pub fn resolve(self, system: Lang) -> Lang {
        match self {
            LangPref::System => system,
            LangPref::Zh => Lang::Zh,
            LangPref::En => Lang::En,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LangPref::System => "system",
            LangPref::Zh => "zh",
            LangPref::En => "en",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(LangPref::System),
            "zh" => Some(LangPref::Zh),
            "en" => Some(LangPref::En),
            _ => None,
        }
    }
}

/// 偵測系統語言：zh 開頭視為繁體中文介面，其餘用英文。
pub fn system_lang() -> Lang {
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 1 {
        let name = String::from_utf16_lossy(&buf[..(n as usize - 1)]);
        if name.to_ascii_lowercase().starts_with("zh") {
            return Lang::Zh;
        }
    }
    Lang::En
}

/// 翻譯表。
#[derive(Clone, Copy)]
pub struct Tr(pub Lang);

impl Tr {
    fn pick(self, zh: &'static str, en: &'static str) -> &'static str {
        match self.0 {
            Lang::Zh => zh,
            Lang::En => en,
        }
    }

    /// 應用程式顯示名稱＋版本號（視窗標題、系統匣提示共用）。
    /// 版本號取自 Cargo.toml，升版時只需改那一處。
    pub fn app_title(self) -> String {
        match self.0 {
            Lang::Zh => format!("{APP_NAME} v{VERSION}"),
            Lang::En => format!("mvApp - Move windows between monitors v{VERSION}"),
        }
    }
    pub fn hint(self) -> &'static str {
        self.pick("把圖示拖放到其他螢幕的窗格即可搬移視窗。", "Drag an icon onto another monitor's pane to move that window.")
    }
    pub fn refresh(self) -> &'static str {
        self.pick("重新整理", "Refresh")
    }
    pub fn no_monitor(self) -> &'static str {
        self.pick("找不到任何螢幕。", "No monitors found.")
    }
    pub fn minimized(self) -> &'static str {
        self.pick("（已最小化）", " (minimized)")
    }

    pub fn monitor_title(self, n: usize, primary: bool, w: i32, h: i32) -> String {
        match self.0 {
            Lang::Zh => format!("螢幕 {n}{}　{w}×{h}", if primary { "（主要）" } else { "" }),
            Lang::En => format!("Monitor {n}{}  {w}×{h}", if primary { " (Primary)" } else { "" }),
        }
    }
    pub fn monitor_tip(self, name: &str, dpi: u32) -> String {
        let scale = dpi * 100 / 96;
        match self.0 {
            Lang::Zh => format!("{name}\nDPI {dpi}（縮放 {scale}%）"),
            Lang::En => format!("{name}\nDPI {dpi} (scale {scale}%)"),
        }
    }

    pub fn moved(self, title: &str, to: usize) -> String {
        match self.0 {
            Lang::Zh => format!("已將「{title}」搬到螢幕 {to}"),
            Lang::En => format!("Moved \"{title}\" to monitor {to}"),
        }
    }
    pub fn move_failed(self, e: &MoveError) -> String {
        let detail = match e {
            MoveError::NoSourceMonitor => self.pick("找不到視窗目前所在的螢幕", "Cannot find the monitor the window is on").to_string(),
            MoveError::ReadRect => self.pick("讀取視窗位置失敗", "Failed to read the window position").to_string(),
            MoveError::Denied(s) => match self.0 {
                Lang::Zh => format!("無法搬移視窗（可能是以系統管理員身分執行的程式）：{s}"),
                Lang::En => format!("Cannot move the window (it may be running as administrator): {s}"),
            },
        };
        match self.0 {
            Lang::Zh => format!("搬移失敗：{detail}"),
            Lang::En => format!("Move failed: {detail}"),
        }
    }

    pub fn tray_quit(self) -> &'static str {
        self.pick("結束", "Exit")
    }
    pub fn tray_autostart(self) -> &'static str {
        self.pick("開機自動執行", "Run at startup")
    }
    /// 附在三態按鈕提示後面，說明點一下會切換到下一個狀態。
    pub fn cycle_hint(self) -> &'static str {
        self.pick("（點一下切換）", " (click to switch)")
    }

    // 工具列提示
    pub fn theme_system(self) -> &'static str {
        self.pick("配色：隨系統", "Theme: follow system")
    }
    pub fn theme_light(self) -> &'static str {
        self.pick("配色：淺色", "Theme: light")
    }
    pub fn theme_dark(self) -> &'static str {
        self.pick("配色：深色", "Theme: dark")
    }
    pub fn lang_system(self) -> &'static str {
        self.pick("語言：隨系統", "Language: follow system")
    }
    pub fn lang_zh(self) -> &'static str {
        self.pick("語言：繁體中文", "Language: Traditional Chinese")
    }
    pub fn lang_en(self) -> &'static str {
        self.pick("語言：English", "Language: English")
    }
}
