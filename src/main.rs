// Konsol penceresini gizle (Windows)
#![windows_subsystem = "windows"]

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontFamily, FontId, Layout, Margin, Pos2, Rect,
    RichText, Sense, Shape, Stroke,
};
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, Session, SessionOptions};

// ───────────────────────── Renk paleti ─────────────────────────
const BG: Color32 = Color32::from_rgb(0x0D, 0x10, 0x16);
const PANEL: Color32 = Color32::from_rgb(0x14, 0x18, 0x21);
const CARD: Color32 = Color32::from_rgb(0x1A, 0x1F, 0x2B);
const CARD_HOVER: Color32 = Color32::from_rgb(0x21, 0x27, 0x35);
const CARD_SEL: Color32 = Color32::from_rgb(0x1D, 0x25, 0x3A);
const INPUT: Color32 = Color32::from_rgb(0x10, 0x14, 0x1C);
const BORDER: Color32 = Color32::from_rgb(0x2A, 0x31, 0x42);
const TRACK: Color32 = Color32::from_rgb(0x2A, 0x31, 0x42);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEC, 0xF4);
const MUTED: Color32 = Color32::from_rgb(0x8A, 0x93, 0xA6);
const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x8C, 0xFF);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x74, 0x9E, 0xFF);
const GREEN: Color32 = Color32::from_rgb(0x2E, 0xD6, 0x96);
const AMBER: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
const RED: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x6B);

fn tint(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
fn reg(size: f32) -> FontId {
    FontId::proportional(size)
}

// ───────────────────────── Windows: pencere + tepsi ─────────────────────────
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
mod win {
    use super::MAIN_HWND;
    use std::sync::atomic::Ordering;

    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: isize) -> i32;
    }

    pub fn hide() {
        let h = MAIN_HWND.load(Ordering::Relaxed);
        if h != 0 {
            unsafe {
                ShowWindow(h, 0); // SW_HIDE
            }
        }
    }

    pub fn show() {
        let h = MAIN_HWND.load(Ordering::Relaxed);
        if h != 0 {
            unsafe {
                ShowWindow(h, 9); // SW_RESTORE
                SetForegroundWindow(h);
            }
        }
    }

    pub fn grab_hwnd(frame: &eframe::Frame) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        }
    }
}

#[cfg(not(windows))]
mod win {
    pub fn hide() {}
    pub fn show() {}
    pub fn grab_hwnd(_frame: &eframe::Frame) -> Option<isize> {
        None
    }
}

#[cfg(windows)]
mod tray {
    use std::cell::RefCell;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = RefCell::new(None);
    }

    /// Tepsi simgesini oluşturur. Başarısız olursa false döner.
    pub fn init(rgba: Vec<u8>, w: u32, h: u32) -> bool {
        let Ok(icon) = Icon::from_rgba(rgba, w, h) else {
            return false;
        };
        let menu = Menu::new();
        let show = MenuItem::with_id("show", "MiniTorrent'i göster", true, None);
        let quit = MenuItem::with_id("quit", "Çıkış", true, None);
        if menu.append(&show).is_err() {
            return false;
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        let built = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("MiniTorrent")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build();
        let Ok(tray) = built else {
            return false;
        };
        TRAY.with(|t| *t.borrow_mut() = Some(tray));

        TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| match e {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => super::win::show(),
            _ => {}
        }));
        MenuEvent::set_event_handler(Some(|e: MenuEvent| match e.id.0.as_str() {
            "show" => super::win::show(),
            "quit" => quit_app(),
            _ => {}
        }));
        true
    }

    fn quit_app() {
        // Tepsi simgesini kaldır, sonra çık (liste her değişiklikte zaten kaydediliyor)
        TRAY.with(|t| {
            t.borrow_mut().take();
        });
        std::process::exit(0);
    }
}

#[cfg(not(windows))]
mod tray {
    pub fn init(_rgba: Vec<u8>, _w: u32, _h: u32) -> bool {
        false
    }
}

// ───────────────────────── Ayarlar ─────────────────────────
#[derive(Clone)]
struct Settings {
    down_kbps: u32, // 0 = sınırsız
    up_kbps: u32,
    close_to_tray: bool,
}

impl Settings {
    fn load() -> Self {
        let mut s = Settings {
            down_kbps: 0,
            up_kbps: 0,
            close_to_tray: true,
        };
        if let Ok(text) = std::fs::read_to_string(app_dir().join("settings.txt")) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    match k.trim() {
                        "down_kbps" => s.down_kbps = v.trim().parse().unwrap_or(0),
                        "up_kbps" => s.up_kbps = v.trim().parse().unwrap_or(0),
                        "close_to_tray" => s.close_to_tray = v.trim() != "0",
                        _ => {}
                    }
                }
            }
        }
        s
    }

    fn save(&self) {
        let text = format!(
            "down_kbps={}\nup_kbps={}\nclose_to_tray={}\n",
            self.down_kbps,
            self.up_kbps,
            if self.close_to_tray { 1 } else { 0 }
        );
        let _ = std::fs::write(app_dir().join("settings.txt"), text);
    }
}

fn apply_limits(session: &Session, s: &Settings) {
    // librqbit sınırları saniyedeki bayt cinsinden ister; 0 → sınırsız (None)
    let to_bps = |kbps: u32| NonZeroU32::new(kbps.saturating_mul(1024));
    session.ratelimits.set_download_bps(to_bps(s.down_kbps));
    session.ratelimits.set_upload_bps(to_bps(s.up_kbps));
}

fn fmt_kb(kbps: u32) -> String {
    if kbps == 0 {
        "Sınırsız".into()
    } else if kbps >= 1024 && kbps % 1024 == 0 {
        format!("{} MB/s", kbps / 1024)
    } else if kbps >= 1024 {
        format!("{:.1} MB/s", kbps as f32 / 1024.0)
    } else {
        format!("{kbps} KB/s")
    }
}

// ───────────────────────── Torrent verisi ─────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Pending,
    Checking,
    Downloading,
    Done,
    Paused,
    Error,
}

