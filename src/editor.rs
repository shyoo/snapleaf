//! The markup editor: toolbar, zoomable canvas, editable shapes, inline text
//! entry (with IME), crop, undo/redo, and copy/save of the flattened image.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Input::Ime::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::app;
use crate::doc::*;
use crate::geom::*;
use crate::gfx::*;
use crate::fonts;
use crate::output;
use crate::settings::{self, DELAYS, FontSpec, Mode};
use crate::ui::{self, Button, ButtonState, Handler, Icon, Look, WindowSpec};

const TOOLBAR_H: f32 = 48.0;
const STATUS_H: f32 = 26.0;
const MARGIN: f32 = 16.0;

const PALETTE: [Color; 6] = [
    Color(0xE53935),
    Color(0xFDD835),
    Color(0x43A047),
    Color(0x1E88E5),
    Color(0x000000),
    Color(0xFFFFFF),
];
const DOTS: [f32; 3] = [2.5, 4.0, 6.0];

const TIMER_TIP: usize = 1;
const TIMER_CARET: usize = 2;
const TIMER_TOAST: usize = 3;

const CMD_SNIP: usize = 100;
const CMD_DELAY: usize = 200;
const CMD_FONT: usize = 300;
const CMD_BOLD: usize = 390;
const CMD_ITALIC: usize = 391;
const CMD_MORE_FONTS: usize = 392;

const FONT_BTN_W: f32 = 132.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Btn {
    New,
    NewMenu,
    Tool(Tool),
    Color(usize),
    CustomColor,
    Size(usize),
    Font,
    Undo,
    Redo,
    Copy,
    Save,
}

const TOOLS: [(Tool, Icon, &str); 7] = [
    (Tool::Select, Icon::Select, "Select and move (V)"),
    (Tool::Rect, Icon::Rect, "Rectangle (R)"),
    (Tool::Arrow, Icon::Arrow, "Arrow (A)"),
    (Tool::Text, Icon::Text, "Text (T)"),
    (Tool::Highlight, Icon::Highlight, "Highlighter (H)"),
    (Tool::Pen, Icon::Pen, "Pen (P)"),
    (Tool::Crop, Icon::Crop, "Crop (C)"),
];

enum Drag {
    None,
    /// The last shape is being drawn.
    Create,
    TextSelect,
    Move { from: Pt, orig: Shape },
    Handle { h: usize, orig: Shape },
    Crop { from: Pt, to: Pt },
    Pan { from: Pt, ox: f32, oy: f32 },
}

struct TextEdit {
    idx: usize,
    /// Caret as a char index into the shape's text.
    caret: usize,
    /// Fixed end of the selection, also a char index.
    anchor: usize,
    /// Document x retained across consecutive vertical moves.
    preferred_x: Option<f32>,
    /// Uncommitted IME composition, shown at the caret.
    composing: String,
}

impl TextEdit {
    fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    fn move_to(&mut self, caret: usize, extend: bool) {
        self.caret = caret;
        if !extend { self.anchor = caret; }
    }

    fn replace(&mut self, text: &mut String, value: &str) {
        let r = self.range();
        text.replace_range(byte_at(text, r.start)..byte_at(text, r.end), value);
        self.caret = r.start + value.chars().count();
        self.anchor = self.caret;
        self.preferred_x = None;
    }

    fn navigate(&mut self, text: &str, key: VIRTUAL_KEY, ctrl: bool, shift: bool) {
        let chars: Vec<char> = text.chars().collect();
        let r = self.range();
        let c = self.caret;
        let dest = match key {
            VK_LEFT if !shift && !r.is_empty() => r.start,
            VK_RIGHT if !shift && !r.is_empty() => r.end,
            VK_LEFT if ctrl => word_edge(&chars, c, false),
            VK_RIGHT if ctrl => word_edge(&chars, c, true),
            VK_LEFT => c.saturating_sub(1),
            VK_RIGHT => (c + 1).min(chars.len()),
            VK_HOME if ctrl => 0,
            VK_END if ctrl => chars.len(),
            VK_HOME => chars[..c].iter().rposition(|ch| *ch == '\n').map_or(0, |i| i + 1),
            VK_END => chars[c..].iter().position(|ch| *ch == '\n').map_or(chars.len(), |i| c + i),
            _ => c,
        };
        self.move_to(dest, shift);
        self.preferred_x = None;
    }

    fn delete(&mut self, text: &mut String, back: bool, ctrl: bool) {
        if self.range().is_empty() {
            let chars: Vec<char> = text.chars().collect();
            self.anchor = if ctrl { word_edge(&chars, self.caret, !back) }
                else if back { self.caret.saturating_sub(1) }
                else { (self.caret + 1).min(chars.len()) };
        }
        self.replace(text, "");
    }
}

fn word_edge(chars: &[char], mut c: usize, forward: bool) -> usize {
    let class = |ch: char| if ch.is_whitespace() { 0 } else if ch.is_alphanumeric() || ch == '_' { 1 } else { 2 };
    if forward {
        if let Some(ch) = chars.get(c) {
            let kind = class(*ch);
            while c < chars.len() && class(chars[c]) == kind { c += 1; }
        }
        while c < chars.len() && chars[c].is_whitespace() { c += 1; }
    } else {
        while c > 0 && chars[c - 1].is_whitespace() { c -= 1; }
        if c > 0 {
            let kind = class(chars[c - 1]);
            while c > 0 && class(chars[c - 1]) == kind { c -= 1; }
        }
    }
    c
}

struct Editor {
    target: WindowTarget,
    bmp: Option<ID2D1Bitmap>,
    doc: Doc,
    /// State last copied or saved; closing with a different state prompts.
    exported: Option<State>,
    tool: Tool,
    selected: Option<usize>,
    drag: Drag,
    edit: Option<TextEdit>,
    scale: f32,
    zoom: f32,
    ox: f32,
    oy: f32,
    fit: bool,
    hover: Option<Btn>,
    pressed: Option<Btn>,
    tip: Option<Btn>,
    toast: Option<(String, Instant)>,
    space: bool,
    high_surrogate: Option<u16>,
    caret_on: bool,
    custom_colors: [COLORREF; 16],
    title: String,
}

pub fn open(img: Image) -> Result<()> {
    let (work, scale) = ui::cursor_monitor();
    let title = format!("Snapleaf — {} × {}", img.w, img.h);
    let min_w = toolbar_min_width(scale);
    let cw = (img.w as f32 + 2.0 * MARGIN * scale).max(min_w);
    let ch = img.h as f32 + (TOOLBAR_H + STATUS_H + 2.0 * MARGIN) * scale;
    let mut r = RECT { left: 0, top: 0, right: cw as i32, bottom: ch as i32 };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0), (scale * 96.0) as u32);
    }
    let w = ((r.right - r.left) as f32).min(work.w() * 0.92) as i32;
    let h = ((r.bottom - r.top) as f32).min(work.h() * 0.92) as i32;
    let x = (work.center().x - w as f32 / 2.0) as i32;
    let y = (work.center().y - h as f32 / 2.0) as i32;

    let state = Editor {
        target: WindowTarget::default(),
        bmp: None,
        doc: Doc::new(img),
        exported: None,
        tool: Tool::Rect,
        selected: None,
        drag: Drag::None,
        edit: None,
        scale,
        zoom: 1.0,
        ox: 0.0,
        oy: 0.0,
        fit: true,
        hover: None,
        pressed: None,
        tip: None,
        toast: None,
        space: false,
        high_surrogate: None,
        caret_on: true,
        custom_colors: [COLORREF(0xFFFFFF); 16],
        title: title.clone(),
    };
    let hwnd = ui::create_window(
        WindowSpec {
            class: w!("SnapleafEditor"),
            title: &title,
            style: WS_OVERLAPPEDWINDOW,
            ex: WINDOW_EX_STYLE(0),
            rect: (x, y, w, h),
            cursor: IDC_ARROW,
            owner: None,
        },
        state,
    )?;
    ui::style_frame(hwnd, ui::theme().dark);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetForegroundWindow(hwnd);
        // Auto-copy the fresh capture, as Snipping Tool does.
        let _ = PostMessageW(Some(hwnd), WM_USER, WPARAM(0), LPARAM(0));
    }
    Ok(())
}

fn toolbar_min_width(s: f32) -> f32 {
    let h = 36.0;
    let left = 8.0 + 40.0 + 22.0 + 7.0 * h + 6.0 * 28.0 + 28.0 + 3.0 * 28.0 + FONT_BTN_W + 2.0 * h + 5.0 * 13.0;
    (left + 16.0 + 2.0 * h + 8.0) * s
}

