#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod i18n;
mod icons;
mod monitors;
mod mover;
mod settings;
mod single_instance;
mod toolbar;
mod tray;
mod windows_list;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// 載入系統中文字型，避免視窗標題顯示成方塊。
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, path) in [
        ("msjh", r"C:\Windows\Fonts\msjh.ttc"),
        ("msyh", r"C:\Windows\Fonts\msyh.ttc"),
        ("meiryo", r"C:\Windows\Fonts\meiryo.ttc"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(name.to_owned());
            }
        }
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod smoke {
    #[test]
    fn monitors_list() {
        let m = crate::monitors::list();
        println!("monitors: {m:?}");
        assert!(!m.is_empty());
    }

    #[test]
    fn windows_list() {
        let w = crate::windows_list::list();
        println!("windows: {}", w.len());
        for x in &w {
            println!("{:?}", x);
        }
    }

    #[test]
    fn icons_load() {
        for w in crate::windows_list::list() {
            let ok = crate::icons::probe(w.hwnd, &w.exe_path);
            println!("{} -> {ok}", w.title);
        }
    }
}

fn main() -> eframe::Result {
    // 只允許執行一份：已有實例在執行時，通知它把主視窗叫出來，自己直接結束
    let guard = match single_instance::acquire() {
        single_instance::Acquire::First(g) => g,
        single_instance::Acquire::AlreadyRunning => {
            single_instance::notify_existing();
            return Ok(());
        }
    };
    // Intel 的 Vulkan 驅動（igvk64.dll）在部分機器上會崩潰，預設改用 DX12；
    // 仍可由使用者自行設定 WGPU_BACKEND 覆寫。
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: 在任何其他執行緒啟動之前、於 main 開頭設定。
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    let icon = eframe::icon_data::from_png_bytes(ICON_PNG).expect("icon.png 格式錯誤");
    // 初始標題依使用者的語言設定決定，避免啟動瞬間出現錯誤語言的標題
    let lang = settings::Settings::load().lang.resolve(i18n::system_lang());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            .with_title(i18n::Tr(lang).app_title())
            .with_inner_size([900.0, 520.0])
            .with_min_inner_size([400.0, 300.0])
            // 啟動時只顯示在系統匣，雙按系統匣圖示才顯示主視窗
            .with_visible(false),
        ..Default::default()
    };
    eframe::run_native(
        "mvapp",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            // 之後若有人再啟動第二份，由這條執行緒收到通知並顯示主視窗
            guard.spawn_listener(&cc.egui_ctx);
            // 記下主視窗 HWND，供系統匣事件直接顯示／隱藏視窗
            if let Ok(RawWindowHandle::Win32(h)) = cc.window_handle().map(|h| h.as_raw()) {
                tray::set_main_hwnd(h.hwnd.get());
                tray::make_invisible_for_startup();
            }
            Ok(Box::new(app::App::new(&cc.egui_ctx, ICON_PNG)))
        }),
    )
}