struct Entry {
    uid: u64,
    /// "magnet:?..." bağlantısı ya da kopyalanmış .torrent dosyasının yolu
    source: String,
    path: String,
    id: Option<usize>,
    handle: Option<Arc<ManagedTorrent>>,
    error: Option<String>,
    name: String,
    total: u64,
    done: u64,
    kind: Kind,
    state: String,
    down: f64,
    up: f64,
    last_done: u64,
    last_up: u64,
    last_t: Instant,
}

impl Entry {
    fn new(uid: u64, source: String, path: String) -> Self {
        Entry {
            uid,
            source,
            path,
            id: None,
            handle: None,
            error: None,
            name: "Metadata alınıyor…".into(),
            total: 0,
            done: 0,
            kind: Kind::Pending,
            state: "Metadata alınıyor…".into(),
            down: 0.0,
            up: 0.0,
            last_done: 0,
            last_up: 0,
            last_t: Instant::now(),
        }
    }

    fn refresh(&mut self, now: Instant) {
        if let Some(err) = &self.error {
            self.kind = Kind::Error;
            self.state = format!("Hata: {err}");
            return;
        }
        let Some(h) = self.handle.clone() else {
            return;
        };
        let st = h.stats();
        if let Some(n) = h.name() {
            self.name = n;
        }
        self.total = st.total_bytes;
        self.done = st.progress_bytes;

        // TorrentStatsState Display: "initializing" | "live" | "paused" | "error"
        let state_s = st.state.to_string();
        let (kind, text) = if let Some(err) = &st.error {
            (Kind::Error, format!("Hata: {err}"))
        } else if state_s == "paused" {
            (Kind::Paused, "Duraklatıldı".to_string())
        } else if st.finished {
            (Kind::Done, "Tamamlandı · seed ediliyor".to_string())
        } else {
            match state_s.as_str() {
                "initializing" => (Kind::Checking, "Kontrol ediliyor".to_string()),
                "live" => (Kind::Downloading, "İndiriliyor".to_string()),
                "error" => (Kind::Error, "Hata".to_string()),
                other => (Kind::Checking, other.to_string()),
            }
        };
        self.kind = kind;
        self.state = text;

        let dt = now.duration_since(self.last_t).as_secs_f64();
        if dt >= 1.0 {
            self.down = self.done.saturating_sub(self.last_done) as f64 / dt;
            self.up = st.uploaded_bytes.saturating_sub(self.last_up) as f64 / dt;
            self.last_done = self.done;
            self.last_up = st.uploaded_bytes;
            self.last_t = now;
        }
    }
}

/// Arayüzün çizim sırasında kullandığı, kilitsiz kopya
#[derive(Clone)]
struct View {
    uid: u64,
    name: String,
    total: u64,
    done: u64,
    kind: Kind,
    state: String,
    down: f64,
    up: f64,
    path: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Active,
    Done,
    Paused,
}

impl Filter {
    fn matches(self, k: Kind) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => matches!(k, Kind::Pending | Kind::Checking | Kind::Downloading),
            Filter::Done => k == Kind::Done,
            Filter::Paused => k == Kind::Paused,
        }
    }
}

enum Action {
    SubmitInput,
    FromClipboard,
    PickTorrent,
    ChooseDir,
    Pause(u64),
    Resume(u64),
    AskRemove(u64),
    Remove(u64, bool),
    CancelConfirm,
    OpenFolder(String),
    OpenSettings,
    SaveSettings,
    CloseSettings,
}

// ───────────────────────── Biçimlendirme ─────────────────────────
fn fmt_size(n: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{:.0} {}", v, units[i])
    } else {
        format!("{:.1} {}", v, units[i])
    }
}

fn fmt_speed(n: f64) -> String {
    if n < 1.0 {
        "0 KB/s".into()
    } else {
        format!("{}/s", fmt_size(n))
    }
}

fn fmt_eta(v: &View) -> String {
    if v.total == 0 || v.done >= v.total {
        return "-".into();
    }
    if v.down < 1.0 {
        return "belirsiz".into();
    }
    let s = ((v.total - v.done) as f64 / v.down) as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 99 {
        "belirsiz".into()
    } else if h > 0 {
        format!("{h} sa {m} dk")
    } else if m > 0 {
        format!("{m} dk {sec} sn")
    } else {
        format!("{sec} sn")
    }
}

fn shorten(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
        format!("…{tail}")
    }
}

// ───────────────────────── Dosya / sistem ─────────────────────────
fn app_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(base).join("MiniTorrent");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn data_file() -> PathBuf {
    app_dir().join("torrents.txt")
}

fn save_list(list: &[Entry]) {
    let text: String = list
        .iter()
        .map(|e| format!("{}\t{}\n", e.path, e.source))
        .collect();
    let _ = std::fs::write(data_file(), text);
}

fn default_download_dir() -> String {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Downloads")
        .to_string_lossy()
        .into_owned()
}

fn open_folder(path: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn load_rgba(bytes: &[u8]) -> (Vec<u8>, [usize; 2]) {
    let img = image::load_from_memory(bytes)
        .expect("simge okunamadı")
        .into_rgba8();
    let (w, h) = img.dimensions();
    (img.into_raw(), [w as usize, h as usize])
}

// ───────────────────────── Tema ─────────────────────────
fn setup_fonts(ctx: &egui::Context) {
    // Windows'ta Segoe UI kullan (yerel görünüm + tam Türkçe karakter desteği)
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let regular = std::fs::read(format!("{windir}\\Fonts\\segoeui.ttf")).ok();
    let bolder = std::fs::read(format!("{windir}\\Fonts\\segoeuib.ttf")).ok();

    let mut fonts = egui::FontDefinitions::default();
    let mut prop = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(b) = regular {
        fonts
            .font_data
            .insert("segoe".into(), egui::FontData::from_owned(b).into());
        prop.insert(0, "segoe".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop.clone());

    let mut bold_family = prop;
    if let Some(b) = bolder {
        fonts
            .font_data
            .insert("segoe-bold".into(), egui::FontData::from_owned(b).into());
        bold_family.insert(0, "segoe-bold".into());
    }
    fonts
        .families
        .insert(FontFamily::Name("bold".into()), bold_family);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_rounding = 14.0.into();
    v.menu_rounding = 10.0.into();
    v.extreme_bg_color = INPUT;
    v.selection.bg_fill = tint(ACCENT, 90);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.active.weak_bg_fill = CARD_SEL;
    v.widgets.active.bg_fill = CARD_SEL;
    v.widgets.noninteractive.rounding = 8.0.into();
    v.widgets.inactive.rounding = 8.0.into();
    v.widgets.hovered.rounding = 8.0.into();
    v.widgets.active.rounding = 8.0.into();
    v.widgets.open.rounding = 8.0.into();
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.interaction.selectable_labels = false;
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.5));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.0));
        s.text_styles
            .insert(egui::TextStyle::Heading, bold(20.0));
    });
}