fn byte_at(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}

fn constrain(kind: &Kind, a: Pt, b: Pt) -> Pt {
    let d = b - a;
    match kind {
        Kind::Rect { .. } | Kind::Highlight { .. } => {
            let side = d.x.abs().max(d.y.abs());
            pt(a.x + side * d.x.signum(), a.y + side * d.y.signum())
        }
        Kind::Arrow { .. } => {
            let len = d.len();
            let step = std::f32::consts::FRAC_PI_4;
            let ang = (d.y.atan2(d.x) / step).round() * step;
            pt(a.x + len * ang.cos(), a.y + len * ang.sin())
        }
        _ => b,
    }
}

impl Editor {
    // ------------------------------------------------------------ layout

    fn canvas(&self, hwnd: HWND) -> Rect {
        let r = ui::client_rect(hwnd);
        Rect::new(0.0, TOOLBAR_H * self.scale, r.r, (r.b - STATUS_H * self.scale).max(TOOLBAR_H * self.scale))
    }

    fn update_view(&mut self, hwnd: HWND) {
        let cv = self.canvas(hwnd);
        let crop = self.doc.state.crop;
        let title = format!("Snapleaf — {} × {}", crop.w() as i32, crop.h() as i32);
        if title != self.title {
            unsafe {
                let _ = SetWindowTextW(hwnd, &HSTRING::from(title.as_str()));
            }
            self.title = title;
        }
        let m = MARGIN * self.scale;
        if self.fit {
            self.zoom = 1f32.min((cv.w() - 2.0 * m) / crop.w()).min((cv.h() - 2.0 * m) / crop.h()).max(0.05);
        }
        let (sw, sh) = (crop.w() * self.zoom, crop.h() * self.zoom);
        self.ox = if sw <= cv.w() - 2.0 * m { ((cv.w() - sw) / 2.0).round() } else { self.ox.clamp(cv.w() - sw - m, m).round() };
        self.oy = if sh <= cv.h() - 2.0 * m { ((cv.h() - sh) / 2.0).round() } else { self.oy.clamp(cv.h() - sh - m, m).round() };
    }

    fn origin(&self, hwnd: HWND) -> Pt {
        let cv = self.canvas(hwnd);
        let c = self.doc.state.crop;
        pt(cv.l + self.ox - c.l * self.zoom, cv.t + self.oy - c.t * self.zoom)
    }

    fn to_screen(&self, hwnd: HWND, p: Pt) -> Pt {
        self.origin(hwnd) + p * self.zoom
    }

    fn to_doc(&self, hwnd: HWND, p: Pt) -> Pt {
        (p - self.origin(hwnd)) * (1.0 / self.zoom)
    }

    fn zoom_at(&mut self, hwnd: HWND, factor: f32, at: Pt) {
        let cv = self.canvas(hwnd);
        let new = (self.zoom * factor).clamp(0.05, 16.0);
        let k = new / self.zoom;
        let (px, py) = (at.x - cv.l, at.y - cv.t);
        self.ox = px - (px - self.ox) * k;
        self.oy = py - (py - self.oy) * k;
        self.zoom = new;
        self.fit = false;
        self.update_view(hwnd);
        ui::invalidate(hwnd);
    }

    fn buttons(&self, hwnd: HWND) -> (Vec<Button<Btn>>, Vec<f32>) {
        let s = self.scale;
        let h = 36.0 * s;
        let y = ((TOOLBAR_H * s - h) / 2.0).round();
        let mut v: Vec<Button<Btn>> = Vec::new();
        let mut seps = Vec::new();
        let mut x = 8.0 * s;
        let put = |v: &mut Vec<Button<Btn>>, mut b: Button<Btn>, w: f32, x: &mut f32| {
            b.rect = Rect::xywh(x.round(), y, w.round(), h);
            *x += w;
            v.push(b);
        };
        let sep = |seps: &mut Vec<f32>, x: &mut f32| {
            seps.push((*x + 6.5 * s).round() + 0.5);
            *x += 13.0 * s;
        };
        put(&mut v, Button::new(Btn::New, Icon::New, "New snip (Ctrl+N)").look(Look::Accent), 40.0 * s, &mut x);
        put(&mut v, Button::new(Btn::NewMenu, Icon::Chevron, "Snipping mode and delay"), 22.0 * s, &mut x);
        sep(&mut seps, &mut x);
        for (t, icon, tip) in TOOLS {
            put(&mut v, Button::new(Btn::Tool(t), icon, tip), h, &mut x);
        }
        sep(&mut seps, &mut x);
        for (i, c) in PALETTE.iter().enumerate() {
            put(&mut v, Button::new(Btn::Color(i), Icon::None, "Colour").look(Look::Swatch(*c)), 28.0 * s, &mut x);
        }
        let custom = settings::get().custom_color;
        put(&mut v, Button::new(Btn::CustomColor, Icon::None, "Custom colour…").look(Look::Swatch(custom)), 28.0 * s, &mut x);
        sep(&mut seps, &mut x);
        let size_tips = if self.style_tool() == Some(Tool::Text) {
            ["Small text", "Medium text", "Large text"]
        } else {
            ["Thin", "Medium", "Thick"]
        };
        for i in 0..3 {
            put(&mut v, Button::new(Btn::Size(i), Icon::None, size_tips[i]).look(Look::Dot(DOTS[i])), 28.0 * s, &mut x);
        }
        sep(&mut seps, &mut x);
        let family = self.text_font().unwrap_or_else(|| settings::get().font).family;
        let label: String = if family.chars().count() > 13 {
            family.chars().take(12).chain(std::iter::once('…')).collect()
        } else {
            family
        };
        let font_btn = Button::new(Btn::Font, Icon::None, "Font (Ctrl+B bold, Ctrl+I italic)").label(&label).menu();
        put(&mut v, font_btn, FONT_BTN_W * s, &mut x);
        sep(&mut seps, &mut x);
        put(&mut v, Button::new(Btn::Undo, Icon::Undo, "Undo (Ctrl+Z)"), h, &mut x);
        put(&mut v, Button::new(Btn::Redo, Icon::Redo, "Redo (Ctrl+Y)"), h, &mut x);
        let width = ui::client_rect(hwnd).r;
        let mut rx = (width - 8.0 * s - 2.0 * h).max(x + 16.0 * s);
        put(&mut v, Button::new(Btn::Copy, Icon::Copy, "Copy (Ctrl+C)"), h, &mut rx);
        put(&mut v, Button::new(Btn::Save, Icon::Save, "Save as PNG (Ctrl+S)"), h, &mut rx);
        (v, seps)
    }

    fn button_at(&self, hwnd: HWND, p: Pt) -> Option<Btn> {
        if p.y >= TOOLBAR_H * self.scale {
            return None;
        }
        self.buttons(hwnd).0.into_iter().find(|b| b.rect.contains(p)).map(|b| b.id)
    }

    // ------------------------------------------------------------ styles

    /// The tool whose colour/size the toolbar currently shows and edits.
    fn style_tool(&self) -> Option<Tool> {
        match self.selected.and_then(|i| self.doc.state.shapes.get(i)) {
            Some(s) => Some(s.tool()),
            None => self.tool.slot().map(|_| self.tool),
        }
    }

    fn style(&self) -> Option<(Color, usize)> {
        if let Some(s) = self.selected.and_then(|i| self.doc.state.shapes.get(i)) {
            return Some((s.color, s.size));
        }
        let slot = self.tool.slot()?;
        let set = settings::get();
        Some((set.colors[slot], set.sizes[slot]))
    }

    fn apply_style(&mut self, color: Option<Color>, size: Option<usize>) {
        let Some(tool) = self.style_tool() else { return };
        if let Some(i) = self.selected {
            self.doc.checkpoint();
            if let Some(s) = self.doc.state.shapes.get_mut(i) {
                if let Some(c) = color {
                    s.color = c;
                }
                if let Some(z) = size {
                    s.size = z;
                }
            }
            self.doc.settle();
        }
        if let Some(slot) = tool.slot() {
            settings::update(|s| {
                if let Some(c) = color {
                    s.colors[slot] = c;
                }
                if let Some(z) = size {
                    s.sizes[slot] = z;
                }
            });
        }
    }

