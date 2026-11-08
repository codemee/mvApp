//! 開機自動執行：在 HKCU\Software\Microsoft\Windows\CurrentVersion\Run 新增／移除一個值。
//! 只寫入目前使用者的機碼，不需要系統管理員權限。

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: PCWSTR = w!("mvApp");

/// 目前執行檔的完整路徑，加上引號（路徑含空白時 Run 值才會正確解析）。
fn command_line() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

/// 讀取 Run 值目前的內容；沒有設定時回傳 None。
fn current_value() -> Option<String> {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key) != ERROR_SUCCESS {
            return None;
        }
        let mut len = 0u32;
        let mut out = None;
        // 先問長度，再讀內容
        if RegQueryValueExW(key, VALUE_NAME, None, None, None, Some(&mut len)) == ERROR_SUCCESS && len > 0 {
            let mut buf = vec![0u16; (len as usize).div_ceil(2)];
            let mut len2 = (buf.len() * 2) as u32;
            if RegQueryValueExW(key, VALUE_NAME, None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len2))
                == ERROR_SUCCESS
            {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                out = Some(String::from_utf16_lossy(&buf[..n]));
            }
        }
        let _ = RegCloseKey(key);
        out
    }
}

/// 是否已設定開機自動執行。
pub fn is_enabled() -> bool {
    current_value().is_some()
}

/// 開啟或關閉開機自動執行。失敗時回傳錯誤碼。
pub fn set_enabled(enable: bool) -> Result<(), u32> {
    unsafe {
        let mut key = HKEY::default();
        let rc = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key);
        if rc != ERROR_SUCCESS {
            return Err(rc.0);
        }
        let rc = if enable {
            let Some(cmd) = command_line() else {
                let _ = RegCloseKey(key);
                return Err(1);
            };
            let wide: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(key, VALUE_NAME, None, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, VALUE_NAME)
        };
        let _ = RegCloseKey(key);
        // 關閉時若本來就沒有這個值（ERROR_FILE_NOT_FOUND = 2）視為成功
        if rc == ERROR_SUCCESS || (!enable && rc.0 == 2) { Ok(()) } else { Err(rc.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 實際寫入再刪除 HKCU 的 Run 值；若使用者原本已啟用則略過，避免動到真實設定。
    #[test]
    fn toggle_roundtrip() {
        if is_enabled() {
            eprintln!("使用者已啟用開機自動執行，略過此測試");
            return;
        }
        set_enabled(true).expect("寫入失敗");
        assert!(is_enabled());
        let v = current_value().expect("應該讀得到值");
        assert!(v.starts_with('"') && v.ends_with('"'), "路徑應加上引號：{v}");
        assert!(v.to_ascii_lowercase().contains(".exe"));
        set_enabled(false).expect("刪除失敗");
        assert!(!is_enabled());
        // 重複關閉不應報錯
        set_enabled(false).expect("重複關閉應視為成功");
    }
}

/// 若已啟用但路徑與目前執行檔不同（例如程式被搬動過），把路徑更新成目前位置。
pub fn refresh_path_if_stale() {
    if let (Some(cur), Some(cmd)) = (current_value(), command_line())
        && cur != cmd
    {
        let _ = set_enabled(true);
    }
}