// ───────────────────────── Vektör simgeler ─────────────────────────
#[derive(Clone, Copy)]
enum Icon {
    Down,
    Up,
    Pause,
    Play,
    Trash,
    Folder,
    Plus,
    Clipboard,
    Check,
    File,
    Gear,
}

fn paint_icon(p: &egui::Painter, c: Pos2, size: f32, icon: Icon, color: Color32) {
    let h = size / 2.0;
    let st = Stroke::new((size / 9.0).max(1.4), color);
    match icon {
        Icon::Down | Icon::Up => {
            let s = if matches!(icon, Icon::Down) { 1.0 } else { -1.0 };
            p.line_segment([pos2(c.x, c.y - h * s), pos2(c.x, c.y + h * s)], st);
            p.add(Shape::line(
                vec![
                    pos2(c.x - h * 0.65, c.y + h * 0.15 * s),
                    pos2(c.x, c.y + h * s),
                    pos2(c.x + h * 0.65, c.y + h * 0.15 * s),
                ],
                st,
            ));
        }
        Icon::Pause => {
            let w = h * 0.42;
            for dx in [-h * 0.45, h * 0.45] {
                p.rect_filled(
                    Rect::from_min_max(
                        pos2(c.x + dx - w / 2.0, c.y - h * 0.8),
                        pos2(c.x + dx + w / 2.0, c.y + h * 0.8),
                    ),
                    1.5,
                    color,
                );
            }
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![
                    pos2(c.x - h * 0.55, c.y - h * 0.85),
                    pos2(c.x + h * 0.85, c.y),
                    pos2(c.x - h * 0.55, c.y + h * 0.85),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Trash => {
            p.line_segment(
                [pos2(c.x - h * 0.85, c.y - h * 0.5), pos2(c.x + h * 0.85, c.y - h * 0.5)],
                st,
            );
            p.add(Shape::line(
                vec![
                    pos2(c.x - h * 0.3, c.y - h * 0.5),
                    pos2(c.x - h * 0.3, c.y - h * 0.9),
                    pos2(c.x + h * 0.3, c.y - h * 0.9),
                    pos2(c.x + h * 0.3, c.y - h * 0.5),
                ],
                st,
            ));
            p.rect_stroke(
                Rect::from_min_max(
                    pos2(c.x - h * 0.6, c.y - h * 0.5),
                    pos2(c.x + h * 0.6, c.y + h * 0.9),
                ),
                2.0,
                st,
            );
        }
        Icon::Folder => {
            p.rect_stroke(
                Rect::from_min_max(
                    pos2(c.x - h * 0.95, c.y - h * 0.4),
                    pos2(c.x + h * 0.95, c.y + h * 0.75),
                ),
                2.5,
                st,
            );
            p.add(Shape::line(
                vec![
                    pos2(c.x - h * 0.95, c.y - h * 0.4),
                    pos2(c.x - h * 0.95, c.y - h * 0.75),
                    pos2(c.x - h * 0.2, c.y - h * 0.75),
                    pos2(c.x + h * 0.05, c.y - h * 0.4),
                ],
                st,
            ));
        }
        Icon::Plus => {
            p.line_segment([pos2(c.x - h * 0.8, c.y), pos2(c.x + h * 0.8, c.y)], st);
            p.line_segment([pos2(c.x, c.y - h * 0.8), pos2(c.x, c.y + h * 0.8)], st);
        }
        Icon::Clipboard => {
            p.rect_stroke(
                Rect::from_min_max(
                    pos2(c.x - h * 0.7, c.y - h * 0.75),
                    pos2(c.x + h * 0.7, c.y + h * 0.95),
                ),
                2.5,
                st,
            );
            p.rect_filled(
                Rect::from_min_max(
                    pos2(c.x - h * 0.35, c.y - h * 1.0),
                    pos2(c.x + h * 0.35, c.y - h * 0.5),
                ),
                1.5,
                color,
            );
        }
        Icon::Check => {
            p.add(Shape::line(
                vec![
                    pos2(c.x - h * 0.75, c.y),
                    pos2(c.x - h * 0.2, c.y + h * 0.55),
                    pos2(c.x + h * 0.8, c.y - h * 0.6),
                ],
                Stroke::new((size / 7.0).max(1.8), color),
            ));
        }
        Icon::File => {
            p.rect_stroke(
                Rect::from_min_max(
                    pos2(c.x - h * 0.65, c.y - h * 0.95),
                    pos2(c.x + h * 0.65, c.y + h * 0.95),
                ),
                2.5,
                st,
            );
            for dy in [-0.3_f32, 0.15, 0.6] {
                p.line_segment(
                    [pos2(c.x - h * 0.3, c.y + h * dy), pos2(c.x + h * 0.3, c.y + h * dy)],
                    Stroke::new(st.width * 0.8, color),
                );
            }
        }
        Icon::Gear => {
            p.circle_stroke(c, h * 0.62, st);
            p.circle_stroke(c, h * 0.24, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (sn, cs) = a.sin_cos();
                p.line_segment(
                    [
                        pos2(c.x + cs * h * 0.68, c.y + sn * h * 0.68),
                        pos2(c.x + cs * h * 0.98, c.y + sn * h * 0.98),
                    ],
                    Stroke::new(st.width * 1.7, color),
                );
            }
        }
    }
}

fn icon_space(ui: &mut egui::Ui, icon: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size + 2.0, size + 2.0), Sense::hover());
    paint_icon(ui.painter(), rect.center(), size, icon, color);
}

// ───────────────────────── Bileşenler ─────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum Btn {
    Primary,
    Secondary,
    Danger,
}