    /// Font shown and edited by the font button: the selected text's, or the
    /// default for new text while the Text tool is active.
    fn text_font(&self) -> Option<FontSpec> {
        match self.selected.and_then(|i| self.doc.state.shapes.get(i)) {
            Some(Shape { kind: Kind::Text { font, .. }, .. }) => Some(font.clone()),
            Some(_) => None,
            None => (self.tool == Tool::Text).then(|| settings::get().font),
        }
    }

    fn apply_font(&mut self, hwnd: HWND, change: impl FnOnce(&mut FontSpec)) {
        let Some(mut font) = self.text_font() else { return };
        change(&mut font);
        if let Some(i) = self.selected {
            self.doc.checkpoint();
            if let Some(Shape { kind: Kind::Text { font: f, .. }, .. }) = self.doc.state.shapes.get_mut(i) {
                *f = font.clone();
            }
            self.doc.settle();
        }
        settings::update(|s| s.font = font);
        self.place_ime(hwnd);
        ui::invalidate(hwnd);
    }

    fn font_menu(&mut self, hwnd: HWND, anchor: Rect) {
        let Some(cur) = self.text_font() else { return };
        let mut families: Vec<String> = fonts::menu_families().into_iter().map(String::from).collect();
        if !families.iter().any(|f| f.eq_ignore_ascii_case(&cur.family)) {
            families.insert(0, cur.family.clone());
        }
        let mut items: Vec<(usize, String, bool)> = families
            .iter()
            .enumerate()
            .map(|(i, f)| (CMD_FONT + i, f.clone(), f.eq_ignore_ascii_case(&cur.family)))
            .collect();
        items.push((0, String::new(), false));
        items.push((CMD_BOLD, "Bold\tCtrl+B".into(), cur.bold));
        items.push((CMD_ITALIC, "Italic\tCtrl+I".into(), cur.italic));
        items.push((0, String::new(), false));
        items.push((CMD_MORE_FONTS, "More fonts…".into(), false));
        let cmd = ui::popup_menu(hwnd, ui::to_screen(hwnd, pt(anchor.l, anchor.b)), &items);
        self.pressed = None;
        match cmd {
            CMD_BOLD => self.apply_font(hwnd, |f| f.bold = !f.bold),
            CMD_ITALIC => self.apply_font(hwnd, |f| f.italic = !f.italic),
            CMD_MORE_FONTS => {
                if let Some(chosen) = fonts::choose(hwnd, &cur) {
                    self.apply_font(hwnd, |f| *f = chosen);
                }
            }
            c if c >= CMD_FONT && c < CMD_FONT + families.len() => {
                let family = families[c - CMD_FONT].clone();
                self.apply_font(hwnd, |f| f.family = family);
            }
            _ => {}
        }
        ui::invalidate(hwnd);
    }

    fn pick_custom_color(&mut self, hwnd: HWND) {
        let init = self.style().map(|s| s.0).unwrap_or(settings::get().custom_color);
        let mut cc = CHOOSECOLORW {
            lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: hwnd,
            rgbResult: COLORREF(init.colorref()),
            lpCustColors: self.custom_colors.as_mut_ptr(),
            Flags: CC_FULLOPEN | CC_RGBINIT,
            ..Default::default()
        };
        if unsafe { ChooseColorW(&mut cc) }.as_bool() {
            let c = Color::from_colorref(cc.rgbResult.0);
            settings::update(|s| s.custom_color = c);
            self.apply_style(Some(c), None);
        }
    }

    // ------------------------------------------------------------ actions

    fn set_tool(&mut self, hwnd: HWND, t: Tool) {
        self.commit_text(hwnd);
        self.tool = t;
        if t != Tool::Select {
            self.selected = None;
        }
        ui::invalidate(hwnd);
    }

    fn show_toast(&mut self, hwnd: HWND, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
        unsafe {
            SetTimer(Some(hwnd), TIMER_TOAST, 1800, None);
        }
        ui::invalidate(hwnd);
    }

    fn copy(&mut self, hwnd: HWND) {
        self.commit_text(hwnd);
        let result = self.doc.flatten().and_then(|img| output::copy_image(hwnd, &img));
        match result {
            Ok(()) => {
                self.exported = Some(self.doc.state.clone());
                self.show_toast(hwnd, "Copied to clipboard");
            }
            Err(e) => self.show_toast(hwnd, &format!("Copy failed: {}", e.message())),
        }
    }

    /// Returns false if the user cancelled.
    fn save(&mut self, hwnd: HWND) -> bool {
        self.commit_text(hwnd);
        let Some(path) = output::ask_save_path(hwnd) else { return false };
        let result = self.doc.flatten().map_err(|e| e.message().to_string()).and_then(|img| output::save_png(&path, &img));
        match result {
            Ok(()) => {
                self.exported = Some(self.doc.state.clone());
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.show_toast(hwnd, &format!("Saved {name}"));
                true
            }
            Err(e) => {
                ui::message_box(Some(hwnd), &format!("Could not save the image:\n{e}"), MB_ICONERROR);
                false
            }
        }
    }

    fn undo(&mut self, hwnd: HWND, redo: bool) {
        self.commit_text(hwnd);
        let changed = if redo { self.doc.redo() } else { self.doc.undo() };
        if changed {
            self.selected = None;
            self.drag = Drag::None;
            self.update_view(hwnd);
        }
        ui::invalidate(hwnd);
    }

    fn delete_selected(&mut self, hwnd: HWND) {
        if let Some(i) = self.selected.take() {
            self.doc.checkpoint();
            self.doc.state.shapes.remove(i);
            ui::invalidate(hwnd);
        }
    }

    fn nudge(&mut self, hwnd: HWND, d: Pt) {
        if let Some(i) = self.selected {
            self.doc.checkpoint();
            self.doc.state.shapes[i].translate(d);
            ui::invalidate(hwnd);
        }
    }

    fn unexported(&self) -> bool {
        self.doc.is_dirty() && self.exported.as_ref() != Some(&self.doc.state)
    }

    fn new_snip(&mut self, hwnd: HWND, mode: Mode, delay: u32) {
        self.commit_text(hwnd);
        unsafe {
            if self.unexported() {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            } else {
                let _ = DestroyWindow(hwnd);
            }
        }
        app::start(mode, delay);
    }

    fn new_menu(&mut self, hwnd: HWND, anchor: Rect) {
        let set = settings::get();
        let mut items: Vec<(usize, String, bool)> = Mode::ALL
            .iter()
            .enumerate()
            .map(|(i, m)| (CMD_SNIP + i, format!("{} snip", m.label()), false))
            .collect();
        items.push((0, String::new(), false));
        for (i, d) in DELAYS.iter().enumerate() {
            let label = if *d == 0 { "No delay".to_string() } else { format!("{d} s delay") };
            items.push((CMD_DELAY + i, label, *d == set.delay));
        }
        let cmd = ui::popup_menu(hwnd, ui::to_screen(hwnd, pt(anchor.l, anchor.b)), &items);
        self.pressed = None;
        match cmd {
            c if (CMD_SNIP..CMD_SNIP + 3).contains(&c) => {
                let m = Mode::ALL[c - CMD_SNIP];
                settings::update(|s| s.mode = m);
                self.new_snip(hwnd, m, settings::get().delay);
            }
            c if (CMD_DELAY..CMD_DELAY + DELAYS.len()).contains(&c) => settings::update(|s| s.delay = DELAYS[c - CMD_DELAY]),
            _ => {}
        }
    }

    fn press(&mut self, hwnd: HWND, b: Btn) {
        match b {
            Btn::New => {
                let s = settings::get();
                self.new_snip(hwnd, s.mode, s.delay);
            }
            Btn::NewMenu | Btn::Font => {}
            Btn::Tool(t) => self.set_tool(hwnd, t),
            Btn::Color(i) => self.apply_style(Some(PALETTE[i]), None),
            Btn::CustomColor => self.pick_custom_color(hwnd),
            Btn::Size(i) => self.apply_style(None, Some(i)),
            Btn::Undo => self.undo(hwnd, false),
            Btn::Redo => self.undo(hwnd, true),
            Btn::Copy => self.copy(hwnd),
            Btn::Save => {
                self.save(hwnd);
            }
        }
        ui::invalidate(hwnd);
    }

    fn enabled(&self, b: Btn) -> bool {
        match b {
            Btn::Color(_) | Btn::CustomColor => self.style_tool().is_some(),
            Btn::Size(_) => self.style_tool().is_some_and(|t| t.has_size()),
            Btn::Font => self.text_font().is_some(),
            Btn::Undo => self.doc.can_undo() || self.edit.is_some(),
            Btn::Redo => self.doc.can_redo(),
            _ => true,
        }
    }

