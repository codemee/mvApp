//! 取得視窗圖示並轉成 egui texture（含快取）。

use std::collections::HashMap;
use std::ffi::c_void;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GCLP_HICON, GetClassLongPtrW, GetIconInfo, HICON, ICON_BIG, ICON_SMALL2, ICONINFO,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETICON,
};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::{GUID, PCWSTR};

use crate::windows_list::hwnd_of;

type Rgba = (usize, usize, Vec<u8>);

/// 把 HICON 轉成 RGBA 像素。
fn hicon_to_rgba(hicon: HICON) -> Option<Rgba> {
    unsafe {
        let mut ii = ICONINFO::default();
        GetIconInfo(hicon, &mut ii).ok()?;
        let result = (|| {
            if ii.hbmColor.is_invalid() {
                return None; // 單色圖示，略過
            }
            let mut bm = BITMAP::default();
            let got = GetObjectW(
                ii.hbmColor.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut _ as *mut c_void),
            );
            if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
                return None;
            }
            let (w, h) = (bm.bmWidth as usize, bm.bmHeight as usize);
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let hdc = GetDC(None);
            let mut color = vec![0u8; w * h * 4];
            let lines = GetDIBits(
                hdc,
                ii.hbmColor,
                0,
                h as u32,
                Some(color.as_mut_ptr() as *mut c_void),
                &mut bmi,
                DIB_RGB_COLORS,
            );
            // 沒有 alpha 的舊式圖示：改用遮罩
            let has_alpha = color.chunks_exact(4).any(|p| p[3] != 0);
            let mut mask = vec![0u8; w * h * 4];
            let mask_ok = !has_alpha
                && !ii.hbmMask.is_invalid()
                && GetDIBits(
                    hdc,
                    ii.hbmMask,
                    0,
                    h as u32,
                    Some(mask.as_mut_ptr() as *mut c_void),
                    &mut bmi,
                    DIB_RGB_COLORS,
                ) != 0;
            ReleaseDC(None, hdc);
            if lines == 0 {
                return None;
            }
            for (i, p) in color.chunks_exact_mut(4).enumerate() {
                p.swap(0, 2); // BGRA -> RGBA
                if !has_alpha {
                    p[3] = if mask_ok && mask[i * 4] != 0 { 0 } else { 255 };
                }
            }
            Some((w, h, color))
        })();
        let _ = DeleteObject(ii.hbmColor.into());
        let _ = DeleteObject(ii.hbmMask.into());
        result
    }
}

fn icon_from_window(hwnd: isize) -> Option<Rgba> {
    let h = hwnd_of(hwnd);
    for kind in [ICON_BIG, ICON_SMALL2] {
        let mut res = 0usize;
        let ok = unsafe {
            SendMessageTimeoutW(
                h,
                WM_GETICON,
                WPARAM(kind as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                100,
                Some(&mut res),
            )
        };
        if ok.0 != 0 && res != 0 {
            // WM_GETICON 取得的圖示由視窗擁有，不可 DestroyIcon
            if let Some(i) = hicon_to_rgba(HICON(res as *mut c_void)) {
                return Some(i);
            }
        }
    }
    let c = unsafe { GetClassLongPtrW(h, GCLP_HICON) };
    if c != 0 {
        return hicon_to_rgba(HICON(c as *mut c_void));
    }
    None
}

fn icon_from_exe(exe: &str) -> Option<Rgba> {
    if exe.is_empty() {
        return None;
    }
    let wide: Vec<u16> = exe.encode_utf16().chain(Some(0)).collect();
    let mut sfi = SHFILEINFOW::default();
    unsafe {
        let r = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if r == 0 || sfi.hIcon.is_invalid() {
            return None;
        }
        let out = hicon_to_rgba(sfi.hIcon);
        let _ = DestroyIcon(sfi.hIcon);
        out
    }
}

/// 取得視窗的 AppUserModelID（UWP / 封裝應用程式才有意義）。
fn aumid_of(hwnd: isize) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd_of(hwnd)).ok()?;
        // PKEY_AppUserModel_ID：{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, 5
        let key = PROPERTYKEY {
            fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
            pid: 5,
        };
        let mut pv = store.GetValue(&key).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok();
        let out = s.as_ref().and_then(|p| p.to_string().ok());
        if let Some(p) = s {
            CoTaskMemFree(Some(p.0 as *const c_void));
        }
        let _ = PropVariantClear(&mut pv);
        out.filter(|s| !s.is_empty())
    }
}

/// 把 shell 回傳的 HBITMAP（32bpp、預乘 alpha）轉成 RGBA 並釋放 bitmap。
fn hbitmap_to_rgba(hbm: HBITMAP) -> Option<Rgba> {
    unsafe {
        let mut bm = BITMAP::default();
        let got = GetObjectW(
            hbm.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as *mut c_void),
        );
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(hbm.into());
            return None;
        }
        let (w, h) = (bm.bmWidth as usize, bm.bmHeight as usize);
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let hdc = GetDC(None);
        let mut px = vec![0u8; w * h * 4];
        let lines = GetDIBits(hdc, hbm, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(None, hdc);
        let _ = DeleteObject(hbm.into());
        if lines == 0 {
            return None;
        }
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                // 預乘 -> 非預乘
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 / a).min(255)) as u8;
                }
            }
        }
        Some((w, h, px))
    }
}

/// 透過 AppUserModelID 向 shell 取得封裝應用程式（如「設定」）的真正圖示。
fn icon_from_aumid(hwnd: isize) -> Option<Rgba> {
    let id = aumid_of(hwnd)?;
    let path: Vec<u16> = format!("shell:AppsFolder\\{id}").encode_utf16().chain(Some(0)).collect();
    unsafe {
        // 通常 winit 已初始化 COM；重複呼叫無害
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None).ok()?;
        let hbm = factory.GetImage(SIZE { cx: 64, cy: 64 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        hbitmap_to_rgba(hbm)
    }
}

/// 依序嘗試：封裝應用程式圖示（僅 ApplicationFrameHost 代管的視窗）→ 視窗圖示 → exe 圖示。
fn fetch_icon(hwnd: isize, exe: &str) -> Option<Rgba> {
    let hosted = exe.to_ascii_lowercase().ends_with("applicationframehost.exe");
    if hosted && let Some(i) = icon_from_aumid(hwnd) {
        return Some(i);
    }
    icon_from_window(hwnd).or_else(|| icon_from_exe(exe))
}

/// 測試用：不建立 texture，僅確認能取得圖示像素。
#[cfg(test)]
pub fn probe(hwnd: isize, exe: &str) -> bool {
    fetch_icon(hwnd, exe).is_some()
}

/// 以 HWND 為 key 的 texture 快取。
#[derive(Default)]
pub struct IconCache {
    map: HashMap<isize, Option<TextureHandle>>,
}

impl IconCache {
    pub fn get(&mut self, ctx: &egui::Context, hwnd: isize, exe: &str) -> Option<TextureHandle> {
        self.map
            .entry(hwnd)
            .or_insert_with(|| {
                let (w, h, px) = fetch_icon(hwnd, exe)?;
                let img = ColorImage::from_rgba_unmultiplied([w, h], &px);
                Some(ctx.load_texture(format!("icon-{hwnd}"), img, TextureOptions::LINEAR))
            })
            .clone()
    }

    /// 移除已不存在視窗的快取。
    pub fn retain(&mut self, alive: impl Fn(isize) -> bool) {
        self.map.retain(|k, _| alive(*k));
    }
}