fn button(ui: &mut egui::Ui, text: &str, icon: Option<Icon>, kind: Btn) -> egui::Response {
    let (bg, bg_hover, fg, border) = match kind {
        Btn::Primary => (ACCENT, ACCENT_HOVER, Color32::WHITE, ACCENT),
        Btn::Secondary => (CARD, CARD_HOVER, TEXT, BORDER),
        Btn::Danger => (tint(RED, 28), tint(RED, 60), RED, tint(RED, 90)),
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), bold(14.0), fg);
    let icon_w = if icon.is_some() { 24.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 30.0, 38.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if resp.hovered() { bg_hover } else { bg };
    let p = ui.painter();
    p.rect(rect, 10.0, fill, Stroke::new(1.0, border));
    let mut x = rect.left() + 15.0;
    if let Some(i) = icon {
        paint_icon(p, pos2(x + 8.0, rect.center().y), 15.0, i, fg);
        x += icon_w;
    }
    let gy = rect.center().y - galley.size().y / 2.0;
    p.galley(pos2(x, gy), galley, fg);
    resp
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    let resp = resp
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let fill = if resp.hovered() { CARD_HOVER } else { CARD };
    p.rect(rect, 10.0, fill, Stroke::new(1.0, BORDER));
    paint_icon(
        p,
        rect.center(),
        18.0,
        icon,
        if resp.hovered() { TEXT } else { MUTED },
    );
    resp
}

fn pill(ui: &mut egui::Ui, text: &str, active: bool) -> egui::Response {
    let width = {
        let g = ui
            .painter()
            .layout_no_wrap(text.to_owned(), reg(13.5), MUTED);
        g.size().x + 30.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if active {
        ACCENT
    } else if resp.hovered() {
        TEXT
    } else {
        MUTED
    };
    let fill = if active {
        tint(ACCENT, 38)
    } else if resp.hovered() {
        CARD
    } else {
        Color32::TRANSPARENT
    };
    let font = if active { bold(13.5) } else { reg(13.5) };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let p = ui.painter();
    p.rect_filled(rect, 16.0, fill);
    if active {
        p.rect_stroke(rect, 16.0, Stroke::new(1.0, tint(ACCENT, 110)));
    }
    let pos = pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    p.galley(pos, galley, color);
    resp
}

fn chip(ui: &mut egui::Ui, icon: Icon, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), bold(13.5), TEXT);
    let size = vec2(galley.size().x + 52.0, 32.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect(rect, 16.0, tint(color, 24), Stroke::new(1.0, tint(color, 70)));
    paint_icon(p, pos2(rect.left() + 18.0, rect.center().y), 13.0, icon, color);
    let gy = rect.center().y - galley.size().y / 2.0;
    p.galley(pos2(rect.left() + 32.0, gy), galley, TEXT);
}

fn card_btn(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    icon: Icon,
    tip: &str,
    hover_color: Color32,
) -> bool {
    let r = ui
        .interact(rect, id, Sense::click())
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    if r.hovered() {
        p.rect_filled(rect, 9.0, tint(hover_color, 40));
    }
    paint_icon(
        p,
        rect.center(),
        16.0,
        icon,
        if r.hovered() { hover_color } else { MUTED },
    );
    r.clicked()
}

fn ellipsized(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: width,
        max_rows: 1,
        break_anywhere: true,
        ..Default::default()
    };
    painter.layout_job(job)
}

/// Ayarlar penceresindeki hız sınırı satırı
fn limit_row(ui: &mut egui::Ui, label: &str, icon: Icon, color: Color32, buf: &mut String) {
    ui.horizontal(|ui| {
        icon_space(ui, icon, color, 14.0);
        ui.label(RichText::new(label).font(bold(14.0)).color(TEXT));
        ui.add_space(6.0);
        egui::Frame::none()
            .fill(INPUT)
            .rounding(8.0)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(buf)
                        .frame(false)
                        .desired_width(80.0)
                        .font(reg(14.5)),
                );
            });
        ui.label(RichText::new("KB/s").color(MUTED));
    });
    ui.horizontal(|ui| {
        ui.add_space(22.0);
        let presets = [
            ("Sınırsız", "0"),
            ("500 KB", "500"),
            ("1 MB", "1024"),
            ("5 MB", "5120"),
            ("10 MB", "10240"),
        ];
        for (txt, val) in presets {
            if pill(ui, txt, buf.trim() == val).clicked() {
                *buf = val.to_string();
            }
        }
    });
}