    fn active(&self, b: Btn) -> bool {
        match b {
            Btn::Tool(t) => t == self.tool,
            Btn::Color(i) => self.style().is_some_and(|s| s.0 == PALETTE[i]),
            Btn::CustomColor => {
                self.style().is_some_and(|s| s.0 == settings::get().custom_color && !PALETTE.contains(&s.0))
            }
            Btn::Size(i) => self.style_tool().is_some_and(|t| t.has_size()) && self.style().is_some_and(|s| s.1 == i),
            _ => false,
        }
    }

    // ------------------------------------------------------------ text editing

    fn start_text(&mut self, hwnd: HWND, idx: Option<usize>, at: Pt, click: Option<Pt>) {
        self.commit_text(hwnd);
        self.doc.checkpoint();
        let idx = match idx {
            Some(i) => i,
            None => {
                let slot = Tool::Text.slot().unwrap_or(2);
                let set = settings::get();
                let size = set.sizes[slot];
                let (_, line_h) = measure_text("", &set.font, size);
                self.doc.state.shapes.push(Shape {
                    kind: Kind::Text { at: pt(at.x, at.y - line_h / 2.0), text: String::new(), font: set.font.clone() },
                    color: set.colors[slot],
                    size,
                });
                self.doc.state.shapes.len() - 1
            }
        };
        let caret = match (&self.doc.state.shapes[idx].kind, click) {
            (Kind::Text { text, .. }, Some(p)) => self.caret_from_point(idx, p).unwrap_or(text.chars().count()),
            (Kind::Text { text, .. }, None) => text.chars().count(),
            _ => 0,
        };
        self.selected = Some(idx);
        self.edit = Some(TextEdit { idx, caret, anchor: caret, preferred_x: None, composing: String::new() });
        self.caret_on = true;
        unsafe {
            let _ = ImmAssociateContextEx(hwnd, HIMC::default(), IACE_DEFAULT);
            SetTimer(Some(hwnd), TIMER_CARET, 530, None);
        }
        ui::invalidate(hwnd);
    }

