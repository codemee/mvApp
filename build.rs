//! 建置時產生並嵌入 Windows 資源：
//!   - manifest（PerMonitorV2 DPI 感知）
//!   - 程式圖示（序號 2，多尺寸）
//!   - 版本資訊（顯示名稱與版本號）：工作列滑過提示、工作管理員、檔案內容「詳細資料」用的就是這裡。
//!     沒有版本資訊時，Windows 會退回顯示檔名（mvapp）。
//!
//! 版本號取自 Cargo.toml，升版時只需改那一處。

use std::path::PathBuf;
use std::{env, fs};

/// 顯示名稱。需與 src/i18n.rs 的 `APP_NAME` 一致。
const APP_NAME: &str = "mvApp-應用程式歸位器";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let num = |k: &str| env::var(k).ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let (major, minor, patch) = (num("CARGO_PKG_VERSION_MAJOR"), num("CARGO_PKG_VERSION_MINOR"), num("CARGO_PKG_VERSION_PATCH"));
    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");

    // rc.exe 的路徑字串用正斜線，避免跳脫字元問題
    let path = |rel: &str| manifest_dir.join(rel).to_string_lossy().replace('\\', "/");
    let rc = format!(
        r#"#pragma code_page(65001)

1 24 "{manifest}"
2 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040404B0"
        BEGIN
            VALUE "FileDescription", "{name} v{version}"
            VALUE "ProductName", "{name}"
            VALUE "FileVersion", "{version}"
            VALUE "ProductVersion", "{version}"
            VALUE "InternalName", "mvapp"
            VALUE "OriginalFilename", "mvapp.exe"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0404, 1200
    END
END
"#,
        manifest = path("app.manifest"),
        icon = path("assets/icon.ico"),
        name = APP_NAME,
    );

    // 以 UTF-8（含 BOM）寫入，rc.exe 才能正確讀取中文
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(rc.as_bytes());
    let rc_path = out_dir.join("app.rc");
    fs::write(&rc_path, bytes).expect("寫入 app.rc 失敗");

    // 資源編譯失敗就讓建置失敗，避免悄悄少了圖示或版本資訊
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_required()
        .expect("嵌入資源失敗");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
