//! Typeface choices for text annotations: a short curated menu of fonts that
//! ship with Windows, plus the system font dialog for anything else.

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::core::*;

use crate::gfx::gfx;
use crate::settings::FontSpec;

/// Offered in the font menu when installed.
const CURATED: [&str; 10] = [
    "Segoe UI",
    "Arial",
    "Calibri",
    "Verdana",
    "Georgia",
    "Times New Roman",
    "Consolas",
    "Comic Sans MS",
    "Impact",
    "Malgun Gothic",
];

pub fn is_installed(family: &str) -> bool {
    unsafe {
        let mut fc = None;
        if gfx().dw.GetSystemFontCollection(&mut fc, false).is_err() {
            return false;
        }
        let Some(fc) = fc else { return false };
        let (mut index, mut exists) = (0u32, BOOL(0));
        fc.FindFamilyName(&HSTRING::from(family), &mut index, &mut exists).is_ok() && exists.as_bool()
    }
}

/// Curated families present on this machine.
pub fn menu_families() -> Vec<&'static str> {
    CURATED.into_iter().filter(|f| is_installed(f)).collect()
}

/// The system font dialog, mapped back to a DirectWrite family. `None` if cancelled.
pub fn choose(owner: HWND, current: &FontSpec) -> Option<FontSpec> {
    let mut lf = LOGFONTW {
        lfWeight: if current.bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
        lfItalic: current.italic as u8,
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };
    let name: Vec<u16> = current.family.encode_utf16().take(lf.lfFaceName.len() - 1).collect();
    lf.lfFaceName[..name.len()].copy_from_slice(&name);
    let mut cf = CHOOSEFONTW {
        lStructSize: std::mem::size_of::<CHOOSEFONTW>() as u32,
        hwndOwner: owner,
        lpLogFont: &mut lf,
        Flags: CF_SCREENFONTS | CF_INITTOLOGFONTSTRUCT | CF_NOSIZESEL | CF_NOVERTFONTS | CF_FORCEFONTEXIST,
        ..Default::default()
    };
    if !unsafe { ChooseFontW(&mut cf) }.as_bool() {
        return None;
    }
    let bold = lf.lfWeight >= FW_SEMIBOLD.0 as i32;
    let italic = lf.lfItalic != 0;
    // GDI face names ("Segoe UI Semibold") are not always DirectWrite family
    // names; ask DirectWrite which family the chosen face belongs to.
    let family = dwrite_family(&lf).unwrap_or_else(|| {
        let len = lf.lfFaceName.iter().position(|c| *c == 0).unwrap_or(lf.lfFaceName.len());
        String::from_utf16_lossy(&lf.lfFaceName[..len])
    });
    Some(FontSpec { family, bold, italic })
}

fn dwrite_family(lf: &LOGFONTW) -> Option<String> {
    unsafe {
        let font = gfx().dw.GetGdiInterop().ok()?.CreateFontFromLOGFONT(lf).ok()?;
        let names = font.GetFontFamily().ok()?.GetFamilyNames().ok()?;
        let (mut index, mut exists) = (0u32, BOOL(0));
        let _ = names.FindLocaleName(w!("en-us"), &mut index, &mut exists);
        if !exists.as_bool() {
            index = 0;
        }
        let len = names.GetStringLength(index).ok()? as usize;
        let mut buf = vec![0u16; len + 1];
        names.GetString(index, &mut buf).ok()?;
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}