    fn commit_text(&mut self, hwnd: HWND) {
        let Some(mut e) = self.edit.take() else { return };
        unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_CARET);
            if !e.composing.is_empty() {
                let himc = ImmGetContext(hwnd);
                let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
                let _ = ImmReleaseContext(hwnd, himc);
            }
            // Keys are tool shortcuts again, whatever the input language.
            let _ = ImmAssociateContextEx(hwnd, HIMC::default(), 0);
        }
        if !e.composing.is_empty() {
            let comp = std::mem::take(&mut e.composing);
            if let Some(Shape { kind: Kind::Text { text, .. }, .. }) = self.doc.state.shapes.get_mut(e.idx) {
                e.replace(text, &comp);
            }
        }
        let empty = matches!(self.doc.state.shapes.get(e.idx), Some(Shape { kind: Kind::Text { text, .. }, .. }) if text.trim().is_empty());
        if empty {
            self.doc.state.shapes.remove(e.idx);
            self.selected = None;
        }
        self.doc.settle();
        ui::invalidate(hwnd);
    }

    fn edit_text<R>(&mut self, f: impl FnOnce(&mut String, &mut TextEdit) -> R) -> Option<R> {
        let e = self.edit.as_mut()?;
        match self.doc.state.shapes.get_mut(e.idx) {
            Some(Shape { kind: Kind::Text { text, .. }, .. }) => {
                self.caret_on = true;
                Some(f(text, e))
            }
            _ => None,
        }
    }

    fn insert_text(&mut self, s: &str) {
        self.edit_text(|text, e| e.replace(text, s));
    }

    fn text_key(&mut self, hwnd: HWND, vk: VIRTUAL_KEY) -> bool {
        let ctrl = ui::key_down(VK_CONTROL.0);
        let shift = ui::key_down(VK_SHIFT.0);
        // IME owns navigation while its composition UI is active.
        if self.edit.as_ref().is_some_and(|e| !e.composing.is_empty()) && vk != VK_ESCAPE {
            return false;
        }
        let handled = match vk {
            VK_BACK | VK_DELETE => self.edit_text(|t, e| e.delete(t, vk == VK_BACK, ctrl)),
            VK_LEFT | VK_RIGHT | VK_HOME | VK_END => self.edit_text(|t, e| e.navigate(t, vk, ctrl, shift)),
            VK_UP | VK_DOWN => {
                self.move_text_vertical(vk == VK_DOWN, shift);
                Some(())
            }
            VK_A if ctrl => self.edit_text(|t, e| {
                e.anchor = 0;
                e.caret = t.chars().count();
                e.preferred_x = None;
            }),
            VK_ESCAPE | VK_RETURN if vk == VK_ESCAPE || ctrl => {
                self.commit_text(hwnd);
                Some(())
            }
            _ => None,
        };
        if handled.is_some() {
            ui::invalidate(hwnd);
            self.place_ime(hwnd);
        }
        handled.is_some()
    }

    fn move_text_vertical(&mut self, down: bool, shift: bool) {
        let Some((p, h)) = self.caret_doc() else { return };
        let Some(e) = &self.edit else { return };
        let x = e.preferred_x.unwrap_or(p.x);
        let target = pt(x, p.y + if down { h * 1.5 } else { -h * 0.5 });
        if let Some(caret) = self.caret_from_point(e.idx, target)
            && let Some(e) = self.edit.as_mut() {
                e.move_to(caret, shift);
                e.preferred_x = Some(x);
                self.caret_on = true;
            }
    }

    fn copy_text_selection(&mut self, hwnd: HWND, cut: bool) {
        let Some(e) = &self.edit else { return };
        let Kind::Text { text, .. } = &self.doc.state.shapes[e.idx].kind else { return };
        let r = e.range();
        if r.is_empty() { return; }
        let selected = &text[byte_at(text, r.start)..byte_at(text, r.end)];
        match output::copy_text(hwnd, selected) {
            Ok(()) if cut => { self.insert_text(""); }
            Err(_) => self.show_toast(hwnd, "Could not copy text"),
            _ => {}
        }
        self.place_ime(hwnd);
        ui::invalidate(hwnd);
    }

    /// Text as displayed while editing (composition spliced in) and the caret's UTF-16 index.
    fn display_text(&self) -> Option<(usize, String, u32)> {
        let e = self.edit.as_ref()?;
        let Some(Shape { kind: Kind::Text { text, .. }, .. }) = self.doc.state.shapes.get(e.idx) else { return None };
        let r = if e.composing.is_empty() { e.caret..e.caret } else { e.range() };
        let b = byte_at(text, r.start);
        let end = byte_at(text, r.end);
        let shown = format!("{}{}{}", &text[..b], e.composing, &text[end..]);
        let caret16 = (text[..b].encode_utf16().count() + e.composing.encode_utf16().count()) as u32;
        Some((e.idx, shown, caret16))
    }

    /// Caret position (doc coords, top of the line) and line height.
    fn caret_doc(&self) -> Option<(Pt, f32)> {
        let (idx, shown, caret16) = self.display_text()?;
        let s = &self.doc.state.shapes[idx];
        let Kind::Text { at, ref font, .. } = s.kind else { return None };
        let l = text_layout(&shown, &annotation_format(font, s.size)?).ok()?;
        let (mut x, mut y) = (0f32, 0f32);
        let mut m = DWRITE_HIT_TEST_METRICS::default();
        unsafe { l.HitTestTextPosition(caret16, false, &mut x, &mut y, &mut m).ok()? };
        Some((pt(at.x + x, at.y + y), m.height))
    }

    fn caret_from_point(&self, idx: usize, p: Pt) -> Option<usize> {
        let s = self.doc.state.shapes.get(idx)?;
        let Kind::Text { at, text, font } = &s.kind else { return None };
        let l = text_layout(text, &annotation_format(font, s.size)?).ok()?;
        let (mut trailing, mut inside) = (BOOL(0), BOOL(0));
        let mut m = DWRITE_HIT_TEST_METRICS::default();
        unsafe { l.HitTestPoint(p.x - at.x, p.y - at.y, &mut trailing, &mut inside, &mut m).ok()? };
        let pos16 = m.textPosition as usize + if trailing.as_bool() { m.length as usize } else { 0 };
        let mut n16 = 0;
        for (ci, ch) in text.chars().enumerate() {
            if n16 >= pos16 {
                return Some(ci);
            }
            n16 += ch.len_utf16();
        }
        Some(text.chars().count())
    }

    fn place_ime(&self, hwnd: HWND) {
        let Some((p, h)) = self.caret_doc() else { return };
        let sp = self.to_screen(hwnd, p);
        unsafe {
            let himc = ImmGetContext(hwnd);
            if himc.is_invalid() {
                return;
            }
            let cf = COMPOSITIONFORM {
                dwStyle: CFS_POINT,
                ptCurrentPos: POINT { x: sp.x as i32, y: sp.y as i32 },
                rcArea: RECT::default(),
            };
            let _ = ImmSetCompositionWindow(himc, &cf);
            let cand = CANDIDATEFORM {
                dwIndex: 0,
                dwStyle: CFS_CANDIDATEPOS,
                ptCurrentPos: POINT { x: sp.x as i32, y: (sp.y + h * self.zoom) as i32 + 4 },
                rcArea: RECT::default(),
            };
            let _ = ImmSetCandidateWindow(himc, &cand);
            let _ = ImmReleaseContext(hwnd, himc);
        }
    }

    // ------------------------------------------------------------ mouse

    fn handle_at(&self, hwnd: HWND, p: Pt) -> Option<usize> {
        let s = self.doc.state.shapes.get(self.selected?)?;
        s.handles().iter().position(|h| self.to_screen(hwnd, *h).dist(p) <= 7.0 * self.scale)
    }

    fn mouse_down(&mut self, hwnd: HWND, p: Pt, middle: bool, double: bool) {
        unsafe { SetFocus(Some(hwnd)).ok() };
        if let Some(b) = self.button_at(hwnd, p) {
            // Style buttons restyle the text being typed without ending the edit.
            if !matches!(b, Btn::Font | Btn::Color(_) | Btn::CustomColor | Btn::Size(_)) {
                self.commit_text(hwnd);
            }
            self.pressed = Some(b);
            let rect = || self.buttons(hwnd).0.into_iter().find(|x| x.id == b).map(|x| x.rect).unwrap_or_default();
            if b == Btn::NewMenu {
                let r = rect();
                self.new_menu(hwnd, r);
            } else if b == Btn::Font && self.enabled(b) {
                let r = rect();
                self.font_menu(hwnd, r);
            }
            ui::invalidate(hwnd);
            return;
        }
        if !self.canvas(hwnd).contains(p) {
            return;
        }
        if middle || self.space {
            self.drag = Drag::Pan { from: p, ox: self.ox, oy: self.oy };
            unsafe { SetCapture(hwnd) };
            return;
        }
        let d = self.to_doc(hwnd, p);
        let tol = 5.0 * self.scale / self.zoom;

        // Click or drag inside the active text to move/extend its selection.
        if let Some(e) = &self.edit {
            let idx = e.idx;
            if self.doc.state.shapes[idx].bounds().inflate(tol).contains(d) {
                if let Some(c) = self.caret_from_point(idx, d)
                    && let Some(e) = self.edit.as_mut() {
                        e.move_to(c, ui::key_down(VK_SHIFT.0));
                        e.preferred_x = None;
                    }
                self.drag = Drag::TextSelect;
                unsafe { SetCapture(hwnd) };
                self.place_ime(hwnd);
                self.caret_on = true;
                ui::invalidate(hwnd);
                return;
            }
            self.commit_text(hwnd);
        }

        if let Some(h) = self.handle_at(hwnd, p) {
            let orig = self.doc.state.shapes[self.selected.unwrap_or(0)].clone();
            self.doc.checkpoint();
            self.drag = Drag::Handle { h, orig };
            unsafe { SetCapture(hwnd) };
            return;
        }

        let hit = self.doc.hit(d, tol);
        match self.tool {
            Tool::Select => {
                self.selected = hit;
                if let Some(i) = hit {
                    if double && matches!(self.doc.state.shapes[i].kind, Kind::Text { .. }) {
                        self.start_text(hwnd, Some(i), d, Some(d));
                        return;
                    }
                    self.doc.checkpoint();
                    self.drag = Drag::Move { from: d, orig: self.doc.state.shapes[i].clone() };
                    unsafe { SetCapture(hwnd) };
                }
            }
            Tool::Text => {
                let text_hit = hit.filter(|i| matches!(self.doc.state.shapes[*i].kind, Kind::Text { .. }));
                self.start_text(hwnd, text_hit, d, text_hit.map(|_| d));
            }
            Tool::Crop => {
                let d = self.clamp_to_image(d);
                self.selected = None;
                self.drag = Drag::Crop { from: d, to: d };
                unsafe { SetCapture(hwnd) };
            }
            tool => {
                // Grabbing the shape that is already selected moves it.
                if hit.is_some() && hit == self.selected {
                    let i = hit.unwrap_or(0);
                    self.doc.checkpoint();
                    self.drag = Drag::Move { from: d, orig: self.doc.state.shapes[i].clone() };
                    unsafe { SetCapture(hwnd) };
                    ui::invalidate(hwnd);
                    return;
                }
                let slot = tool.slot().unwrap_or(0);
                let set = settings::get();
                let kind = match tool {
                    Tool::Rect => Kind::Rect { a: d, b: d },
                    Tool::Highlight => Kind::Highlight { a: d, b: d },
                    Tool::Arrow => Kind::Arrow { a: d, b: d },
                    _ => Kind::Pen { pts: vec![d] },
                };
                self.doc.checkpoint();
                self.doc.state.shapes.push(Shape { kind, color: set.colors[slot], size: set.sizes[slot] });
                self.selected = Some(self.doc.state.shapes.len() - 1);
                self.drag = Drag::Create;
                unsafe { SetCapture(hwnd) };
            }
        }
        ui::invalidate(hwnd);
    }

    fn clamp_to_image(&self, p: Pt) -> Pt {
        let c = self.doc.state.crop;
        pt(p.x.clamp(c.l, c.r), p.y.clamp(c.t, c.b))
    }

    fn mouse_move(&mut self, hwnd: HWND, p: Pt) {
        let d = self.to_doc(hwnd, p);
        let shift = ui::key_down(VK_SHIFT.0);
        let text_caret = if matches!(self.drag, Drag::TextSelect) {
            self.edit.as_ref().and_then(|e| self.caret_from_point(e.idx, d))
        } else { None };
        match &mut self.drag {
            Drag::TextSelect => {
                if let Some(caret) = text_caret && let Some(e) = self.edit.as_mut() {
                    e.move_to(caret, true);
                    e.preferred_x = None;
                    self.caret_on = true;
                }
                self.place_ime(hwnd);
            }
            Drag::None => {
                let h = self.button_at(hwnd, p);
                if h != self.hover {
                    self.hover = h;
                    self.tip = None;
                    ui::track_leave(hwnd);
                    unsafe {
                        if h.is_some() {
                            SetTimer(Some(hwnd), TIMER_TIP, 600, None);
                        }
                    }
                    ui::invalidate(hwnd);
                }
                return;
            }
            Drag::Create => {
                let zoom = self.zoom;
                if let Some(s) = self.doc.state.shapes.last_mut() {
                    let kind = s.kind.clone();
                    match &mut s.kind {
                        Kind::Rect { a, b } | Kind::Highlight { a, b } | Kind::Arrow { a, b } => {
                            *b = if shift { constrain(&kind, *a, d) } else { d };
                        }
                        Kind::Pen { pts } => {
                            if pts.last().is_none_or(|q| q.dist(d) * zoom >= 1.5) {
                                pts.push(d);
                            }
                        }
                        Kind::Text { .. } => {}
                    }
                }
            }
            Drag::Move { from, orig } => {
                let mut s = orig.clone();
                s.translate(d - *from);
                if let Some(i) = self.selected {
                    self.doc.state.shapes[i] = s;
                }
            }
            Drag::Handle { h, orig } => {
                let mut target = d;
                if shift
                    && let Kind::Arrow { a, b } = &orig.kind {
                        let anchor = if *h == 0 { *b } else { *a };
                        target = constrain(&orig.kind, anchor, d);
                    }
                let (h, orig) = (*h, orig.clone());
                if let Some(i) = self.selected {
                    self.doc.state.shapes[i].drag_handle(&orig, h, target);
                }
            }
            Drag::Crop { to, .. } => {
                let c = self.doc.state.crop;
                *to = pt(d.x.clamp(c.l, c.r), d.y.clamp(c.t, c.b));
            }
            Drag::Pan { from, ox, oy } => {
                self.ox = *ox + (p.x - from.x);
                self.oy = *oy + (p.y - from.y);
                self.fit = false;
                self.update_view(hwnd);
            }
        }
        ui::invalidate(hwnd);
    }

    fn mouse_up(&mut self, hwnd: HWND, p: Pt) {
        if let Some(b) = self.pressed.take() {
            if self.button_at(hwnd, p) == Some(b) && self.enabled(b) {
                self.press(hwnd, b);
            }
            ui::invalidate(hwnd);
            return;
        }
        unsafe {
            let _ = ReleaseCapture();
        }
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Create => {
                if self.doc.state.shapes.last().is_some_and(|s| s.is_degenerate()) {
                    self.doc.state.shapes.pop();
                    self.selected = None;
                }
                self.doc.settle();
            }
            Drag::Move { .. } | Drag::Handle { .. } => {
                if let Some(i) = self.selected {
                    // Keep rectangles normalised so handle order stays intuitive.
                    if let Kind::Rect { a, b } | Kind::Highlight { a, b } = &mut self.doc.state.shapes[i].kind {
                        let r = Rect::from_pts(*a, *b);
                        *a = pt(r.l, r.t);
                        *b = pt(r.r, r.b);
                    }
                }
                self.doc.settle();
            }
            Drag::Crop { from, to } => {
                let r = Rect::from_pts(from, to).round().intersect(&self.doc.state.crop);
                if r.w() >= 4.0 && r.h() >= 4.0 {
                    self.doc.checkpoint();
                    self.doc.state.crop = r;
                    self.fit = true;
                    self.tool = Tool::Select;
                    self.update_view(hwnd);
                }
            }
            Drag::Pan { .. } | Drag::TextSelect | Drag::None => {}
        }
        ui::invalidate(hwnd);
    }

    fn cursor(&self, hwnd: HWND) -> PCWSTR {
        let p = ui::cursor_pos(hwnd);
        if let Drag::Pan { .. } = self.drag {
            return IDC_SIZEALL;
        }
        if p.y < TOOLBAR_H * self.scale || !self.canvas(hwnd).contains(p) {
            return IDC_ARROW;
        }
        if self.space {
            return IDC_HAND;
        }
        if let Some(h) = self.handle_at(hwnd, p) {
            let is_box = self
                .selected
                .and_then(|i| self.doc.state.shapes.get(i))
                .is_some_and(|s| matches!(s.kind, Kind::Rect { .. } | Kind::Highlight { .. }));
            if is_box {
                return [IDC_SIZENWSE, IDC_SIZENS, IDC_SIZENESW, IDC_SIZEWE][h % 4];
            }
            return IDC_SIZEALL;
        }
        let d = self.to_doc(hwnd, p);
        let tol = 5.0 * self.scale / self.zoom;
        let hit = self.doc.hit(d, tol);
        match self.tool {
            Tool::Select if hit.is_some() => IDC_SIZEALL,
            Tool::Select => IDC_ARROW,
            Tool::Text => IDC_IBEAM,
            _ if hit.is_some() && hit == self.selected => IDC_SIZEALL,
            _ => IDC_CROSS,
        }
    }

    // ------------------------------------------------------------ painting

    fn paint(&mut self, hwnd: HWND, rt: &ID2D1RenderTarget, fresh: bool) -> Result<()> {
        if fresh || self.bmp.is_none() {
            self.bmp = Some(self.doc.image.to_bitmap(rt)?);
        }
        let t = ui::theme();
        let s = self.scale;
        let c = Canvas::new(rt)?;
        let client = ui::client_rect(hwnd);
        let cv = self.canvas(hwnd);
        c.clear(t.canvas);

        // Image and shapes, in document coordinates.
        let o = self.origin(hwnd);
        let crop = self.doc.state.crop;
        c.push_clip(cv);
        c.set_transform(self.zoom, o.x, o.y);
        c.push_clip(crop);
        if let Some(bmp) = &self.bmp {
            let full = Rect::new(0.0, 0.0, self.doc.image.w as f32, self.doc.image.h as f32);
            let interp = if self.zoom < 1.0 {
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR
            } else {
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR
            };
            unsafe { rt.DrawBitmap(bmp, Some(&full.d2d()), 1.0, interp, None) };
        }
        match self.display_text() {
            Some((idx, shown, _)) => {
                let mut shapes = self.doc.state.shapes.clone();
                if let Some(Shape { kind: Kind::Text { text, .. }, .. }) = shapes.get_mut(idx) {
                    *text = shown;
                }
                draw_shapes(&c, &shapes);
            }
            None => draw_shapes(&c, &self.doc.state.shapes),
        }
        c.pop_clip();
        c.reset_transform();

        let img_rect = Rect::from_pts(self.to_screen(hwnd, pt(crop.l, crop.t)), self.to_screen(hwnd, pt(crop.r, crop.b)));
        c.stroke(img_rect.inflate(0.5), t.text, 0.15, 1.0);

        self.paint_selection(hwnd, &c, &t)?;
        if let Drag::Crop { from, to } = self.drag {
            let r = Rect::from_pts(self.to_screen(hwnd, from), self.to_screen(hwnd, to));
            let ir = img_rect;
            c.fill(Rect::new(ir.l, ir.t, ir.r, r.t), Color::BLACK, 0.5);
            c.fill(Rect::new(ir.l, r.b, ir.r, ir.b), Color::BLACK, 0.5);
            c.fill(Rect::new(ir.l, r.t, r.l, r.b), Color::BLACK, 0.5);
            c.fill(Rect::new(r.r, r.t, ir.r, r.b), Color::BLACK, 0.5);
            c.stroke(r, Color::WHITE, 1.0, 1.0);
            c.stroke_styled(r, Color::BLACK, 1.0, 1.0, &gfx().dash);
        }
        c.pop_clip();

        // Toolbar.
        let tb = Rect::new(0.0, 0.0, client.r, TOOLBAR_H * s);
        c.fill(tb, t.panel, 1.0);
        c.fill(Rect::new(0.0, tb.b - 1.0, client.r, tb.b), t.text, 0.1);
        let fmt = text_format(12.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        let (buttons, seps) = self.buttons(hwnd);
        for b in &buttons {
            let st = ButtonState {
                hover: self.hover == Some(b.id) && self.enabled(b.id),
                pressed: self.pressed == Some(b.id),
                active: self.active(b.id),
                enabled: self.enabled(b.id),
            };
            ui::draw_button(&c, &t, b, st, s, &fmt);
        }
        for x in seps {
            c.line(pt(x, tb.t + 12.0 * s), pt(x, tb.b - 12.0 * s), t.text, 0.15, 1.0);
        }

        // Status bar.
        let sb = Rect::new(0.0, cv.b, client.r, client.b);
        c.fill(sb, t.panel, 1.0);
        c.fill(Rect::new(0.0, sb.t, client.r, sb.t + 1.0), t.text, 0.1);
        let sf = text_format(12.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        unsafe {
            sf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        }
        let hint = match self.tool {
            Tool::Select => "Drag to move · Double-click text to edit · Del to delete",
            Tool::Text => "Click to type · Esc or click outside to finish · Ctrl+B / Ctrl+I bold, italic",
            Tool::Crop => "Drag to choose the area to keep",
            _ => "Shift to constrain · Space+drag to pan · Ctrl+wheel to zoom",
        };
        let left = format!("{} × {} px     {}", crop.w() as i32, crop.h() as i32, hint);
        c.text(&left, &sf, Rect::new(10.0 * s, sb.t, sb.r - 80.0 * s, sb.b), t.dim, 1.0);
        unsafe {
            sf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING)?;
        }
        c.text(&format!("{:.0}%", self.zoom * 100.0), &sf, Rect::new(sb.r - 80.0 * s, sb.t, sb.r - 10.0 * s, sb.b), t.dim, 1.0);

        // Toast.
        if let Some((msg, _)) = &self.toast {
            let f = centered_format(13.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let l = text_layout(msg, &f)?;
            let (tw, th) = layout_size(&l);
            let r = Rect::xywh((cv.center().x - tw / 2.0 - 16.0 * s).round(), cv.b - th - 34.0 * s, tw + 32.0 * s, th + 16.0 * s);
            c.fill_round(r, r.h() / 2.0, Color(0x1F1F1F), 0.92);
            c.text(msg, &f, r, Color::WHITE, 1.0);
        }

        // Tooltip.
        if let Some(id) = self.tip
            && let Some(b) = buttons.iter().find(|b| b.id == id) {
                ui::draw_tip(&c, &t, &b.tip, b.rect, client, s, &fmt);
            }
        Ok(())
    }

    fn paint_selection(&self, hwnd: HWND, c: &Canvas, t: &ui::Theme) -> Result<()> {
        let Some(shape) = self.selected.and_then(|i| self.doc.state.shapes.get(i)) else { return Ok(()) };
        let s = self.scale;
        let accent = t.accent;
        let b = shape.bounds();
        let r = Rect::from_pts(self.to_screen(hwnd, pt(b.l, b.t)), self.to_screen(hwnd, pt(b.r, b.b))).inflate(3.0 * s).round();
        let r = r.offset(pt(0.5, 0.5));
        let editing = self.edit.is_some();
        if !matches!(shape.kind, Kind::Arrow { .. }) || editing {
            c.stroke(r, Color::WHITE, 0.9, 1.0);
            c.stroke_styled(r, accent, 1.0, 1.0, &gfx().dash);
        }
        if !editing {
            for h in shape.handles() {
                let p = self.to_screen(hwnd, h);
                if matches!(shape.kind, Kind::Arrow { .. }) {
                    c.fill_circle(p, 5.0 * s, Color::WHITE, 1.0);
                    c.stroke_circle(p, 5.0 * s, accent, 1.0, 1.5 * s);
                } else {
                    let hr = Rect::xywh(p.x - 4.0 * s, p.y - 4.0 * s, 8.0 * s, 8.0 * s).round().offset(pt(0.5, 0.5));
                    c.fill(hr, Color::WHITE, 1.0);
                    c.stroke(hr, accent, 1.0, 1.0);
                }
            }
        }
        if let Some(e) = &self.edit
            && e.composing.is_empty() && !e.range().is_empty()
            && let Kind::Text { at, text, font } = &shape.kind
            && let Some(format) = annotation_format(font, shape.size) {
                let layout = text_layout(text, &format)?;
                let range = e.range();
                let start = text[..byte_at(text, range.start)].encode_utf16().count() as u32;
                let len = text[byte_at(text, range.start)..byte_at(text, range.end)].encode_utf16().count() as u32;
                let mut count = 0;
                unsafe { let _ = layout.HitTestTextRange(start, len, at.x, at.y, None, &mut count); }
                let mut metrics = vec![DWRITE_HIT_TEST_METRICS::default(); count as usize];
                unsafe { layout.HitTestTextRange(start, len, at.x, at.y, Some(&mut metrics), &mut count)?; }
                for m in metrics.iter().take(count as usize) {
                    let a = self.to_screen(hwnd, pt(m.left, m.top));
                    let b = self.to_screen(hwnd, pt(m.left + m.width.max(2.0), m.top + m.height));
                    c.fill(Rect::from_pts(a, b), accent, 0.35);
                }
            }
        if editing && self.caret_on
            && let Some((p, h)) = self.caret_doc() {
                let top = self.to_screen(hwnd, p);
                let x = top.x.round() + 0.5;
                c.line(pt(x, top.y), pt(x, top.y + h * self.zoom), shape.color, 1.0, (1.5 * s).max(1.0));
                if shape.color.luma() > 200 {
                    c.line(pt(x + 1.5 * s, top.y), pt(x + 1.5 * s, top.y + h * self.zoom), Color::BLACK, 0.4, 1.0);
                }
            }
        Ok(())
    }
}

fn imm_string(himc: HIMC, kind: IME_COMPOSITION_STRING) -> String {
    unsafe {
        let len = ImmGetCompositionStringW(himc, kind, None, 0);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize / 2];
        ImmGetCompositionStringW(himc, kind, Some(buf.as_mut_ptr() as *mut _), len as u32);
        String::from_utf16_lossy(&buf)
    }
}