// ───────────────────────── Torrent kartı ─────────────────────────
fn draw_card(ui: &mut egui::Ui, v: &View, selected: bool, actions: &mut Vec<Action>) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 90.0), Sense::click());

    let accent = match v.kind {
        Kind::Downloading => ACCENT,
        Kind::Done => GREEN,
        Kind::Paused => AMBER,
        Kind::Error => RED,
        Kind::Pending | Kind::Checking => MUTED,
    };

    let btn_zone = 3.0 * 32.0 + 2.0 * 6.0 + 16.0;
    let speed_w = 124.0;
    let left = rect.left() + 16.0 + 44.0 + 14.0;
    let speed_x = rect.right() - btn_zone - speed_w;
    let cw = (speed_x - 14.0 - left).max(60.0);

    {
        let p = ui.painter();
        let fill = if selected {
            CARD_SEL
        } else if resp.hovered() {
            CARD_HOVER
        } else {
            CARD
        };
        let stroke = if selected {
            Stroke::new(1.5, ACCENT)
        } else {
            Stroke::new(1.0, BORDER)
        };
        p.rect(rect, 14.0, fill, stroke);

        // Durum rozeti
        let bc = pos2(rect.left() + 16.0 + 22.0, rect.center().y);
        p.circle_filled(bc, 22.0, tint(accent, 36));
        match v.kind {
            Kind::Downloading | Kind::Pending | Kind::Checking => {
                paint_icon(p, bc, 18.0, Icon::Down, accent)
            }
            Kind::Done => paint_icon(p, bc, 18.0, Icon::Check, accent),
            Kind::Paused => paint_icon(p, bc, 16.0, Icon::Pause, accent),
            Kind::Error => {
                p.text(bc, Align2::CENTER_CENTER, "!", bold(22.0), accent);
            }
        }

        // Ad
        let g = ellipsized(p, &v.name, bold(15.5), TEXT, cw);
        p.galley(pos2(left, rect.top() + 14.0), g, TEXT);

        // İlerleme çubuğu
        let bar = Rect::from_min_size(pos2(left, rect.top() + 45.0), vec2(cw, 6.0));
        p.rect_filled(bar, 3.0, TRACK);
        let frac = if v.total > 0 {
            (v.done as f32 / v.total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if frac > 0.0 {
            p.rect_filled(
                Rect::from_min_size(bar.min, vec2((bar.width() * frac).max(6.0), 6.0)),
                3.0,
                accent,
            );
        }

        // Bilgi satırı
        let info = match v.kind {
            Kind::Pending => "Metadata alınıyor…".to_string(),
            Kind::Error => v.state.clone(),
            _ => {
                let mut s = format!(
                    "{} / {}  ·  %{:.1}  ·  {}",
                    fmt_size(v.done as f64),
                    fmt_size(v.total as f64),
                    frac * 100.0,
                    v.state
                );
                if v.kind == Kind::Downloading {
                    s.push_str(&format!("  ·  Kalan: {}", fmt_eta(v)));
                }
                s
            }
        };
        let clip = Rect::from_min_size(pos2(left, rect.top() + 58.0), vec2(cw, 20.0));
        p.with_clip_rect(clip).text(
            pos2(left, rect.top() + 60.0),
            Align2::LEFT_TOP,
            info,
            reg(12.5),
            MUTED,
        );

        // Hızlar
        let dcol = if v.down >= 1.0 { ACCENT } else { MUTED };
        let ucol = if v.up >= 1.0 { GREEN } else { MUTED };
        paint_icon(p, pos2(speed_x + 8.0, rect.top() + 32.0), 12.0, Icon::Down, dcol);
        p.text(
            pos2(speed_x + 24.0, rect.top() + 32.0),
            Align2::LEFT_CENTER,
            fmt_speed(v.down),
            reg(13.5),
            if v.down >= 1.0 { TEXT } else { MUTED },
        );
        paint_icon(p, pos2(speed_x + 8.0, rect.top() + 58.0), 12.0, Icon::Up, ucol);
        p.text(
            pos2(speed_x + 24.0, rect.top() + 58.0),
            Align2::LEFT_CENTER,
            fmt_speed(v.up),
            reg(13.5),
            if v.up >= 1.0 { TEXT } else { MUTED },
        );
    }

    // Eylem düğmeleri
    let by = rect.center().y - 16.0;
    let mut bx = rect.right() - 16.0 - 32.0;
    let base = ui.id().with(("card", v.uid));

    if card_btn(
        ui,
        Rect::from_min_size(pos2(bx, by), vec2(32.0, 32.0)),
        base.with("rm"),
        Icon::Trash,
        "Kaldır",
        RED,
    ) {
        actions.push(Action::AskRemove(v.uid));
    }
    bx -= 38.0;
    if card_btn(
        ui,
        Rect::from_min_size(pos2(bx, by), vec2(32.0, 32.0)),
        base.with("dir"),
        Icon::Folder,
        "Klasörü aç",
        ACCENT,
    ) {
        actions.push(Action::OpenFolder(v.path.clone()));
    }
    bx -= 38.0;
    let (icon, tip, col) = if v.kind == Kind::Paused {
        (Icon::Play, "Devam et", GREEN)
    } else {
        (Icon::Pause, "Duraklat", AMBER)
    };
    if card_btn(
        ui,
        Rect::from_min_size(pos2(bx, by), vec2(32.0, 32.0)),
        base.with("pp"),
        icon,
        tip,
        col,
    ) {
        if v.kind == Kind::Paused {
            actions.push(Action::Resume(v.uid));
        } else {
            actions.push(Action::Pause(v.uid));
        }
    }

    resp.context_menu(|ui| {
        if v.kind == Kind::Paused {
            if ui.button("Devam et").clicked() {
                actions.push(Action::Resume(v.uid));
                ui.close_menu();
            }
        } else if ui.button("Duraklat").clicked() {
            actions.push(Action::Pause(v.uid));
            ui.close_menu();
        }
        if ui.button("Klasörü aç").clicked() {
            actions.push(Action::OpenFolder(v.path.clone()));
            ui.close_menu();
        }
        if ui.button("Kaldır…").clicked() {
            actions.push(Action::AskRemove(v.uid));
            ui.close_menu();
        }
    });
    if resp.double_clicked() {
        actions.push(Action::OpenFolder(v.path.clone()));
    }
    resp.clicked()
}

// ───────────────────────── Uygulama ─────────────────────────
struct App {
    rt: tokio::runtime::Runtime,
    session: Arc<Session>,
    entries: Arc<Mutex<Vec<Entry>>>,
    input: String,
    save_dir: String,
    selected: Option<u64>,
    next_uid: u64,
    filter: Filter,
    confirm: Option<u64>,
    notice: Option<(String, Instant)>,
    logo_small: egui::TextureHandle,
    logo_big: egui::TextureHandle,
    // ayarlar
    settings: Settings,
    settings_open: bool,
    edit_down: String,
    edit_up: String,
    edit_tray: bool,
    settings_err: Option<String>,
    tray_ok: bool,
}

impl App {
    fn new(
        rt: tokio::runtime::Runtime,
        session: Arc<Session>,
        ctx: &egui::Context,
        settings: Settings,
        tray_ok: bool,
    ) -> Self {
        let (s_rgba, s_size) = load_rgba(include_bytes!("../assets/logo_small.png"));
        let (b_rgba, b_size) = load_rgba(include_bytes!("../assets/logo_big.png"));
        let logo_small = ctx.load_texture(
            "logo_small",
            egui::ColorImage::from_rgba_unmultiplied(s_size, &s_rgba),
            egui::TextureOptions::LINEAR,
        );
        let logo_big = ctx.load_texture(
            "logo_big",
            egui::ColorImage::from_rgba_unmultiplied(b_size, &b_rgba),
            egui::TextureOptions::LINEAR,
        );

        let mut app = App {
            rt,
            session,
            entries: Arc::new(Mutex::new(Vec::new())),
            input: String::new(),
            save_dir: default_download_dir(),
            selected: None,
            next_uid: 1,
            filter: Filter::All,
            confirm: None,
            notice: None,
            logo_small,
            logo_big,
            settings,
            settings_open: false,
            edit_down: String::new(),
            edit_up: String::new(),
            edit_tray: true,
            settings_err: None,
            tray_ok,
        };
        if let Ok(text) = std::fs::read_to_string(data_file()) {
            for line in text.lines() {
                if let Some((path, source)) = line.split_once('\t') {
                    app.add_source(source.to_string(), path.to_string());
                }
            }
        }
        app
    }

    fn save(&self) {
        let list = self.entries.lock().unwrap();
        save_list(&list);
    }

    fn notify(&mut self, msg: &str) {
        self.notice = Some((msg.to_string(), Instant::now()));
    }

    fn submit(&mut self, text: &str) -> bool {
        let t = text.trim().to_string();
        if !t.starts_with("magnet:?") {
            self.notify("Geçerli bir magnet linki değil (magnet:? ile başlamalı).");
            return false;
        }
        let exists = self.entries.lock().unwrap().iter().any(|e| e.source == t);
        if exists {
            self.notify("Bu torrent zaten listede.");
            return false;
        }
        let dir = self.save_dir.clone();
        self.add_source(t, dir);
        self.notice = None;
        true
    }

    /// .torrent dosyasını uygulama klasörüne kopyalayıp ekler
    fn add_torrent_file(&mut self, src: &Path) {
        let is_torrent = src
            .extension()
            .map(|e| e.eq_ignore_ascii_case("torrent"))
            .unwrap_or(false);
        if !is_torrent {
            self.notify("Yalnızca .torrent dosyaları eklenebilir.");
            return;
        }
        let dest_dir = app_dir().join("torrents");
        let _ = std::fs::create_dir_all(&dest_dir);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let dest = dest_dir.join(format!("{}_{}.torrent", stamp, self.next_uid));
        if let Err(e) = std::fs::copy(src, &dest) {
            self.notify(&format!("Dosya okunamadı: {e}"));
            return;
        }
        let dir = self.save_dir.clone();
        self.add_source(dest.to_string_lossy().into_owned(), dir);
        self.notice = None;
    }

    /// `source`: magnet bağlantısı ya da .torrent dosya yolu
    fn add_source(&mut self, source: String, path: String) {
        let uid = self.next_uid;
        self.next_uid += 1;
        self.entries
            .lock()
            .unwrap()
            .push(Entry::new(uid, source.clone(), path.clone()));

        let session = self.session.clone();
        let entries = self.entries.clone();
        self.rt.spawn(async move {
            let opts = AddTorrentOptions {
                output_folder: Some(path),
                overwrite: true,
                ..Default::default()
            };
            let prepared: Result<AddTorrent<'static>, String> = if source.starts_with("magnet:") {
                Ok(AddTorrent::from_url(source.clone()))
            } else {
                std::fs::read(&source)
                    .map(|b| AddTorrent::from_bytes(b))
                    .map_err(|e| format!("Dosya okunamadı: {e}"))
            };
            let res = match prepared {
                Ok(add) => session
                    .add_torrent(add, Some(opts))
                    .await
                    .map_err(|e| e.to_string()),
                Err(msg) => Err(msg),
            };

            let mut list = entries.lock().unwrap();
            let Some(pos) = list.iter().position(|e| e.uid == uid) else {
                return;
            };
            match res {
                Ok(AddTorrentResponse::Added(id, h))
                | Ok(AddTorrentResponse::AlreadyManaged(id, h)) => {
                    let duplicate = list.iter().any(|o| o.uid != uid && o.id == Some(id));
                    if duplicate {
                        // Aynı torrent zaten listede: yinelenen kaydı kaldır
                        list.remove(pos);
                        if !source.starts_with("magnet:") {
                            let _ = std::fs::remove_file(&source);
                        }
                        save_list(&list);
                    } else {
                        list[pos].id = Some(id);
                        list[pos].handle = Some(h);
                    }
                }
                Ok(_) => list[pos].error = Some("beklenmeyen yanıt".into()),
                Err(msg) => list[pos].error = Some(msg),
            }
        });
        self.save();
    }

    fn handle_of(&self, uid: u64) -> Option<Arc<ManagedTorrent>> {
        let list = self.entries.lock().unwrap();
        list.iter().find(|e| e.uid == uid)?.handle.clone()
    }

    fn pause(&self, uid: u64) {
        if let Some(h) = self.handle_of(uid) {
            let s = self.session.clone();
            self.rt.spawn(async move {
                let _ = s.pause(&h).await;
            });
        }
    }

    fn resume(&self, uid: u64) {
        if let Some(h) = self.handle_of(uid) {
            let s = self.session.clone();
            self.rt.spawn(async move {
                let _ = s.unpause(&h).await;
            });
        }
    }

    fn remove(&mut self, uid: u64, delete_files: bool) {
        let removed = {
            let mut list = self.entries.lock().unwrap();
            list.iter().position(|e| e.uid == uid).map(|i| list.remove(i))
        };
        if let Some(e) = removed {
            if !e.source.starts_with("magnet:") {
                let _ = std::fs::remove_file(&e.source);
            }
            if let Some(id) = e.id {
                let s = self.session.clone();
                self.rt.spawn(async move {
                    let _ = s.delete(id.into(), delete_files).await;
                });
            }
        }
        if self.selected == Some(uid) {
            self.selected = None;
        }
        self.save();
    }

    fn open_settings(&mut self) {
        self.edit_down = self.settings.down_kbps.to_string();
        self.edit_up = self.settings.up_kbps.to_string();
        self.edit_tray = self.settings.close_to_tray;
        self.settings_err = None;
        self.settings_open = true;
    }

    fn save_settings(&mut self) {
        let parse = |s: &str| -> Option<u32> {
            let t = s.trim();
            if t.is_empty() {
                Some(0)
            } else {
                t.parse::<u32>().ok().filter(|v| *v <= 4_000_000)
            }
        };
        let (Some(d), Some(u)) = (parse(&self.edit_down), parse(&self.edit_up)) else {
            self.settings_err = Some("Hız sınırları için geçerli bir sayı gir (KB/s).".into());
            return;
        };
        self.settings.down_kbps = d;
        self.settings.up_kbps = u;
        self.settings.close_to_tray = self.edit_tray;
        self.settings.save();
        apply_limits(&self.session, &self.settings);
        self.settings_open = false;
        self.settings_err = None;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));

        // Pencere tutamacını bir kez al (tepsiye gizleme / gösterme için)
        if MAIN_HWND.load(Ordering::Relaxed) == 0 {
            if let Some(h) = win::grab_hwnd(frame) {
                MAIN_HWND.store(h, Ordering::Relaxed);
            }
        }

        // Kapat düğmesi → sistem tepsisine küçült
        if self.tray_ok
            && self.settings.close_to_tray
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            win::hide();
        }

        // Sürükle-bırak: .torrent dosyaları
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for p in dropped {
            self.add_torrent_file(&p);
        }

        // 1) Durumu güncelle, çizim için kilitsiz kopya al
        let now = Instant::now();
        let mut views: Vec<View> = Vec::new();
        let (mut tdown, mut tup) = (0.0f64, 0.0f64);
        {
            let mut list = self.entries.lock().unwrap();
            for e in list.iter_mut() {
                e.refresh(now);
                tdown += e.down;
                tup += e.up;
                views.push(View {
                    uid: e.uid,
                    name: e.name.clone(),
                    total: e.total,
                    done: e.done,
                    kind: e.kind,
                    state: e.state.clone(),
                    down: e.down,
                    up: e.up,
                    path: e.path.clone(),
                });
            }
        }
        let mut actions: Vec<Action> = Vec::new();

        // 2) Üst şerit
        let down_limit = self.settings.down_kbps;
        let up_limit = self.settings.up_kbps;
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(22.0, 14.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(
                        self.logo_small.id(),
                        vec2(34.0, 34.0),
                    )));
                    ui.add_space(4.0);
                    ui.label(RichText::new("MiniTorrent").font(bold(21.0)).color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Gear, "Ayarlar").clicked() {
                            actions.push(Action::OpenSettings);
                        }
                        ui.add_space(4.0);
                        let up_txt = if up_limit > 0 {
                            format!("{}  /  {}", fmt_speed(tup), fmt_kb(up_limit))
                        } else {
                            fmt_speed(tup)
                        };
                        let down_txt = if down_limit > 0 {
                            format!("{}  /  {}", fmt_speed(tdown), fmt_kb(down_limit))
                        } else {
                            fmt_speed(tdown)
                        };
                        chip(ui, Icon::Up, &up_txt, GREEN);
                        chip(ui, Icon::Down, &down_txt, ACCENT);
                    });
                });
            });

        // 3) Ana alan
        let mut clicked: Option<u64> = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(Margin::symmetric(22.0, 18.0)),
            )
            .show(ctx, |ui| {
                // Ekleme kartı
                egui::Frame::none()
                    .fill(PANEL)
                    .rounding(14.0)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin::same(14.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if button(ui, "Ekle", Some(Icon::Plus), Btn::Primary).clicked() {
                                    actions.push(Action::SubmitInput);
                                }
                                if button(ui, "Panodan", Some(Icon::Clipboard), Btn::Secondary)
                                    .clicked()
                                {
                                    actions.push(Action::FromClipboard);
                                }
                                if button(ui, ".torrent dosyası", Some(Icon::File), Btn::Secondary)
                                    .clicked()
                                {
                                    actions.push(Action::PickTorrent);
                                }
                                let w = (ui.available_width() - 34.0).max(100.0);
                                let inner = egui::Frame::none()
                                    .fill(INPUT)
                                    .rounding(10.0)
                                    .stroke(Stroke::new(1.0, BORDER))
                                    .inner_margin(Margin::symmetric(12.0, 9.0))
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.input)
                                                .frame(false)
                                                .desired_width(w)
                                                .font(reg(14.5))
                                                .hint_text("magnet:?xt=urn:btih:…  linkini buraya yapıştır"),
                                        )
                                    });
                                let te = inner.inner;
                                if te.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    actions.push(Action::SubmitInput);
                                }
                            });
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            icon_space(ui, Icon::Folder, MUTED, 14.0);
                            ui.label(
                                RichText::new(shorten(&self.save_dir, 60))
                                    .color(MUTED)
                                    .size(12.5),
                            );
                            let ch = ui
                                .add(
                                    egui::Label::new(
                                        RichText::new("Değiştir").color(ACCENT).size(12.5),
                                    )
                                    .sense(Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if ch.clicked() {
                                actions.push(Action::ChooseDir);
                            }
                            if let Some((msg, t)) = &self.notice {
                                if t.elapsed() < Duration::from_secs(5) {
                                    ui.add_space(10.0);
                                    ui.label(RichText::new(msg).color(RED).size(12.5));
                                }
                            }
                        });
                    });

                ui.add_space(14.0);

                // Filtre sekmeleri
                ui.horizontal(|ui| {
                    let tabs = [
                        (Filter::All, "Tümü"),
                        (Filter::Active, "İndirilenler"),
                        (Filter::Done, "Tamamlanan"),
                        (Filter::Paused, "Duraklatılan"),
                    ];
                    for (f, label) in tabs {
                        let n = views.iter().filter(|v| f.matches(v.kind)).count();
                        if pill(ui, &format!("{label}  {n}"), self.filter == f).clicked() {
                            self.filter = f;
                        }
                    }
                });
                ui.add_space(8.0);

                // Liste
                let visible: Vec<&View> =
                    views.iter().filter(|v| self.filter.matches(v.kind)).collect();
                if views.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.12).max(8.0));
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(
                            self.logo_big.id(),
                            vec2(96.0, 96.0),
                        )));
                        ui.add_space(10.0);
                        ui.label(RichText::new("Henüz indirme yok").font(bold(21.0)).color(TEXT));
                        ui.label(
                            RichText::new(
                                "Magnet linki yapıştır ya da .torrent dosyasını pencereye sürükleyip bırak.",
                            )
                            .color(MUTED),
                        );
                        ui.add_space(12.0);
                        if button(ui, "Panodan magnet ekle", Some(Icon::Clipboard), Btn::Primary)
                            .clicked()
                        {
                            actions.push(Action::FromClipboard);
                        }
                    });
                } else if visible.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Bu sekmede torrent yok.").color(MUTED));
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for v in visible {
                                if draw_card(ui, v, self.selected == Some(v.uid), &mut actions) {
                                    clicked = Some(v.uid);
                                }
                                ui.add_space(4.0);
                            }
                        });
                }
            });

        // 4) Silme onayı
        if let Some(uid) = self.confirm {
            let name = views
                .iter()
                .find(|v| v.uid == uid)
                .map(|v| shorten(&v.name, 46))
                .unwrap_or_default();
            egui::Window::new("Torrent'i kaldır")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(380.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new(name).font(bold(15.0)).color(TEXT));
                    ui.label(RichText::new("İndirilen dosyalar da silinsin mi?").color(MUTED));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if button(ui, "İptal", None, Btn::Secondary).clicked() {
                            actions.push(Action::CancelConfirm);
                        }
                        if button(ui, "Sadece listeden", None, Btn::Secondary).clicked() {
                            actions.push(Action::Remove(uid, false));
                        }
                        if button(ui, "Dosyalarla sil", Some(Icon::Trash), Btn::Danger).clicked() {
                            actions.push(Action::Remove(uid, true));
                        }
                    });
                });
        }

        // 5) Ayarlar penceresi
        if self.settings_open {
            let tray_ok = self.tray_ok;
            egui::Window::new("Ayarlar")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(460.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new("Hız sınırları").font(bold(16.0)).color(TEXT));
                    ui.label(
                        RichText::new("0 yazarsan sınırsız olur. Kaydedince anında uygulanır.")
                            .color(MUTED)
                            .size(13.0),
                    );
                    ui.add_space(6.0);
                    limit_row(ui, "İndirme", Icon::Down, ACCENT, &mut self.edit_down);
                    ui.add_space(4.0);
                    limit_row(ui, "Yükleme", Icon::Up, GREEN, &mut self.edit_up);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(RichText::new("Pencere").font(bold(16.0)).color(TEXT));
                    ui.add_enabled(
                        tray_ok,
                        egui::Checkbox::new(
                            &mut self.edit_tray,
                            "Pencereyi kapatınca sistem tepsisine küçült",
                        ),
                    );
                    if !tray_ok {
                        ui.label(
                            RichText::new("Sistem tepsisi bu bilgisayarda kullanılamıyor.")
                                .color(MUTED)
                                .size(12.5),
                        );
                    }
                    if let Some(err) = &self.settings_err {
                        ui.add_space(4.0);
                        ui.label(RichText::new(err).color(RED).size(13.0));
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if button(ui, "İptal", None, Btn::Secondary).clicked() {
                            actions.push(Action::CloseSettings);
                        }
                        if button(ui, "Kaydet", Some(Icon::Check), Btn::Primary).clicked() {
                            actions.push(Action::SaveSettings);
                        }
                    });
                });
        }

        // 6) Sürükleme sırasında bırakma alanı göstergesi
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop_overlay"),
            ));
            let r = ctx.screen_rect();
            painter.rect_filled(r, 0.0, Color32::from_black_alpha(185));
            painter.rect_stroke(r.shrink(26.0), 18.0, Stroke::new(2.0, ACCENT));
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                ".torrent dosyasını buraya bırak",
                bold(26.0),
                TEXT,
            );
        }

        // 7) Eylemleri uygula
        if clicked.is_some() {
            self.selected = clicked;
        }
        for a in actions {
            match a {
                Action::SubmitInput => {
                    let t = self.input.clone();
                    if !t.trim().is_empty() && self.submit(&t) {
                        self.input.clear();
                    }
                }
                Action::FromClipboard => match read_clipboard() {
                    Some(t) => {
                        self.submit(&t);
                    }
                    None => self.notify("Panoda metin bulunamadı."),
                },
                Action::PickTorrent => {
                    if let Some(files) = rfd::FileDialog::new()
                        .add_filter("Torrent dosyası", &["torrent"])
                        .pick_files()
                    {
                        for f in files {
                            self.add_torrent_file(&f);
                        }
                    }
                }
                Action::ChooseDir => {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_directory(&self.save_dir)
                        .pick_folder()
                    {
                        self.save_dir = p.to_string_lossy().into_owned();
                    }
                }
                Action::Pause(u) => self.pause(u),
                Action::Resume(u) => self.resume(u),
                Action::AskRemove(u) => self.confirm = Some(u),
                Action::Remove(u, files) => {
                    self.confirm = None;
                    self.remove(u, files);
                }
                Action::CancelConfirm => self.confirm = None,
                Action::OpenFolder(p) => open_folder(&p),
                Action::OpenSettings => self.open_settings(),
                Action::SaveSettings => self.save_settings(),
                Action::CloseSettings => self.settings_open = false,
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let settings = Settings::load();
    let session = rt
        .block_on(Session::new_with_opts(
            PathBuf::from(default_download_dir()),
            SessionOptions {
                // Gelen bağlantılar + otomatik port yönlendirme: daha çok peer, daha hızlı indirme
                listen_port_range: Some(6881..6999),
                enable_upnp_port_forwarding: true,
                persistence: None, // liste kalıcılığını uygulama kendisi yönetiyor
                ..Default::default()
            },
        ))
        .expect("torrent oturumu başlatılamadı");
    apply_limits(&session, &settings);

    let (rgba, [w, h]) = load_rgba(include_bytes!("../assets/icon.png"));
    let icon = egui::IconData {
        rgba,
        width: w as u32,
        height: h as u32,
    };
    let (tray_rgba, [tw, th]) = load_rgba(include_bytes!("../assets/tray.png"));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 640.0])
            .with_min_inner_size([780.0, 460.0])
            .with_title("MiniTorrent")
            .with_icon(Arc::new(icon))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "MiniTorrent",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            let tray_ok = tray::init(tray_rgba, tw as u32, th as u32);
            Ok(Box::new(App::new(rt, session, &cc.egui_ctx, settings, tray_ok)))
        }),
    )
}
