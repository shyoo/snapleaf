//! Image export (clipboard DIB + PNG, PNG files) and text clipboard access.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::Shell::*;
use windows::core::*;

use crate::gfx::{Image, encode_png};
use crate::settings;

const CF_DIB: u32 = 8;

/// Put the image on the clipboard as a 24-bit DIB (what every Windows app
/// reads) and as PNG (what browsers and Electron apps prefer).
pub fn copy_image(owner: HWND, img: &Image) -> Result<()> {
    let dib = dib_bytes(img);
    let png = encode_png(img).ok();
    unsafe {
        open_clipboard(owner)?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            set_data(CF_DIB, &dib)?;
            if let Some(png) = &png {
                let fmt = RegisterClipboardFormatW(w!("PNG"));
                if fmt != 0 {
                    set_data(fmt, png)?;
                }
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

fn open_clipboard(owner: HWND) -> Result<()> {
    let mut opened = unsafe { OpenClipboard(Some(owner)) };
    for _ in 0..10 {
        if opened.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        opened = unsafe { OpenClipboard(Some(owner)) };
    }
    opened
}

/// Copy a text selection using Windows' Unicode clipboard format.
pub fn copy_text(owner: HWND, text: &str) -> Result<()> {
    let utf16: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().chain(Some(0)).collect();
    let bytes: Vec<u8> = utf16.iter().flat_map(|u| u.to_ne_bytes()).collect();
    unsafe {
        open_clipboard(owner)?;
        let result = (|| {
            EmptyClipboard()?;
            set_data(13, &bytes)
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Plain text on the clipboard, if any (for pasting into a text annotation).
pub fn paste_text(owner: HWND) -> Option<String> {
    const CF_UNICODETEXT: u32 = 13;
    unsafe {
        open_clipboard(owner).ok()?;
        let text = (|| {
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            let hg = HGLOBAL(h.0);
            let p = GlobalLock(hg) as *const u16;
            if p.is_null() {
                return None;
            }
            let max = GlobalSize(hg) / 2;
            let len = (0..max).take_while(|i| *p.add(*i) != 0).count();
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            let _ = GlobalUnlock(hg);
            Some(s.replace("\r\n", "\n"))
        })();
        let _ = CloseClipboard();
        text
    }
}

unsafe fn set_data(fmt: u32, bytes: &[u8]) -> Result<()> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(h);
        if let Err(e) = SetClipboardData(fmt, Some(HANDLE(h.0))) {
            let _ = GlobalFree(Some(h));
            return Err(e);
        }
        Ok(())
    }
}

/// Packed bottom-up 24-bit DIB: BITMAPINFOHEADER followed by the pixels.
pub fn dib_bytes(img: &Image) -> Vec<u8> {
    let (w, h) = (img.w as usize, img.h as usize);
    let stride = (w * 3 + 3) & !3;
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: h as i32,
        biPlanes: 1,
        biBitCount: 24,
        biCompression: BI_RGB.0,
        biSizeImage: (stride * h) as u32,
        ..Default::default()
    };
    let hsize = std::mem::size_of::<BITMAPINFOHEADER>();
    let mut out = vec![0u8; hsize + stride * h];
    unsafe {
        std::ptr::copy_nonoverlapping(&header as *const _ as *const u8, out.as_mut_ptr(), hsize);
    }
    for y in 0..h {
        let src = &img.px[(h - 1 - y) * w * 4..(h - y) * w * 4];
        let dst = &mut out[hsize + y * stride..hsize + y * stride + w * 3];
        for (d, s) in dst.as_chunks_mut::<3>().0.iter_mut().zip(src.chunks_exact(4)) {
            d.copy_from_slice(&s[..3]);
        }
    }
    out
}

fn pictures_dir() -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Pictures, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn default_dir() -> PathBuf {
    let saved = settings::get().save_dir;
    if !saved.is_empty() && Path::new(&saved).is_dir() {
        return PathBuf::from(saved);
    }
    let pics = pictures_dir().unwrap_or_else(|| PathBuf::from("."));
    let shots = pics.join("Screenshots");
    if shots.is_dir() { shots } else { pics }
}

fn default_name() -> String {
    let t = unsafe { GetLocalTime() };
    format!(
        "Snip {:04}-{:02}-{:02} {:02}{:02}{:02}.png",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// Ask where to save; `None` if the user cancelled.
pub fn ask_save_path(owner: HWND) -> Option<PathBuf> {
    let mut file: Vec<u16> = default_name().encode_utf16().collect();
    file.resize(1024, 0);
    let dir = HSTRING::from(default_dir().as_os_str());
    let filter: Vec<u16> = "PNG image (*.png)\0*.png\0\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        nFilterIndex: 1,
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrInitialDir: PCWSTR(dir.as_ptr()),
        lpstrDefExt: w!("png"),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER,
        ..Default::default()
    };
    if !unsafe { GetSaveFileNameW(&mut ofn) }.as_bool() {
        return None;
    }
    let len = file.iter().position(|c| *c == 0).unwrap_or(file.len());
    let path = PathBuf::from(String::from_utf16_lossy(&file[..len]));
    if let Some(parent) = path.parent() {
        let dir = parent.to_string_lossy().into_owned();
        settings::update(|s| s.save_dir = dir);
    }
    Some(path)
}

pub fn save_png(path: &Path, img: &Image) -> std::result::Result<(), String> {
    let bytes = encode_png(img).map_err(|e| e.message().to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_is_bottom_up_and_padded() {
        // 3x2 image: top row red, bottom row blue (BGRA).
        let mut px = Vec::new();
        for _ in 0..3 {
            px.extend_from_slice(&[0, 0, 255, 255]);
        }
        for _ in 0..3 {
            px.extend_from_slice(&[255, 0, 0, 255]);
        }
        let d = dib_bytes(&Image { w: 3, h: 2, px });
        let hs = 40;
        let stride = 12; // 9 bytes padded to 12
        assert_eq!(d.len(), hs + stride * 2);
        assert_eq!(&d[hs..hs + 3], &[255, 0, 0]); // first stored row is the bottom (blue)
        assert_eq!(&d[hs + stride..hs + stride + 3], &[0, 0, 255]);
    }
}