impl Handler for Editor {
    fn on(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_CREATE => {
                self.scale = ui::dpi_scale(hwnd);
                unsafe {
                    let _ = ImmAssociateContextEx(hwnd, HIMC::default(), 0);
                }
                self.update_view(hwnd);
                None
            }
            WM_USER => {
                self.copy(hwnd);
                Some(LRESULT(0))
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                let mut target = std::mem::take(&mut self.target);
                target.paint(hwnd, |rt, fresh| self.paint(hwnd, rt, fresh));
                self.target = target;
                unsafe {
                    let _ = EndPaint(hwnd, &ps);
                }
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_SIZE => {
                self.target.resize((lp.0 & 0xFFFF) as u32, ((lp.0 >> 16) & 0xFFFF) as u32);
                self.update_view(hwnd);
                ui::invalidate(hwnd);
                None
            }
            WM_GETMINMAXINFO => {
                let mmi = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
                let s = if self.scale > 0.0 { self.scale } else { 1.0 };
                let mut r = RECT { left: 0, top: 0, right: toolbar_min_width(s) as i32, bottom: (TOOLBAR_H + STATUS_H + 120.0) as i32 };
                unsafe {
                    let _ = AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0), (s * 96.0) as u32);
                }
                mmi.ptMinTrackSize = POINT { x: r.right - r.left, y: r.bottom - r.top };
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                self.scale = (wp.0 & 0xFFFF) as f32 / 96.0;
                let r = unsafe { &*(lp.0 as *const RECT) };
                unsafe {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                self.update_view(hwnd);
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE => {
                if ui::refresh_theme() {
                    ui::style_frame(hwnd, ui::theme().dark);
                }
                ui::invalidate(hwnd);
                None
            }
            WM_SETCURSOR if (lp.0 & 0xFFFF) as u32 == HTCLIENT => {
                ui::set_cursor(self.cursor(hwnd));
                Some(LRESULT(1))
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                self.mouse_down(hwnd, ui::xy(lp), false, msg == WM_LBUTTONDBLCLK);
                Some(LRESULT(0))
            }
            WM_MBUTTONDOWN => {
                self.mouse_down(hwnd, ui::xy(lp), true, false);
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                self.mouse_move(hwnd, ui::xy(lp));
                Some(LRESULT(0))
            }
            WM_LBUTTONUP | WM_MBUTTONUP => {
                self.mouse_up(hwnd, ui::xy(lp));
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED => {
                if !matches!(self.drag, Drag::None) && lp.0 != hwnd.0 as isize {
                    let p = ui::cursor_pos(hwnd);
                    self.mouse_up(hwnd, p);
                }
                None
            }
            WM_MOUSELEAVE => {
                self.hover = None;
                self.tip = None;
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as f32 / 120.0;
                let mut sp = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut sp);
                }
                let at = pt(sp.x as f32, sp.y as f32);
                if ui::key_down(VK_CONTROL.0) && msg == WM_MOUSEWHEEL {
                    self.zoom_at(hwnd, 1.15f32.powf(delta), at);
                } else {
                    let step = 60.0 * self.scale * delta;
                    if msg == WM_MOUSEHWHEEL {
                        self.ox -= step;
                    } else if ui::key_down(VK_SHIFT.0) {
                        self.ox += step;
                    } else {
                        self.oy += step;
                    }
                    self.update_view(hwnd);
                    ui::invalidate(hwnd);
                }
                Some(LRESULT(0))
            }
            WM_TIMER => {
                match wp.0 {
                    TIMER_TIP => {
                        unsafe {
                            let _ = KillTimer(Some(hwnd), TIMER_TIP);
                        }
                        if self.pressed.is_none() {
                            self.tip = self.hover;
                        }
                    }
                    TIMER_CARET => self.caret_on = !self.caret_on,
                    TIMER_TOAST
                        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() >= Duration::from_millis(1700)) => {
                            self.toast = None;
                            unsafe {
                                let _ = KillTimer(Some(hwnd), TIMER_TOAST);
                            }
                        }
                    _ => {}
                }
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = VIRTUAL_KEY(wp.0 as u16);
                let ctrl = ui::key_down(VK_CONTROL.0);
                let shift = ui::key_down(VK_SHIFT.0);
                if ctrl {
                    match wp.0 as u8 {
                        b'C' if self.edit.is_some() => self.copy_text_selection(hwnd, false),
                        b'X' if self.edit.is_some() => self.copy_text_selection(hwnd, true),
                        b'C' => self.copy(hwnd),
                        b'S' => {
                            self.save(hwnd);
                        }
                        b'Z' if shift => self.undo(hwnd, true),
                        b'Z' => self.undo(hwnd, false),
                        b'Y' => self.undo(hwnd, true),
                        b'B' => self.apply_font(hwnd, |f| f.bold = !f.bold),
                        b'I' => self.apply_font(hwnd, |f| f.italic = !f.italic),
                        b'V' if self.edit.is_some() => {
                            if let Some(t) = output::paste_text(hwnd) {
                                self.insert_text(&t);
                                self.place_ime(hwnd);
                                ui::invalidate(hwnd);
                            }
                        }
                        b'N' => {
                            let s = settings::get();
                            self.new_snip(hwnd, s.mode, s.delay);
                        }
                        b'W' => unsafe {
                            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                        },
                        b'0' => {
                            self.fit = true;
                            self.update_view(hwnd);
                            ui::invalidate(hwnd);
                        }
                        b'1' => {
                            let c = self.canvas(hwnd).center();
                            self.zoom_at(hwnd, 1.0 / self.zoom, c);
                        }
                        0xBB | 0x6B => {
                            let c = self.canvas(hwnd).center();
                            self.zoom_at(hwnd, 1.25, c);
                        }
                        0xBD | 0x6D => {
                            let c = self.canvas(hwnd).center();
                            self.zoom_at(hwnd, 0.8, c);
                        }
                        _ if self.edit.is_some() => {
                            self.text_key(hwnd, vk);
                        }
                        _ => {}
                    }
                    return Some(LRESULT(0));
                }
                if self.edit.is_some() {
                    self.text_key(hwnd, vk);
                    return Some(LRESULT(0));
                }
                let step = if shift { 10.0 } else { 1.0 };
                match vk {
                    VK_DELETE | VK_BACK => self.delete_selected(hwnd),
                    VK_ESCAPE => {
                        if !matches!(self.drag, Drag::None) {
                            if !matches!(self.drag, Drag::Crop { .. } | Drag::Pan { .. }) {
                                self.doc.revert();
                            }
                            self.drag = Drag::None;
                            unsafe {
                                let _ = ReleaseCapture();
                            }
                        }
                        self.selected = None;
                        ui::invalidate(hwnd);
                    }
                    VK_LEFT => self.nudge(hwnd, pt(-step, 0.0)),
                    VK_RIGHT => self.nudge(hwnd, pt(step, 0.0)),
                    VK_UP => self.nudge(hwnd, pt(0.0, -step)),
                    VK_DOWN => self.nudge(hwnd, pt(0.0, step)),
                    VK_SPACE => {
                        self.space = true;
                        ui::set_cursor(IDC_HAND);
                    }
                    _ => match wp.0 as u8 {
                        b'V' => self.set_tool(hwnd, Tool::Select),
                        b'R' => self.set_tool(hwnd, Tool::Rect),
                        b'A' => self.set_tool(hwnd, Tool::Arrow),
                        b'T' => self.set_tool(hwnd, Tool::Text),
                        b'H' => self.set_tool(hwnd, Tool::Highlight),
                        b'P' => self.set_tool(hwnd, Tool::Pen),
                        b'C' => self.set_tool(hwnd, Tool::Crop),
                        _ => {}
                    },
                }
                Some(LRESULT(0))
            }
            WM_KEYUP => {
                if wp.0 as u16 == VK_SPACE.0 {
                    self.space = false;
                }
                None
            }
            WM_CHAR => {
                if self.edit.is_none() {
                    return Some(LRESULT(0));
                }
                let u = wp.0 as u16;
                match u {
                    0x0D => self.insert_text("\n"),
                    0xD800..=0xDBFF => self.high_surrogate = Some(u),
                    0xDC00..=0xDFFF => {
                        if let Some(hi) = self.high_surrogate.take() {
                            self.insert_text(&String::from_utf16_lossy(&[hi, u]));
                        }
                    }
                    u if u >= 0x20 && u != 0x7F => self.insert_text(&String::from_utf16_lossy(&[u])),
                    _ => {}
                }
                self.place_ime(hwnd);
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_IME_STARTCOMPOSITION if self.edit.is_some() => {
                self.place_ime(hwnd);
                Some(LRESULT(0))
            }
            WM_IME_COMPOSITION if self.edit.is_some() => {
                let flags = lp.0 as u32;
                unsafe {
                    let himc = ImmGetContext(hwnd);
                    if flags & GCS_RESULTSTR.0 != 0 {
                        let s = imm_string(himc, GCS_RESULTSTR);
                        if let Some(e) = self.edit.as_mut() {
                            e.composing.clear();
                        }
                        self.insert_text(&s);
                    }
                    if flags & GCS_COMPSTR.0 != 0 {
                        let s = imm_string(himc, GCS_COMPSTR);
                        if let Some(e) = self.edit.as_mut() {
                            e.composing = s;
                        }
                    }
                    let _ = ImmReleaseContext(hwnd, himc);
                }
                self.place_ime(hwnd);
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_IME_ENDCOMPOSITION if self.edit.is_some() => {
                if let Some(e) = self.edit.as_mut() {
                    e.composing.clear();
                }
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_KILLFOCUS => {
                self.space = false;
                None
            }
            WM_CLOSE => {
                self.commit_text(hwnd);
                if self.unexported() {
                    let r = ui::message_box(Some(hwnd), "Save your marked-up snip before closing?", MB_YESNOCANCEL | MB_ICONQUESTION);
                    if r == IDCANCEL || (r == IDYES && !self.save(hwnd)) {
                        return Some(LRESULT(0));
                    }
                }
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(caret: usize) -> TextEdit {
        TextEdit { idx: 0, caret, anchor: caret, preferred_x: None, composing: String::new() }
    }

    #[test]
    fn shift_selection_reverses_and_arrows_collapse_it() {
        let mut e = edit(2);
        e.navigate("abcd", VK_RIGHT, false, true);
        assert_eq!(e.range(), 2..3);
        e.navigate("abcd", VK_LEFT, false, true);
        e.navigate("abcd", VK_LEFT, false, true);
        assert_eq!(e.range(), 1..2);
        e.navigate("abcd", VK_RIGHT, false, false);
        assert_eq!((e.caret, e.anchor), (2, 2));
        e.navigate("abcd", VK_END, true, true);
        e.navigate("abcd", VK_LEFT, false, false);
        assert_eq!((e.caret, e.anchor), (2, 2));
    }

    #[test]
    fn selection_replacement_and_deletion_preserve_unicode() {
        let mut text = String::from("a\u{1f642}\u{d55c}\nz");
        let mut e = edit(3);
        e.anchor = 1;
        e.replace(&mut text, "\u{1f680}");
        assert_eq!(text, "a\u{1f680}\nz");
        assert_eq!((e.caret, e.anchor), (2, 2));
        e.delete(&mut text, true, false);
        assert_eq!(text, "a\nz");
        e.anchor = 3;
        e.delete(&mut text, false, false);
        assert_eq!(text, "a");
        e.move_to(0, false);
        e.delete(&mut text, true, false);
        assert_eq!(text, "a");
        e.move_to(1, false);
        e.delete(&mut text, false, false);
        assert_eq!(text, "a");
    }

    #[test]
    fn home_end_use_line_or_document_boundaries() {
        let text = "one\n\ntwo\n";
        let mut e = edit(6);
        e.navigate(text, VK_HOME, false, false);
        assert_eq!(e.caret, 5);
        e.navigate(text, VK_END, false, true);
        assert_eq!(e.range(), 5..8);
        e.navigate(text, VK_HOME, true, true);
        assert_eq!((e.caret, e.anchor), (0, 5));
        e.navigate(text, VK_END, true, false);
        assert_eq!(e.caret, 9);
        e.navigate(text, VK_HOME, false, false);
        assert_eq!(e.caret, 9);
        e.move_to(4, false);
        e.navigate(text, VK_END, false, false);
        assert_eq!(e.caret, 4);
    }

    #[test]
    fn ctrl_navigation_and_deletion_follow_word_boundaries() {
        let mut text = String::from("hello,  world\nnext");
        let mut e = edit(0);
        e.navigate(&text, VK_RIGHT, true, false);
        assert_eq!(e.caret, 5);
        e.navigate(&text, VK_RIGHT, true, true);
        assert_eq!(e.range(), 5..8);
        e.replace(&mut text, " ");
        assert_eq!(text, "hello world\nnext");
        e.delete(&mut text, false, true);
        assert_eq!(text, "hello next");
        e.delete(&mut text, true, true);
        assert_eq!(text, "next");
        assert_eq!(e.caret, 0);
    }
}
