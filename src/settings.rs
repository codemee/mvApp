//! 使用者設定（配色、語言），存於 %APPDATA%\mvapp\settings.txt。

use std::path::PathBuf;

use eframe::egui::ThemePreference;

use crate::i18n::LangPref;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Settings {
    pub theme: ThemePreference,
    pub lang: LangPref,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: ThemePreference::System, lang: LangPref::System }
    }
}

fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("mvapp").join("settings.txt"))
}

fn theme_str(t: ThemePreference) -> &'static str {
    match t {
        ThemePreference::System => "system",
        ThemePreference::Light => "light",
        ThemePreference::Dark => "dark",
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return s;
        };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("theme", "light")) => s.theme = ThemePreference::Light,
                Some(("theme", "dark")) => s.theme = ThemePreference::Dark,
                Some(("theme", "system")) => s.theme = ThemePreference::System,
                Some(("lang", v)) => {
                    if let Some(l) = LangPref::parse(v.trim()) {
                        s.lang = l;
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// 儲存失敗時靜默略過（設定非關鍵功能）。
    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, format!("theme={}\nlang={}\n", theme_str(self.theme), self.lang.as_str()));
    }
}
