//! Persisted preferences: `%APPDATA%\Snapleaf\settings.ini`, plain `key=value`.

use std::cell::RefCell;
use std::path::PathBuf;

use crate::gfx::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Region,
    Window,
    Full,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Region, Mode::Window, Mode::Full];
    pub fn label(self) -> &'static str {
        match self {
            Mode::Region => "Rectangle",
            Mode::Window => "Window",
            Mode::Full => "Full screen",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Mode::Region => "region",
            Mode::Window => "window",
            Mode::Full => "full",
        }
    }
    fn parse(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.key() == s)
    }
}

pub const DELAYS: [u32; 4] = [0, 3, 5, 10];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    CtrlPrintScreen,
    PrintScreen,
    CtrlAltS,
    CtrlShiftS,
    Off,
}

impl Hotkey {
    pub const ALL: [Hotkey; 5] =
        [Hotkey::CtrlPrintScreen, Hotkey::PrintScreen, Hotkey::CtrlAltS, Hotkey::CtrlShiftS, Hotkey::Off];
    pub fn label(self) -> &'static str {
        match self {
            Hotkey::CtrlPrintScreen => "Ctrl+Print Screen",
            Hotkey::PrintScreen => "Print Screen",
            Hotkey::CtrlAltS => "Ctrl+Alt+S",
            Hotkey::CtrlShiftS => "Ctrl+Shift+S",
            Hotkey::Off => "None",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Hotkey::CtrlPrintScreen => "ctrl+prtsc",
            Hotkey::PrintScreen => "prtsc",
            Hotkey::CtrlAltS => "ctrl+alt+s",
            Hotkey::CtrlShiftS => "ctrl+shift+s",
            Hotkey::Off => "off",
        }
    }
    fn parse(s: &str) -> Option<Hotkey> {
        Hotkey::ALL.into_iter().find(|h| h.key() == s)
    }
}

/// Typeface of a text annotation. `family` is a DirectWrite family name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontSpec {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

impl Default for FontSpec {
    fn default() -> Self {
        FontSpec { family: "Segoe UI".into(), bold: true, italic: false }
    }
}

/// Colour and size index for each styled tool, in `doc::Tool::slot` order:
/// rectangle, arrow, text, highlighter, pen.
pub const STYLE_SLOTS: usize = 5;
const SLOT_KEYS: [&str; STYLE_SLOTS] = ["rect", "arrow", "text", "highlight", "pen"];

#[derive(Clone, Debug)]
pub struct Settings {
    pub mode: Mode,
    pub delay: u32,
    pub hotkey: Hotkey,
    pub colors: [Color; STYLE_SLOTS],
    pub sizes: [usize; STYLE_SLOTS],
    pub custom_color: Color,
    pub save_dir: String,
    /// Font for new text annotations.
    pub font: FontSpec,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: Mode::Region,
            delay: 0,
            hotkey: Hotkey::CtrlPrintScreen,
            colors: [Color(0xE53935), Color(0xE53935), Color(0xE53935), Color(0xFDD835), Color(0xE53935)],
            sizes: [1, 1, 1, 1, 1],
            custom_color: Color(0x00897B),
            save_dir: String::new(),
            font: FontSpec::default(),
        }
    }
}

thread_local! {
    static SETTINGS: RefCell<Settings> = RefCell::new(load());
}

pub fn get() -> Settings {
    SETTINGS.with(|s| s.borrow().clone())
}

/// Mutate and persist.
pub fn update(f: impl FnOnce(&mut Settings)) {
    let snapshot = SETTINGS.with(|s| {
        let mut s = s.borrow_mut();
        f(&mut s);
        s.clone()
    });
    save(&snapshot);
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("Snapleaf").join("settings.ini"))
}

fn load() -> Settings {
    // Carry existing preferences forward on the first launch under the new name.
    // Saves always go to Snapleaf; the old file stays untouched.
    let text = path().and_then(|p| {
        std::fs::read_to_string(&p).ok().or_else(|| {
            let legacy = p.parent()?.parent()?.join("Snipper").join("settings.ini");
            std::fs::read_to_string(legacy).ok()
        })
    }).unwrap_or_default();
    parse(&text)
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "mode" => s.mode = Mode::parse(v).unwrap_or(s.mode),
            "delay" => s.delay = v.parse().ok().filter(|d| DELAYS.contains(d)).unwrap_or(0),
            "hotkey" => s.hotkey = Hotkey::parse(v).unwrap_or(s.hotkey),
            "custom_color" => s.custom_color = parse_color(v).unwrap_or(s.custom_color),
            "save_dir" => s.save_dir = v.to_string(),
            "text.font" if !v.is_empty() => s.font.family = v.to_string(),
            "text.bold" => s.font.bold = v == "1",
            "text.italic" => s.font.italic = v == "1",
            _ => {
                for (i, slot) in SLOT_KEYS.iter().enumerate() {
                    if k == format!("{slot}.color") {
                        s.colors[i] = parse_color(v).unwrap_or(s.colors[i]);
                    } else if k == format!("{slot}.size") {
                        s.sizes[i] = v.parse().ok().filter(|n| *n < 3).unwrap_or(s.sizes[i]);
                    }
                }
            }
        }
    }
    s
}

pub fn serialize(s: &Settings) -> String {
    let mut out = format!(
        "mode={}\ndelay={}\nhotkey={}\ncustom_color={:06X}\nsave_dir={}\n",
        s.mode.key(),
        s.delay,
        s.hotkey.key(),
        s.custom_color.0,
        s.save_dir
    );
    for (i, slot) in SLOT_KEYS.iter().enumerate() {
        out += &format!("{slot}.color={:06X}\n{slot}.size={}\n", s.colors[i].0, s.sizes[i]);
    }
    out += &format!("text.font={}\ntext.bold={}\ntext.italic={}\n", s.font.family, s.font.bold as u8, s.font.italic as u8);
    out
}

fn parse_color(v: &str) -> Option<Color> {
    u32::from_str_radix(v.trim_start_matches('#'), 16).ok().filter(|c| *c <= 0xFFFFFF).map(Color)
}

fn save(s: &Settings) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, serialize(s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut s = Settings::default();
        s.mode = Mode::Window;
        s.delay = 5;
        s.hotkey = Hotkey::Off;
        s.colors[3] = Color(0x123456);
        s.sizes[0] = 2;
        s.font = FontSpec { family: "Malgun Gothic".into(), bold: false, italic: true };
        s.save_dir = r"C:\Users\me\Pictures".into();
        let back = parse(&serialize(&s));
        assert_eq!(back.mode, Mode::Window);
        assert_eq!(back.delay, 5);
        assert_eq!(back.hotkey, Hotkey::Off);
        assert_eq!(back.colors[3], Color(0x123456));
        assert_eq!(back.sizes[0], 2);
        assert_eq!(back.save_dir, s.save_dir);
        assert_eq!(back.font, s.font);
    }

    #[test]
    fn ignores_garbage() {
        let s = parse("delay=7\nmode=nope\nrect.size=9\nrect.color=zz\njunk");
        assert_eq!(s.delay, 0);
        assert_eq!(s.mode, Mode::Region);
        assert_eq!(s.sizes[0], 1);
        assert_eq!(s.colors[0], Settings::default().colors[0]);
    }
}
