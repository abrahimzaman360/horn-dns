//! Windows 10 "Metro" Start screen, built on GPUI Kit (https://gpui-kit.com).
//!
//! Layout
//!   • Tiles live on a grid of small cells. Small = 1×1, Medium = 2×2, Wide = 4×2, Large = 4×4.
//!   • The grid auto-fits the window: column count and cell size are recomputed from the
//!     window width on every frame (4 columns on a phone-sized window, more when it's wider),
//!     and fonts / icons scale with the cell size.
//!   • A first-fit packer flows tiles into the grid, so gaps are back-filled just like the
//!     real Start screen. Tiles glide to their new place / size (GPUI Base `transition`).
//!   • A selectable Metro wallpaper sits behind the tiles; tile ink flips between black and
//!     white based on the accent luminance so every label stays readable.
//!
//! Interactions
//!   • Live tiles flip on their own (staggered), clock ticks every second
//!   • Press a tile -> it squishes, release opens the app with a book-page turn
//!     (the outgoing page is peeled away, Windows Phone style)
//!   • Right-click a tile -> edit mode: ✕ unpins, ↔ cycles small → medium → wide → large
//!   • Small tiles are icon-only: captions, badges and live info are hidden
//!   • Long labels are clamped with an ellipsis, so tile text never spills out
//!   • In edit mode, DRAG a tile onto another tile to take its place; everything re-packs.
//!     Dropping on empty space sends it to the end.
//!   • Arrow at the end of the tiles -> All Apps: category headings, a search field at
//!     the top, rectangular icon borders (right-click an app to pin/unpin)
//!   • Settings tile -> theme + wallpaper + accent, tile gap and tile transparency,
//!     plus "Reset to Defaults"
//!   • The status bar never hides: it is the always-on top layer. Swipe down on its
//!     left half for notifications, on its right half for the system panel (quick
//!     toggles, brightness, accent). Clicking outside slides it back up.
//!   • Bottom bar: ← back (also leaves edit mode / closes the panel), ⊞ start, ⌕ apps
//!
//! Persistence
//!   • Tile order/sizes and every theme setting are saved as JSON and restored on
//!     the next launch. Writes are debounced, then flushed on shutdown.
//!   • Default path: `$XDG_CONFIG_HOME|~/.config/horn/config.json`, `%APPDATA%\horn\
//!     config.json` on Windows, `~/Library/Application Support/horn/config.json` on
//!     macOS. Set `HORN_CONFIG` to override it with an explicit file path.
use gpui_kit::assets::{Assets as ComponentAssets, IconName, icon_assets};
use gpui_kit::base::{Easing, Transition, transition};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::Duration;

// ───────────────────────────── assets ─────────────────────────────

// Kit's default source only embeds 101 icons; pick exactly the extra Lucide ones we use.
// (A misspelt / missing variant is a compile error, so swap for the nearest Lucide name.)
icon_assets!(
    ExtraIcons,
    [
        Phone,
        Users,
        MessageSquare,
        Mail,
        CalendarDays,
        Image,
        Music,
        ShoppingBag,
        Sun,
        Map,
        Globe,
        Clock,
        Settings,
        Gamepad2,
        Camera,
        StickyNote,
        ArrowLeft,
        ArrowRight,
        ArrowLeftRight,
        Search,
        X,
        ChevronUp,
        Wifi,
        WifiOff,
        Bluetooth,
        BluetoothOff,
        Plane,
        Bell,
        BellOff,
        Moon,
        Battery,
        BatteryCharging
    ]
);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        ComponentAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = ComponentAssets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

// ───────────────────────────── data ─────────────────────────────

const ACCENTS: [u32; 20] = [
    0xA4C400, 0x60A917, 0x008A00, 0x00ABA9, 0x1BA1E2, 0x0050EF, 0x6A00FF, 0xAA00FF, 0xF472D0,
    0xD80073, 0xA20025, 0xE51400, 0xFA6800, 0xF0A30A, 0xE3C800, 0x825A2C, 0x6D8764, 0x647687,
    0x76608A, 0x87794E,
];

// Layout constants (logical px)
const MARGIN: f32 = 20.0; // left/right page margin
const DEFAULT_GAP: f32 = 10.0; // default gap between tiles (user adjustable)
const GAP_MIN: f32 = 0.0;
const GAP_MAX: f32 = 28.0;
const TARGET_CELL: f32 = 78.0; // preferred small-cell size; actual size stretches to fill the width
const PANEL_PAD: f32 = 20.0; // panel inner margin
const STATUS_H: f32 = 26.0; // status bar height (always the top layer)
const TILE_ALPHA_MIN: f32 = 0.30; // most transparent a tile may get
const WINDOW_WIDTH: f32 = 476.0; // main (host) window width; height fills the screen

// Wallpaper presets: (top-left, bottom-right). Each theme gets its own set so the
// foreground text stays readable without a heavy scrim.
const WALLPAPERS_DARK: [(u32, u32); 5] = [
    (0x0B1B2B, 0x123B4C), // Dusk
    (0x0A0A0A, 0x262626), // Graphite
    (0x170B29, 0x2C1A46), // Plum
    (0x061F2B, 0x0B4F6C), // Ocean
    (0x2B1206, 0x6C2A0B), // Ember
];
const WALLPAPERS_LIGHT: [(u32, u32); 5] = [
    (0xE7EEF5, 0xBBD6E6), // Dusk
    (0xF2F2F2, 0xD6D6D6), // Graphite
    (0xF2EAF7, 0xD7C6E8), // Plum
    (0xE4F5FA, 0xB9DEEC), // Ocean
    (0xFCEFE4, 0xF2CDB4), // Ember
];
const WALLPAPER_NAMES: [&str; 5] = ["Dusk", "Graphite", "Plum", "Ocean", "Ember"];

/// WCAG relative luminance for an 0xRRGGBB colour.
fn relative_luminance(hex: u32) -> f32 {
    fn channel(c: u32) -> f32 {
        let c = c as f32 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel((hex >> 16) & 0xFF)
        + 0.7152 * channel((hex >> 8) & 0xFF)
        + 0.0722 * channel(hex & 0xFF)
}

/// Pick black or white text for a coloured surface, whichever has more contrast.
/// Keeps the Metro look (solid accent tiles) while fixing unreadable white-on-lime.
fn on_color(hex: u32) -> Hsla {
    if relative_luminance(hex) > 0.18 {
        black()
    } else {
        white()
    }
}

/// The opposite ink, for controls drawn on top of `on_color`.
fn inverse_ink(hex: u32) -> Hsla {
    if relative_luminance(hex) > 0.18 {
        white()
    } else {
        black()
    }
}

/// Pivot-style groups used by the All Apps list (declared order = display order).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Communication,
    Media,
    Productivity,
    Games,
    Tools,
    System,
}

impl Category {
    fn name(self) -> &'static str {
        match self {
            Category::Communication => "Communication",
            Category::Media => "Media",
            Category::Productivity => "Productivity",
            Category::Games => "Games",
            Category::Tools => "Tools",
            Category::System => "System",
        }
    }

    fn rank(self) -> usize {
        match self {
            Category::Communication => 0,
            Category::Media => 1,
            Category::Productivity => 2,
            Category::Games => 3,
            Category::Tools => 4,
            Category::System => 5,
        }
    }
}

struct AppDef {
    name: &'static str,
    icon: IconName,
    category: Category,
    /// None = follows the system accent colour
    color: Option<u32>,
}

const fn def(name: &'static str, icon: IconName, category: Category, color: Option<u32>) -> AppDef {
    AppDef {
        name,
        icon,
        category,
        color,
    }
}

static APPS: &[AppDef] = &[
    def("Phone", IconName::Phone, Category::Communication, None), // 0
    def("People", IconName::Users, Category::Communication, None), // 1
    def(
        "Messaging",
        IconName::MessageSquare,
        Category::Communication,
        None,
    ), // 2
    def("Mail", IconName::Mail, Category::Communication, None),   // 3
    def(
        "Calendar",
        IconName::CalendarDays,
        Category::Productivity,
        Some(0x0050EF),
    ), // 4
    def("Photos", IconName::Image, Category::Media, Some(0x008A00)), // 5
    def("Music", IconName::Music, Category::Media, Some(0xFA6800)), // 6
    def(
        "Store",
        IconName::ShoppingBag,
        Category::Tools,
        Some(0x60A917),
    ), // 7
    def("Weather", IconName::Sun, Category::Tools, Some(0x1BA1E2)), // 8
    def("Maps", IconName::Map, Category::Tools, Some(0xAA00FF)),  // 9
    def("Internet", IconName::Globe, Category::Tools, Some(0x0050EF)), // 10
    def("Clock", IconName::Clock, Category::Tools, Some(0x00ABA9)), // 11
    def(
        "Settings",
        IconName::Settings,
        Category::System,
        Some(0x647687),
    ), // 12
    def("Games", IconName::Gamepad2, Category::Games, Some(0x60A917)), // 13
    def("Camera", IconName::Camera, Category::Media, Some(0x76608A)), // 14
    def(
        "Notes",
        IconName::StickyNote,
        Category::Productivity,
        Some(0xF0A30A),
    ), // 15
];
const SETTINGS: usize = 12;

/// A notification shown in the panel pulled down from the left of the status bar.
struct Note {
    app: usize,
    title: &'static str,
    body: &'static str,
    when: &'static str,
}

static NOTES: &[Note] = &[
    Note {
        app: 2,
        title: "Sara · Messaging",
        body: "Hey! Did you try the new live tiles yet?",
        when: "now",
    },
    Note {
        app: 3,
        title: "Fishmind Labs · Mail",
        body: "Build 0.9 is ready for the private beta.",
        when: "12m",
    },
    Note {
        app: 4,
        title: "Design review · Calendar",
        body: "Starts at 4:30 PM in the Studio.",
        when: "1h",
    },
    Note {
        app: 7,
        title: "Store",
        body: "12 app updates are ready to install.",
        when: "3h",
    },
    Note {
        app: 8,
        title: "Weather",
        body: "Light rain expected around 6 PM.",
        when: "5h",
    },
];

// ───────────────────────────── persisted config ─────────────────────────────

/// Bumped when the on-disk shape needs migrating.
const CONFIG_VERSION: u32 = 1;

/// Everything that survives a restart. Stored as pretty JSON so it is easy to
/// hand-edit; unknown or broken values fall back to the defaults below.
#[derive(Serialize, Deserialize, Clone)]
struct Config {
    #[serde(default = "config_version")]
    version: u32,
    #[serde(default = "default_dark")]
    dark: bool,
    #[serde(default = "default_accent")]
    accent: u32,
    #[serde(default)]
    wallpaper: usize,
    #[serde(default = "default_gap")]
    gap: f32,
    #[serde(default = "default_alpha")]
    tile_alpha: f32,
    #[serde(default = "default_brightness")]
    brightness: f32,
    /// Start-screen tiles in display order; the first-fit packer places them.
    #[serde(default = "default_tiles")]
    tiles: Vec<TileConfig>,
}

/// App names (not indices) are stored, so the file stays readable and survives
/// the app list being re-ordered in a future build.
#[derive(Serialize, Deserialize, Clone)]
struct TileConfig {
    app: String,
    #[serde(default = "default_tile_size")]
    size: TileSize,
}

fn config_version() -> u32 {
    CONFIG_VERSION
}

fn default_dark() -> bool {
    true
}

fn default_accent() -> u32 {
    0xA4C400
}

fn default_gap() -> f32 {
    DEFAULT_GAP
}

fn default_alpha() -> f32 {
    1.0
}

fn default_brightness() -> f32 {
    1.0
}

fn default_tile_size() -> TileSize {
    TileSize::Medium
}

fn default_tiles() -> Vec<TileConfig> {
    use TileSize::*;
    [
        ("Phone", Medium),
        ("People", Medium),
        ("Messaging", Wide),
        ("Mail", Medium),
        ("Calendar", Medium),
        ("Weather", Wide),
        ("Clock", Medium),
        ("Music", Medium),
        ("Photos", Wide),
        ("Store", Medium),
        ("Maps", Small),
        ("Internet", Small),
        ("Camera", Small),
        ("Notes", Small),
        ("Settings", Medium),
        ("Games", Wide),
    ]
    .into_iter()
    .map(|(app, size)| TileConfig {
        app: app.to_string(),
        size,
    })
    .collect()
}

/// The live-tile faces an app starts with (also used when restoring a config).
fn default_faces(app: usize) -> Vec<Face> {
    use Face::*;
    match APPS.get(app).map(|a| a.name) {
        Some("People") => vec![
            Mosaic([0xD80073, 0x6A00FF, 0x00ABA9, 0xFA6800]),
            Text {
                head: "Sara",
                body: "is now on Numerals",
            },
        ],
        Some("Messaging") => vec![
            Icon,
            Text {
                head: "Numerals",
                body: "Hey! Did you try the new live tiles yet?",
            },
            Text {
                head: "Fishmind Labs",
                body: "Build 0.9 is ready for the private beta.",
            },
        ],
        Some("Mail") => vec![
            Icon,
            Text {
                head: "Fishmind Labs",
                body: "Weekly build report",
            },
        ],
        Some("Calendar") => vec![
            Big {
                value: "18",
                caption: "Wednesday",
            },
            Text {
                head: "Up next",
                body: "Design review · 4:30 PM",
            },
        ],
        Some("Photos") => vec![Mosaic([0x008A00, 0x1BA1E2, 0xF0A30A, 0xA20025]), Icon],
        Some("Music") => vec![
            Icon,
            Text {
                head: "Now playing",
                body: "Neon Rain — Fishmind",
            },
        ],
        Some("Store") => vec![
            Icon,
            Big {
                value: "12",
                caption: "updates",
            },
        ],
        Some("Weather") => vec![
            Big {
                value: "24°",
                caption: "Sunny",
            },
            Text {
                head: "Tomorrow",
                body: "22° · Partly cloudy",
            },
            Text {
                head: "Weekend",
                body: "Light rain on Saturday",
            },
        ],
        Some("Clock") => vec![Clock],
        _ => vec![Icon],
    }
}

fn default_badge(app: usize) -> Option<u32> {
    match APPS.get(app).map(|a| a.name) {
        Some("Phone") => Some(2),
        Some("Messaging") => Some(3),
        Some("Mail") => Some(12),
        _ => None,
    }
}

fn app_index(name: &str) -> Option<usize> {
    APPS.iter().position(|app| app.name == name)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            dark: default_dark(),
            accent: default_accent(),
            wallpaper: 0,
            gap: DEFAULT_GAP,
            tile_alpha: 1.0,
            brightness: 1.0,
            tiles: default_tiles(),
        }
    }
}

impl Config {
    /// Clamp hand-edited values and drop tiles that name an unknown app.
    fn sanitized(mut self) -> Self {
        self.wallpaper = self.wallpaper.min(WALLPAPER_NAMES.len() - 1);
        self.gap = self.gap.clamp(GAP_MIN, GAP_MAX);
        self.tile_alpha = self.tile_alpha.clamp(TILE_ALPHA_MIN, 1.0);
        self.brightness = self.brightness.clamp(0.05, 1.0);
        self.tiles.retain(|tile| app_index(&tile.app).is_some());
        if self.tiles.is_empty() {
            self.tiles = default_tiles();
        }
        self
    }
}

/// `$HORN_CONFIG` wins, then the usual per-OS user config directory:
/// `%APPDATA%\horn\config.json`, `~/Library/Application Support/horn/config.json`
/// or `$XDG_CONFIG_HOME|~/.config/horn/config.json`.
fn config_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HORN_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    Some(base.join("horn").join("config.json"))
}

fn load_config() -> Option<Config> {
    let path = config_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<Config>(&text) {
        Ok(config) => Some(config.sanitized()),
        Err(err) => {
            eprintln!("horn: ignoring unreadable {}: {err}", path.display());
            None
        }
    }
}

/// Write to a temp file and rename over the old one, so an interrupted save
/// can never leave a truncated config behind.
fn save_config(config: &Config) {
    let Some(path) = config_path() else { return };
    let Some(dir) = path.parent() else { return };
    if let Err(err) = std::fs::create_dir_all(dir) {
        eprintln!("horn: could not create {}: {err}", dir.display());
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(config) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(err) = std::fs::write(&tmp, json) {
        eprintln!("horn: could not write {}: {err}", tmp.display());
        return;
    }
    if let Err(err) = std::fs::rename(&tmp, &path) {
        eprintln!("horn: could not replace {}: {err}", path.display());
    }
}

/// Rebuild tiles from config entries, restoring stable ids and flip stagger.
fn tiles_from_config(configs: &[TileConfig]) -> Vec<Tile> {
    configs
        .iter()
        .filter_map(|tc| app_index(&tc.app).map(|app| (app, tc.size)))
        .enumerate()
        .map(|(i, (app, size))| {
            let mut tile = Tile::new(app, size, default_faces(app), default_badge(app));
            tile.id = i;
            tile.offset = (i as u64 * 5) % 6;
            tile
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TileSize {
    Small,
    Medium,
    Wide,
    Large,
}

impl TileSize {
    /// (columns, rows) in grid cells
    fn cells(self) -> (usize, usize) {
        match self {
            TileSize::Small => (1, 1),
            TileSize::Medium => (2, 2),
            TileSize::Wide => (4, 2),
            TileSize::Large => (4, 4),
        }
    }
    fn next(self) -> Self {
        match self {
            TileSize::Small => TileSize::Medium,
            TileSize::Medium => TileSize::Wide,
            TileSize::Wide => TileSize::Large,
            TileSize::Large => TileSize::Small,
        }
    }
}

#[derive(Clone)]
enum Face {
    Icon,
    Text {
        head: &'static str,
        body: &'static str,
    },
    Big {
        value: &'static str,
        caption: &'static str,
    },
    Mosaic([u32; 4]),
    Clock,
}

struct Tile {
    id: usize, // stable identity (indexes change when tiles are re-ordered)
    app: usize,
    size: TileSize,
    faces: Vec<Face>,
    badge: Option<u32>,
    idx: usize,
    prev: usize,
    tick: usize,
    offset: u64,
}

impl Tile {
    fn new(app: usize, size: TileSize, faces: Vec<Face>, badge: Option<u32>) -> Self {
        Tile {
            id: 0,
            app,
            size,
            faces,
            badge,
            idx: 0,
            prev: 0,
            tick: 0,
            offset: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Start,
    Apps,
    Settings,
    App(usize),
}

/// Which surface slides down from the status bar.
#[derive(Clone, Copy, PartialEq)]
enum PanelKind {
    /// Quick toggles, brightness, accent — pulled from the right half.
    System,
    /// Notification list — pulled from the left half.
    Notifications,
}

/// Slider currently being dragged (the pointer is tracked on the root).
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Brightness,
    Gap,
    Opacity,
}

/// Description of one labelled slider row.
struct Slider {
    id: &'static str,
    kind: Drag,
    min: &'static str,
    max: &'static str,
    value: String,
    ratio: f32,
}

/// Payload carried while a tile is being dragged. Also renders the drag "ghost".
#[derive(Clone)]
struct DraggedTile {
    id: usize,
    w: f32,
    h: f32,
    color: u32,
    icon: IconName,
}

impl Render for DraggedTile {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(self.w))
            .h(px(self.h))
            .bg(rgb(self.color))
            .opacity(0.85)
            .flex()
            .items_center()
            .justify_center()
            .child(glyph(
                self.icon,
                (self.h * 0.4).min(64.0),
                on_color(self.color),
            ))
    }
}

// ───────────────────────────── state ─────────────────────────────

struct Metro {
    tiles: Vec<Tile>,
    next_id: usize,
    accent: u32,
    dark: bool,
    page: Page,
    prev_page: Option<Page>,
    page_morph: bool,
    history: Vec<Page>,
    nav: usize,          // bumps on every navigation so the page-in animation replays
    edit: Option<usize>, // id of the tile in edit mode
    drop_handled: bool,
    time: String,
    date: String,
    secs: u64,
    // responsive layout, recomputed every frame from the *measured* content width
    measured_w: f32, // real laid-out width of the page area (0 until first measurement)
    cols: usize,
    cell: f32,
    scale: f32,
    last_w: f32,
    resizing: bool,
    // panels (swipe down from the status bar)
    panel: Option<PanelKind>,
    panel_closing: bool,
    panel_token: usize,
    pull_start: Option<(f32, f32)>,
    drag: Option<Drag>,
    brightness: f32,
    wifi: bool,
    bluetooth: bool,
    airplane: bool,
    quiet: bool,
    night_light: bool,
    battery_saver: bool,
    notes_cleared: bool,
    // appearance
    wallpaper: usize,
    gap: f32,
    tile_alpha: f32,
    // app search
    search: String,
    search_focus: FocusHandle,
    search_pending: bool,
    // debounced config persistence
    save_token: usize,
}

impl Metro {
    fn new(cx: &mut Context<Self>) -> Self {
        // Restore the last session; a missing or broken file means first run.
        let config = load_config().unwrap_or_default();
        let tiles = tiles_from_config(&config.tiles);
        let next_id = tiles.len();

        // 1 Hz heartbeat: clock + live-tile flips
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.heartbeat(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        let mut me = Metro {
            tiles,
            next_id,
            accent: config.accent,
            dark: config.dark,
            page: Page::Start,
            prev_page: None,
            page_morph: false,
            history: vec![],
            nav: 0,
            edit: None,
            drop_handled: false,
            time: String::new(),
            date: String::new(),
            secs: 0,
            measured_w: 0.0,
            cols: 4,
            cell: TARGET_CELL,
            scale: 1.0,
            last_w: 0.0,
            resizing: false,
            panel: None,
            panel_closing: false,
            panel_token: 0,
            pull_start: None,
            drag: None,
            brightness: config.brightness,
            wifi: true,
            bluetooth: false,
            airplane: false,
            quiet: false,
            night_light: false,
            battery_saver: false,
            notes_cleared: false,
            wallpaper: config.wallpaper,
            gap: config.gap,
            tile_alpha: config.tile_alpha,
            search: String::new(),
            search_focus: cx.focus_handle(),
            search_pending: false,
            save_token: 0,
        };
        me.stamp();
        me
    }

    /// Local wall-clock via jiff 1.x (system time zone).
    /// Note: jiff 1.0 renamed `Zoned` to `ZonedDateTime`.
    fn stamp(&mut self) {
        let now = jiff::ZonedDateTime::now();
        self.time = now.strftime("%-I:%M").to_string();
        self.date = now.strftime("%A, %B %-d").to_string();
    }

    fn heartbeat(&mut self, cx: &mut Context<Self>) {
        self.secs += 1;
        self.drop_handled = false;
        self.stamp();
        if self.edit.is_none() {
            let secs = self.secs;
            for t in self.tiles.iter_mut() {
                if t.faces.len() > 1 && (secs + t.offset) % 6 == 0 {
                    t.prev = t.idx;
                    t.idx = (t.idx + 1) % t.faces.len();
                    t.tick += 1;
                }
            }
        }
        cx.notify();
    }

    // ── responsive grid ──

    /// Pick an even column count (≥ 4) and a cell size so the grid exactly fills the window.
    fn relayout(&mut self, width: f32) {
        let gap = self.gap;
        let avail = (width - 2.0 * MARGIN).max(200.0);
        let cols =
            ((((avail + gap) / (TARGET_CELL + gap)) / 2.0).round() as usize * 2).clamp(4, 16);
        self.cell = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        self.cols = cols;
        self.scale = (self.cell / 70.0).clamp(0.85, 1.5);
        self.resizing = (width - self.last_w).abs() > 0.5;
        self.last_w = width;
    }

    /// First-fit packer: every tile goes to the first free spot (row-major),
    /// so smaller tiles back-fill holes. Returns (col, row) per tile and the row count.
    fn pack(&self) -> (Vec<(usize, usize)>, usize) {
        let cols = self.cols;
        let mut occ: Vec<Vec<bool>> = Vec::new();
        let mut out = Vec::with_capacity(self.tiles.len());
        for t in &self.tiles {
            let (cw, ch) = t.size.cells();
            let cw = cw.min(cols);
            let mut spot = None;
            let mut r = 0;
            while spot.is_none() {
                while occ.len() < r + ch {
                    occ.push(vec![false; cols]);
                }
                for c in 0..=(cols - cw) {
                    if (r..r + ch).all(|rr| (c..c + cw).all(|cc| !occ[rr][cc])) {
                        spot = Some((c, r));
                        break;
                    }
                }
                if spot.is_none() {
                    r += 1;
                }
            }
            let (c, r) = spot.unwrap();
            for rr in r..r + ch {
                for cc in c..c + cw {
                    occ[rr][cc] = true;
                }
            }
            out.push((c, r));
        }
        let rows = occ
            .iter()
            .rposition(|row| row.iter().any(|&b| b))
            .map(|i| i + 1)
            .unwrap_or(0);
        (out, rows)
    }

    /// Animated value for one tile channel. While the window itself is being resized the
    /// tween is effectively instant, so the grid tracks the window edge without lag.
    fn tween(
        &self,
        id: usize,
        key: &'static str,
        target: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> f32 {
        let eid: ElementId = ("tile-motion", id).into();
        let ms = if self.resizing { 1 } else { 280 };
        transition(
            (eid, key),
            target,
            Transition::new(Duration::from_millis(ms)).easing(Easing::EaseOut),
            window,
            cx,
        )
    }

    /// Drag-to-replace: the dragged tile takes the target's slot, the rest shift and re-pack.
    fn move_tile(&mut self, from_id: usize, to_id: usize, cx: &mut Context<Self>) {
        let (Some(from), Some(to)) = (
            self.tiles.iter().position(|t| t.id == from_id),
            self.tiles.iter().position(|t| t.id == to_id),
        ) else {
            return;
        };
        if from != to {
            let t = self.tiles.remove(from);
            self.tiles.insert(to, t);
        }
        self.save_soon(cx);
        cx.notify();
    }

    fn move_to_end(&mut self, from_id: usize, cx: &mut Context<Self>) {
        if let Some(from) = self.tiles.iter().position(|t| t.id == from_id) {
            let t = self.tiles.remove(from);
            self.tiles.push(t);
        }
        self.save_soon(cx);
        cx.notify();
    }

    // ── persistence ──

    /// Snapshot of everything worth keeping between runs.
    fn config(&self) -> Config {
        Config {
            version: CONFIG_VERSION,
            dark: self.dark,
            accent: self.accent,
            wallpaper: self.wallpaper,
            gap: self.gap,
            tile_alpha: self.tile_alpha,
            brightness: self.brightness,
            tiles: self
                .tiles
                .iter()
                .map(|t| TileConfig {
                    app: APPS[t.app].name.to_string(),
                    size: t.size,
                })
                .collect(),
        }
    }

    /// Debounced save: slider drags and rapid edits collapse into one write.
    fn save_soon(&mut self, cx: &mut Context<Self>) {
        self.save_token = self.save_token.wrapping_add(1);
        let token = self.save_token;
        let config = self.config();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let still_current = this
                .update(cx, |this, _| this.save_token == token)
                .unwrap_or(false);
            if still_current {
                save_config(&config);
            }
        })
        .detach();
    }

    fn rebuild_tiles(&mut self, configs: &[TileConfig]) {
        self.tiles = tiles_from_config(configs);
        self.next_id = self.tiles.len();
        self.edit = None;
    }

    fn reset_config(&mut self, cx: &mut Context<Self>) {
        let defaults = Config::default();
        self.dark = defaults.dark;
        self.accent = defaults.accent;
        self.wallpaper = defaults.wallpaper;
        self.gap = defaults.gap;
        self.tile_alpha = defaults.tile_alpha;
        self.brightness = defaults.brightness;
        self.rebuild_tiles(&defaults.tiles);
        self.save_soon(cx);
        cx.notify();
    }

    // ── navigation ──

    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page == page {
            return;
        }
        self.history.push(self.page);
        self.flip_to(page, cx);
    }

    /// Swap pages with the book-page (Windows Phone) reveal: the outgoing page is
    /// kept alive for the duration of the peel, then dropped.
    fn flip_to(&mut self, page: Page, cx: &mut Context<Self>) {
        self.prev_page = Some(self.page);
        self.page = page;
        self.nav += 1;
        self.edit = None;
        self.page_morph = true;
        if page == Page::Apps {
            self.search_pending = true;
        }
        let token = self.nav;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(460))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.nav == token {
                    this.page_morph = false;
                    this.prev_page = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if self.panel.is_some() {
            self.close_panel(cx);
        } else if self.edit.is_some() {
            self.edit = None;
            cx.notify();
        } else if self.page != Page::Start {
            let prev = self.history.pop().unwrap_or(Page::Start);
            self.flip_to(prev, cx);
        } else {
            cx.notify();
        }
    }

    fn home(&mut self, cx: &mut Context<Self>) {
        self.history.clear();
        if self.page != Page::Start {
            self.flip_to(Page::Start, cx);
        } else {
            self.edit = None;
            cx.notify();
        }
    }

    fn open_app(&mut self, app: usize, cx: &mut Context<Self>) {
        // Dismiss any panel immediately; the page flip covers the switch.
        self.panel = None;
        self.panel_closing = false;
        self.panel_token += 1;
        self.go(
            if app == SETTINGS {
                Page::Settings
            } else {
                Page::App(app)
            },
            cx,
        );
    }

    fn toggle_pin(&mut self, app: usize, cx: &mut Context<Self>) {
        if let Some(pos) = self.tiles.iter().position(|t| t.app == app) {
            self.tiles.remove(pos);
        } else {
            let mut t = Tile::new(
                app,
                TileSize::Medium,
                default_faces(app),
                default_badge(app),
            );
            t.id = self.next_id;
            self.next_id += 1;
            t.offset = self.secs;
            self.tiles.push(t);
        }
        self.save_soon(cx);
        cx.notify();
    }

    // ── panels ──

    fn open_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
        self.panel = Some(kind);
        self.panel_closing = false;
        self.panel_token += 1;
        self.edit = None;
        self.pull_start = None;
        cx.notify();
    }

    /// Dismiss with the slide-up animation; the panel stays mounted until it has
    /// travelled out of view, so the exit is animated instead of popping away.
    fn close_panel(&mut self, cx: &mut Context<Self>) {
        if self.panel.is_none() || self.panel_closing {
            return;
        }
        self.panel_closing = true;
        self.pull_start = None;
        self.drag = None;
        self.panel_token += 1;
        let token = self.panel_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(360))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.panel_token == token {
                    this.panel = None;
                    this.panel_closing = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Left half of the status bar = notifications, right half = system panel.
    fn panel_for_x(&self, x: f32) -> PanelKind {
        if x < self.measured_w * 0.5 {
            PanelKind::Notifications
        } else {
            PanelKind::System
        }
    }

    fn toggle_panel(&mut self, x: f32, cx: &mut Context<Self>) {
        let kind = self.panel_for_x(x);
        if self.panel == Some(kind) {
            self.close_panel(cx);
        } else {
            self.open_panel(kind, cx);
        }
    }

    // ── sliders ──

    /// Map a pointer x-position onto the slider being dragged. Settings sliders
    /// live in a centred, max-width column; the panel slider spans the panel.
    fn apply_drag(&mut self, kind: Drag, x: f32) {
        let (left, track) = match kind {
            Drag::Brightness => (
                PANEL_PAD + 34.0,
                (self.measured_w - 2.0 * PANEL_PAD - 34.0 - 92.0).max(1.0),
            ),
            Drag::Gap | Drag::Opacity => {
                let avail = (self.measured_w - 2.0 * MARGIN).max(0.0);
                let column = avail.min(560.0);
                let offset = MARGIN + (avail - column) * 0.5;
                (offset + 34.0, (column - 34.0 - 92.0).max(1.0))
            }
        };
        let t = ((x - left) / track).clamp(0.0, 1.0);
        match kind {
            Drag::Brightness => self.brightness = 0.05 + t * 0.95,
            Drag::Gap => self.gap = GAP_MIN + t * (GAP_MAX - GAP_MIN),
            Drag::Opacity => self.tile_alpha = TILE_ALPHA_MIN + t * (1.0 - TILE_ALPHA_MIN),
        }
    }

    fn toggle_wifi(&mut self) {
        self.wifi = !self.wifi;
        if self.wifi {
            self.airplane = false;
        }
    }

    fn toggle_bluetooth(&mut self) {
        self.bluetooth = !self.bluetooth;
        if self.bluetooth {
            self.airplane = false;
        }
    }

    fn toggle_airplane(&mut self) {
        self.airplane = !self.airplane;
        if self.airplane {
            self.wifi = false;
            self.bluetooth = false;
        }
    }

    fn quick_action(
        &self,
        id: &'static str,
        name: &'static str,
        icon: IconName,
        active: bool,
        toggle: fn(&mut Self),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let ink = if active {
            on_color(self.accent)
        } else {
            self.fg()
        };
        let bg: Hsla = if active {
            rgb(self.accent).into()
        } else {
            self.cell_bg()
        };
        div()
            .id(id)
            .flex_1()
            .min_w_0()
            .h(px(68.0))
            .p(px(10.0))
            .flex()
            .flex_col()
            .justify_between()
            .items_start()
            .bg(bg)
            .rounded(px(2.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.7))
            .on_click(cx.listener(move |this, _, _, cx| {
                toggle(this);
                cx.notify();
            }))
            .child(glyph(icon, 21.0, ink))
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(13.0))
                    .text_color(ink)
                    .child(name),
            )
    }

    /// One labelled slider row: min label, track, value, max label.
    fn slider(&self, spec: Slider, cx: &mut Context<Self>) -> Div {
        let Slider {
            id,
            kind,
            min,
            max,
            value,
            ratio,
        } = spec;
        let fg = self.fg();
        let accent: Hsla = rgb(self.accent).into();
        let ratio = ratio.clamp(0.0, 1.0);
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .w(px(24.0))
                    .text_size(px(10.0))
                    .text_color(fg.opacity(0.55))
                    .child(min),
            )
            .child(
                div()
                    .id(id)
                    .relative()
                    .flex_1()
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            this.drag = Some(kind);
                            this.apply_drag(kind, f32::from(e.position.x));
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(4.0))
                            .bg(self.cell_bg())
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(0.0))
                                    .h_full()
                                    .w(relative(ratio))
                                    .bg(accent)
                                    .child(
                                        div()
                                            .absolute()
                                            .right(px(-8.0))
                                            .top(px(-6.0))
                                            .size(px(16.0))
                                            .rounded_full()
                                            .bg(accent),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .w(px(44.0))
                    .text_size(px(11.0))
                    .text_color(fg.opacity(0.75))
                    .child(value),
            )
            .child(
                div()
                    .w(px(28.0))
                    .text_size(px(10.0))
                    .text_color(fg.opacity(0.5))
                    .child(max),
            )
    }

    fn panel_heading(
        &self,
        overline: &'static str,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let fg = self.fg();
        let close = div()
            .id("panel-close")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| this.close_panel(cx)))
            .child(glyph(IconName::ChevronUp, 20.0, fg));

        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg.opacity(0.55))
                            .child(overline),
                    )
                    .child(
                        div()
                            .text_size(px(28.0))
                            .line_height(px(32.0))
                            .font_weight(FontWeight::LIGHT)
                            .child(title),
                    ),
            )
            .child(close)
    }

    /// System panel: quick toggles, brightness, accent + theme shortcuts.
    fn system_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let fg = self.fg();
        let caption = |text: &'static str| {
            div()
                .mt(px(4.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg.opacity(0.55))
                .child(text)
        };

        let row1 = div()
            .flex()
            .gap(px(8.0))
            .child(self.quick_action(
                "qa-wifi",
                "Wi-Fi",
                if self.wifi {
                    IconName::Wifi
                } else {
                    IconName::WifiOff
                },
                self.wifi,
                Self::toggle_wifi,
                cx,
            ))
            .child(self.quick_action(
                "qa-bt",
                "Bluetooth",
                if self.bluetooth {
                    IconName::Bluetooth
                } else {
                    IconName::BluetoothOff
                },
                self.bluetooth,
                Self::toggle_bluetooth,
                cx,
            ))
            .child(self.quick_action(
                "qa-plane",
                "Airplane mode",
                IconName::Plane,
                self.airplane,
                Self::toggle_airplane,
                cx,
            ));

        let row2 = div()
            .flex()
            .gap(px(8.0))
            .child(self.quick_action(
                "qa-quiet",
                "Quiet hours",
                if self.quiet {
                    IconName::BellOff
                } else {
                    IconName::Bell
                },
                self.quiet,
                |this| this.quiet = !this.quiet,
                cx,
            ))
            .child(self.quick_action(
                "qa-night",
                "Night light",
                IconName::Moon,
                self.night_light,
                |this| this.night_light = !this.night_light,
                cx,
            ))
            .child(self.quick_action(
                "qa-battery",
                "Battery saver",
                if self.battery_saver {
                    IconName::BatteryCharging
                } else {
                    IconName::Battery
                },
                self.battery_saver,
                |this| this.battery_saver = !this.battery_saver,
                cx,
            ));

        let brightness = self.slider(
            Slider {
                id: "brightness-track",
                kind: Drag::Brightness,
                min: "0",
                max: "100",
                value: format!("{}%", (self.brightness * 100.0).round() as u32),
                ratio: self.brightness,
            },
            cx,
        );

        let swatches: Vec<Stateful<Div>> = ACCENTS
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, &c)| {
                div()
                    .id(("panel-accent", i))
                    .size(px(26.0))
                    .rounded(px(2.0))
                    .bg(rgb(c))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .when(self.accent == c, |d| d.border_2().border_color(fg))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.accent = c;
                        this.save_soon(cx);
                        cx.notify();
                    }))
            })
            .collect();

        let panel_button = |id: &'static str, icon: IconName, text: &'static str| {
            div()
                .id(id)
                .flex_1()
                .h(px(40.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .bg(self.cell_bg())
                .rounded(px(2.0))
                .cursor_pointer()
                .active(|s| s.opacity(0.65))
                .child(glyph(icon, 17.0, fg))
                .child(div().text_size(px(12.0)).text_color(fg).child(text))
        };

        let theme_btn = panel_button(
            "panel-theme",
            if self.dark {
                IconName::Sun
            } else {
                IconName::Moon
            },
            if self.dark { "Light mode" } else { "Dark mode" },
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.dark = !this.dark;
            this.save_soon(cx);
            cx.notify();
        }));

        let settings_btn = panel_button("panel-settings", IconName::Settings, "All settings")
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_panel(cx);
                this.go(Page::Settings, cx);
            }));

        div()
            .id("panel-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(PANEL_PAD))
            .pt(px(STATUS_H + 10.0))
            .pb(px(PANEL_PAD))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.panel_heading("Action Center", "Control Panel", cx))
            .child(caption("Quick Actions"))
            .child(row1)
            .child(row2)
            .child(caption("Brightness"))
            .child(brightness)
            .child(caption("Accent"))
            .child(div().flex().flex_wrap().gap(px(6.0)).children(swatches))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(theme_btn)
                    .child(settings_btn),
            )
    }

    /// Notification panel: swipe down from the left half of the status bar.
    fn notifications_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let fg = self.fg();

        let clear = div()
            .id("notes-clear")
            .px(px(12.0))
            .h(px(30.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .bg(self.cell_bg())
            .rounded(px(2.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .child(glyph(IconName::X, 13.0, fg.opacity(0.75)))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(fg.opacity(0.8))
                    .child("Clear all"),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.notes_cleared = true;
                cx.notify();
            }));

        let body: Div = if self.notes_cleared {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .child(glyph(IconName::BellOff, 30.0, fg.opacity(0.35)))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(fg.opacity(0.55))
                        .child("You're all caught up"),
                )
        } else {
            let mut rows: Vec<AnyElement> = Vec::with_capacity(NOTES.len());
            for (i, n) in NOTES.iter().enumerate() {
                rows.push(self.note_row(n, i, cx).into_any_element());
            }
            div().flex().flex_col().children(rows)
        };

        div()
            .id("notes-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(PANEL_PAD))
            .pt(px(STATUS_H + 10.0))
            .pb(px(PANEL_PAD))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.panel_heading("Notifications", "Notifications", cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg.opacity(0.55))
                            .child(if self.notes_cleared {
                                "No new notifications"
                            } else {
                                "New in the last 24 hours"
                            }),
                    )
                    .child(clear),
            )
            .child(body)
            .child(div().flex_1())
    }

    fn note_row(&self, n: &Note, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let a = &APPS[n.app];
        let hex = a.color.unwrap_or(self.accent);
        let app = n.app;
        div()
            .id(("note", i))
            .flex()
            .items_start()
            .gap(px(10.0))
            .py(px(8.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| this.open_app(app, cx)))
            .child(
                div()
                    .size(px(34.0))
                    .rounded(px(3.0))
                    .bg(rgb(hex))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(a.icon, 17.0, on_color(hex))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(n.title),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(fg.opacity(0.5))
                                    .child(n.when),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(15.0))
                            .text_color(fg.opacity(0.72))
                            .child(n.body),
                    ),
            )
    }

    /// The sliding surface itself. It is mounted while a panel is open (or still
    /// animating out) and driven by a reversible `transition`, so both the
    /// slide-down and the dismiss slide-up are smooth and follow live values.
    fn panel_layer(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let viewport_h = f32::from(window.viewport_size().height);
        let panel_h = (viewport_h * 0.66).clamp(320.0, 560.0);
        let open = self.panel.is_some() && !self.panel_closing;

        let slide = transition(
            ("panel", "slide"),
            if open { panel_h } else { 0.0 },
            Transition::new(Duration::from_millis(320)).easing(Easing::EaseInOut),
            window,
            cx,
        );
        let scrim = transition(
            ("panel", "scrim"),
            if open { 1.0f32 } else { 0.0 },
            Transition::new(Duration::from_millis(260)).easing(Easing::EaseInOut),
            window,
            cx,
        );

        if slide < 0.5 {
            return None;
        }
        let slide = slide.clamp(0.0, panel_h);
        let top = slide - panel_h;

        let scrim = div()
            .id("panel-scrim")
            .absolute()
            .left(px(0.0))
            .top(px(slide))
            .w_full()
            .h(px((viewport_h - slide).max(0.0)))
            .bg(hsla(
                0.0,
                0.0,
                0.0,
                scrim * if self.dark { 0.55 } else { 0.3 },
            ))
            .occlude()
            .when(open, |d| {
                d.on_click(cx.listener(|this, _, _, cx| this.close_panel(cx)))
            });

        let content = match self.panel {
            Some(PanelKind::Notifications) => self.notifications_panel(cx),
            _ => self.system_panel(cx),
        };

        let panel = div()
            .id("panel")
            .absolute()
            .left(px(0.0))
            .top(px(top))
            .w_full()
            .h(px(panel_h))
            .bg(self.panel_bg())
            .border_b_1()
            .border_color(self.fg().opacity(0.12))
            .shadow_lg()
            .occlude()
            .child(content);

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .child(scrim)
                .child(panel)
                .into_any_element(),
        )
    }

    // ── wallpaper ──

    fn wallpaper_pair(&self, i: usize) -> (u32, u32) {
        let list = if self.dark {
            &WALLPAPERS_DARK
        } else {
            &WALLPAPERS_LIGHT
        };
        list[i.min(list.len() - 1)]
    }

    fn wallpaper_colors(&self) -> (u32, u32) {
        self.wallpaper_pair(self.wallpaper)
    }

    fn wallpaper_view(&self) -> Div {
        let (from, to) = self.wallpaper_colors();
        let accent: Hsla = rgb(self.accent).into();
        let line = self.fg().opacity(0.05);
        let line_faint = self.fg().opacity(0.025);

        div()
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .overflow_hidden()
            .bg(linear_gradient(
                135.0,
                linear_color_stop(rgb(from), 0.0),
                linear_color_stop(rgb(to), 1.0),
            ))
            .child(div().absolute().size_full().bg(linear_gradient(
                135.0,
                linear_color_stop(accent.opacity(0.0), 0.3),
                linear_color_stop(accent.opacity(if self.dark { 0.22 } else { 0.16 }), 1.0),
            )))
            .child(
                div()
                    .absolute()
                    .top(px(-150.0))
                    .right(px(-120.0))
                    .size(px(430.0))
                    .rounded_full()
                    .bg(accent)
                    .opacity(if self.dark { 0.09 } else { 0.07 }),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(-180.0))
                    .left(px(-140.0))
                    .size(px(380.0))
                    .rounded_full()
                    .bg(accent)
                    .opacity(if self.dark { 0.06 } else { 0.05 }),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(30.0))
                    .right(px(-70.0))
                    .size(px(220.0))
                    .rounded_full()
                    .border_1()
                    .border_color(self.fg().opacity(0.06)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(120.0))
                    .left(px(-60.0))
                    .w(px(560.0))
                    .h(px(1.0))
                    .bg(line),
            )
            .child(
                div()
                    .absolute()
                    .top(px(168.0))
                    .left(px(-60.0))
                    .w(px(560.0))
                    .h(px(1.0))
                    .bg(line_faint),
            )
    }

    // ── theme ──

    fn fg(&self) -> Hsla {
        if self.dark { white() } else { black() }
    }
    /// Opaque-ish backdrop for list / settings pages so text stays readable
    /// while a hint of the wallpaper still shows through.
    fn page_bg(&self) -> Hsla {
        if self.dark {
            hsla(0.0, 0.0, 0.06, 0.95)
        } else {
            hsla(0.0, 0.0, 1.0, 0.95)
        }
    }
    fn panel_bg(&self) -> Hsla {
        if self.dark {
            hsla(0.0, 0.0, 0.10, 1.0)
        } else {
            hsla(0.0, 0.0, 0.97, 1.0)
        }
    }
    fn cell_bg(&self) -> Hsla {
        if self.dark {
            hsla(0.0, 0.0, 0.18, 1.0)
        } else {
            hsla(0.0, 0.0, 0.88, 1.0)
        }
    }
}

// ───────────────────────────── view helpers ─────────────────────────────

fn label(name: &'static str, s: f32) -> Div {
    div()
        .absolute()
        .left(px(10.0 * s))
        .right(px(8.0 * s))
        .bottom(px(8.0 * s))
        .text_size(px(13.0 * s))
        .line_height(px(15.0 * s))
        .whitespace_nowrap()
        .text_ellipsis()
        .child(name)
}

fn glyph(icon: IconName, size: f32, color: Hsla) -> Icon {
    Icon::new(icon).w(px(size)).h(px(size)).text_color(color)
}

fn pivot_header(small: String, big: String) -> Div {
    div()
        .pt(px(8.0))
        .pb(px(12.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(small),
        )
        .child(
            div()
                .text_size(px(54.0))
                .line_height(px(60.0))
                .font_weight(FontWeight::LIGHT)
                .child(big),
        )
}

impl Metro {
    /// Always painted above everything else (see `render`): it never hides on its
    /// own and stays visible while a panel is slid down underneath it.
    fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let fg = self.fg();
        div()
            .id("status-bar")
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .w_full()
            .h(px(STATUS_H))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(12.0))
            .text_color(fg)
            .bg(if self.dark {
                hsla(0.0, 0.0, 0.0, 0.30)
            } else {
                hsla(0.0, 0.0, 1.0, 0.40)
            })
            .cursor_pointer()
            // Press starts a potential pull; a short downward drag opens the
            // panel for the half of the bar the gesture began on.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    this.pull_start = Some((f32::from(e.position.x), f32::from(e.position.y)));
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, e: &MouseUpEvent, _, cx| {
                    if this.pull_start.take().is_some() {
                        this.toggle_panel(f32::from(e.position.x), cx);
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(2.0))
                    .children((0..4).map(|i| div().w(px(3.0)).h(px(4.0 + 3.0 * i as f32)).bg(fg))),
            )
            .child(
                div()
                    .w(px(44.0))
                    .h(px(3.0))
                    .rounded_full()
                    .bg(fg.opacity(0.55)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(18.0))
                            .h(px(9.0))
                            .border_1()
                            .border_color(fg)
                            .p(px(1.0))
                            .child(div().w(px(12.0)).h_full().bg(fg)),
                    )
                    .child(self.time.clone()),
            )
            .into_any_element()
    }

    fn nav_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let btn = |id: &'static str| {
            div()
                .id(id)
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .active(|s| s.opacity(0.4))
        };
        let logo = div()
            .flex()
            .flex_wrap()
            .w(px(20.0))
            .gap(px(2.0))
            .children((0..4).map(|_| div().size(px(9.0)).bg(fg)));

        div()
            .h(px(52.0))
            .flex()
            .items_center()
            .bg(if self.dark {
                black()
            } else {
                rgb(0xE8E8E8).into()
            })
            .text_color(fg)
            .child(
                btn("nav-back")
                    .child(glyph(IconName::ArrowLeft, 26.0, fg))
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
            )
            .child(
                btn("nav-start")
                    .child(logo)
                    .on_click(cx.listener(|this, _, _, cx| this.home(cx))),
            )
            .child(
                btn("nav-search")
                    .child(glyph(IconName::Search, 24.0, fg))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.page == Page::Apps {
                            // Already there: put the caret back in the search field.
                            this.search_pending = true;
                            cx.notify();
                        } else {
                            this.go(Page::Apps, cx);
                        }
                    })),
            )
    }

    // ── tiles ──

    fn face(&self, face: &Face, t: &Tile, ink: Hsla) -> Div {
        let app = &APPS[t.app];
        let s = self.scale;
        let small = t.size == TileSize::Small;
        let base = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .overflow_hidden()
            .text_color(ink);

        match face {
            Face::Icon => {
                let icon_px = match t.size {
                    TileSize::Small => 34.0,
                    TileSize::Medium | TileSize::Wide => 56.0,
                    TileSize::Large => 88.0,
                } * s;
                base.flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(app.icon, icon_px, ink))
                    .when(!small, |d| d.child(label(app.name, s)))
                    .when_some(t.badge.filter(|_| !small), |d, n| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(10.0 * s))
                                .bottom(px(6.0 * s))
                                .text_size(px(24.0 * s))
                                .font_weight(FontWeight::LIGHT)
                                .child(n.to_string()),
                        )
                    })
            }
            Face::Text { head, body } => base
                .p(px(10.0 * s))
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .child(
                    div()
                        .text_size(px(17.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(*head),
                )
                .child(
                    div()
                        .text_size(px(13.0 * s))
                        .line_height(px(16.0 * s))
                        .text_color(ink.opacity(0.82))
                        .child(*body),
                )
                .child(label(app.name, s)),
            Face::Big { value, caption } => base
                .child(
                    div()
                        .absolute()
                        .left(px(10.0 * s))
                        .right(px(10.0 * s))
                        .top(px(16.0 * s))
                        .text_size(px((56.0 * s).min(64.0)))
                        .line_height(px((60.0 * s).min(68.0)))
                        .font_weight(FontWeight::LIGHT)
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(*value),
                )
                .child(label(caption, s)),
            Face::Mosaic(cols) => {
                // (count, columns, rows) of colour blocks for this tile shape
                let (n, c, r) = match t.size {
                    TileSize::Small | TileSize::Medium => (4, 2, 2),
                    TileSize::Wide => (8, 4, 2),
                    TileSize::Large => (16, 4, 4),
                };
                // The caption sits over the bottom-left block; use that block's ink.
                let caption_ink = on_color(cols[((r - 1) * c) % 4]);
                base.flex()
                    .flex_wrap()
                    .children((0..n).map(|i| {
                        div()
                            .w(relative(1.0 / c as f32))
                            .h(relative(1.0 / r as f32))
                            .bg(rgb(cols[i % 4]))
                    }))
                    .child(label(app.name, s).text_color(caption_ink))
            }
            Face::Clock => {
                // The clock owns a 2×2 tile; keep the type small enough that the
                // time and the (long) date line can never spill past the window.
                base.flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(10.0 * s))
                    .child(
                        div()
                            .w_full()
                            .text_size(px((32.0 * s).min(40.0)))
                            .line_height(px((36.0 * s).min(44.0)))
                            .font_weight(FontWeight::LIGHT)
                            .text_center()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(self.time.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .mt(px(2.0 * s))
                            .text_size(px((10.0 * s).min(13.0)))
                            .line_height(px((13.0 * s).min(16.0)))
                            .text_color(ink.opacity(0.8))
                            .text_center()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(self.date.clone()),
                    )
                    .child(label(app.name, s))
            }
        }
    }

    /// Front face, or (old face sliding up + new face sliding in from below).
    /// The wrapper ID includes `t.tick`, so every flip replays a fresh one-shot animation.
    fn tile_content(&self, t: &Tile, h: f32) -> AnyElement {
        let hex = APPS[t.app].color.unwrap_or(self.accent);
        let ink = on_color(hex);

        // Small tiles are icon-only: no captions, badges or live info.
        if t.size == TileSize::Small {
            return self.face(&Face::Icon, t, ink).into_any_element();
        }

        if t.faces.len() > 1 && t.tick > 0 {
            let anim = || Animation::new(Duration::from_millis(650)).with_easing(ease_out_quint());
            let out = self.face(&t.faces[t.prev], t, ink).with_animation(
                ("face-out", t.tick),
                anim(),
                move |el, d| el.top(px(-h * d)),
            );
            let inn = self.face(&t.faces[t.idx], t, ink).with_animation(
                ("face-in", t.tick),
                anim(),
                move |el, d| el.top(px(h * (1.0 - d))),
            );
            div()
                .relative()
                .size_full()
                .child(out)
                .child(inn)
                .into_any_element()
        } else {
            self.face(&t.faces[t.idx], t, ink).into_any_element()
        }
    }

    fn tile_view(
        &self,
        ix: usize,
        pos: (usize, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = &self.tiles[ix];
        let id = t.id;
        let (cw, ch) = t.size.cells();
        let (cell, g) = (self.cell, self.gap);

        // target geometry from the packer …
        let tx = pos.0 as f32 * (cell + g);
        let ty = pos.1 as f32 * (cell + g);
        let tw = cw as f32 * cell + (cw as f32 - 1.0) * g;
        let th = ch as f32 * cell + (ch as f32 - 1.0) * g;
        // … and the animated values actually painted this frame
        let x = self.tween(id, "x", tx, window, cx);
        let y = self.tween(id, "y", ty, window, cx);
        let w = self.tween(id, "w", tw, window, cx);
        let h = self.tween(id, "h", th, window, cx);

        let editing = self.edit == Some(id);
        let color_hex = APPS[t.app].color.unwrap_or(self.accent);
        let color = rgb(color_hex).opacity(self.tile_alpha);
        let ink = on_color(color_hex);
        let inverse = inverse_ink(color_hex);
        let content = self.tile_content(t, h);
        let payload = DraggedTile {
            id,
            w,
            h,
            color: color_hex,
            icon: APPS[t.app].icon,
        };

        let circle_btn = |bid: &'static str, icon: IconName| {
            div()
                .id(bid)
                .absolute()
                .size(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(ink)
                .child(glyph(icon, 14.0, inverse))
        };

        div()
            .id(("tile", id))
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .cursor_pointer()
            // Metro "press" feel: the tile shrinks while held
            .active(|s| s.p(px(5.0)))
            .when(self.edit.is_some() && !editing, |d| d.opacity(0.7))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.edit = Some(id);
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.edit.is_some() {
                    this.edit = None;
                    cx.notify();
                } else if let Some(t) = this.tiles.iter().find(|t| t.id == id) {
                    let a = t.app;
                    this.open_app(a, cx);
                }
            }))
            // Drag to replace (edit mode only): ghost follows the pointer, target highlights.
            .when(self.edit.is_some(), |d| {
                d.on_drag(
                    payload,
                    |p: &DraggedTile, _: Point<Pixels>, _: &mut Window, cx: &mut App| {
                        cx.new(|_| p.clone())
                    },
                )
            })
            .drag_over::<DraggedTile>(|s, _, _, _| s.opacity(0.55))
            .on_drop(cx.listener(move |this, d: &DraggedTile, _, cx| {
                this.drop_handled = true;
                this.move_tile(d.id, id, cx);
            }))
            .child(
                div()
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .bg(color)
                    .when(editing, |d| d.border_2().border_color(ink))
                    .child(content)
                    .when(editing, |d| {
                        d.child(
                            circle_btn("unpin", IconName::X)
                                .top(px(4.0))
                                .right(px(4.0))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.tiles.retain(|t| t.id != id);
                                    this.edit = None;
                                    this.save_soon(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                        .child(
                            circle_btn("resize", IconName::ArrowLeftRight)
                                .bottom(px(4.0))
                                .right(px(4.0))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(t) = this.tiles.iter_mut().find(|t| t.id == id) {
                                        t.size = t.size.next();
                                    }
                                    this.save_soon(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                    }),
            )
    }

    // ── pages ──

    fn start_page(
        &self,
        layout: &[(usize, usize)],
        rows: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fg = self.fg();
        let tiles: Vec<AnyElement> = (0..self.tiles.len())
            .map(|ix| {
                self.tile_view(ix, layout[ix], window, cx)
                    .into_any_element()
            })
            .collect();
        let grid_w = self.cols as f32 * self.cell + (self.cols as f32 - 1.0) * self.gap;
        let grid_h = if rows == 0 {
            0.0
        } else {
            rows as f32 * (self.cell + self.gap) - self.gap
        };

        div()
            .id("start-scroll")
            .size_full()
            .overflow_y_scroll()
            .on_click(cx.listener(|this, _, _, cx| {
                if this.edit.is_some() {
                    this.edit = None;
                    cx.notify();
                }
            }))
            // Dropping on empty space sends the tile to the end of the list.
            // (If a tile already handled this drop, `drop_handled` skips it.)
            .on_drop(cx.listener(|this, d: &DraggedTile, _, cx| {
                if !this.drop_handled {
                    this.move_to_end(d.id, cx);
                }
                this.drop_handled = false;
            }))
            .child(
                div()
                    .pl(px(MARGIN))
                    .pt(px(8.0))
                    .pb(px(24.0))
                    .child(div().relative().w(px(grid_w)).h(px(grid_h)).children(tiles))
                    .child(
                        div().w(px(grid_w)).mt(px(14.0)).flex().justify_end().child(
                            div()
                                .id("all-apps")
                                .size(px(46.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border_2()
                                .border_color(fg)
                                .cursor_pointer()
                                .active(|s| s.opacity(0.5))
                                .child(glyph(IconName::ArrowRight, 24.0, fg))
                                .on_click(cx.listener(|this, _, _, cx| this.go(Page::Apps, cx))),
                        ),
                    ),
            )
    }

    /// One row of the All Apps list: rectangular icon plate, name, pinned hint.
    fn app_row(&self, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let a = &APPS[i];
        let app_hex = a.color.unwrap_or(self.accent);
        let pinned = self.tiles.iter().any(|t| t.app == i);
        div()
            .id(("app-row", i))
            .flex()
            .items_center()
            .gap(px(14.0))
            .py(px(8.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| this.open_app(i, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| this.toggle_pin(i, cx)),
            )
            .child(
                // Rectangular (not circular) icon border in the list view.
                div()
                    .w(px(46.0))
                    .h(px(46.0))
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(fg.opacity(0.25))
                    .bg(rgb(app_hex))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(a.icon, 22.0, on_color(app_hex))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::LIGHT)
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(a.name),
                    )
                    .when(pinned, |d| {
                        d.child(
                            div()
                                .mt(px(1.0))
                                .text_size(px(11.0))
                                .text_color(fg.opacity(0.5))
                                .child("Pinned to Start"),
                        )
                    }),
            )
    }

    fn apps_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let query = self.search.trim().to_lowercase();

        // Filter by name / category, then group by category (declared order).
        let mut order: Vec<usize> = (0..APPS.len())
            .filter(|&i| {
                query.is_empty()
                    || APPS[i].name.to_lowercase().contains(&query)
                    || APPS[i].category.name().to_lowercase().contains(&query)
            })
            .collect();
        order.sort_by_key(|&i| (APPS[i].category.rank(), APPS[i].name));

        let mut list: Vec<AnyElement> = Vec::new();
        let mut current: Option<Category> = None;
        for &i in &order {
            let category = APPS[i].category;
            if current != Some(category) {
                current = Some(category);
                list.push(
                    div()
                        .mt(px(16.0))
                        .mb(px(2.0))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg.opacity(0.55))
                        .child(category.name())
                        .into_any_element(),
                );
            }
            list.push(self.app_row(i, cx).into_any_element());
        }
        if list.is_empty() {
            list.push(
                div()
                    .mt(px(44.0))
                    .w_full()
                    .text_center()
                    .text_size(px(13.0))
                    .text_color(fg.opacity(0.55))
                    .child("No apps match your search")
                    .into_any_element(),
            );
        }

        let searching = !self.search.is_empty();
        let search = div()
            .id("apps-search")
            .track_focus(&self.search_focus)
            .mt(px(6.0))
            .w_full()
            .h(px(44.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(self.cell_bg())
            .rounded(px(2.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| {
                this.search_focus.focus(window, cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = e.keystroke.modifiers;
                let command = m.control || m.alt || m.platform || m.function;
                match e.keystroke.key.as_str() {
                    "backspace" => {
                        this.search.pop();
                    }
                    "escape" => this.search.clear(),
                    _ => {
                        if !command && let Some(text) = e.keystroke.key_char.as_deref() {
                            this.search.push_str(text);
                        }
                    }
                }
                cx.notify();
            }))
            .child(glyph(IconName::Search, 17.0, fg.opacity(0.6)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(15.0))
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(if searching { fg } else { fg.opacity(0.45) })
                    .child(if searching {
                        self.search.clone()
                    } else {
                        "Search apps".to_string()
                    }),
            )
            .when(searching, |d| {
                d.child(
                    div()
                        .id("apps-search-clear")
                        .size(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(fg.opacity(0.15))
                        .cursor_pointer()
                        .active(|s| s.opacity(0.6))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search.clear();
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .child(glyph(IconName::X, 12.0, fg)),
                )
            });

        div()
            .id("apps-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(MARGIN))
            .child(
                div()
                    .w_full()
                    .max_w(px(560.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .pb(px(28.0))
                    .child(pivot_header("All Apps".into(), "Apps".into()))
                    .child(search)
                    .children(list),
            )
    }

    fn settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();

        let themes: Vec<_> = [("Dark", true), ("Light", false)]
            .into_iter()
            .map(|(name, dark)| {
                let selected = self.dark == dark;
                div()
                    .id(name)
                    .text_size(px(34.0))
                    .font_weight(FontWeight::LIGHT)
                    .cursor_pointer()
                    .text_color(if selected { fg } else { fg.opacity(0.45) })
                    .when(selected, |d| d.border_b_3().border_color(fg))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dark = dark;
                        this.save_soon(cx);
                        cx.notify();
                    }))
            })
            .collect();

        let wallpapers: Vec<_> = (0..WALLPAPER_NAMES.len())
            .map(|i| {
                let (from, to) = self.wallpaper_pair(i);
                let selected = self.wallpaper == i;
                div()
                    .id(("wallpaper", i))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.wallpaper = i;
                        this.save_soon(cx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(92.0))
                            .h(px(58.0))
                            .bg(linear_gradient(
                                135.0,
                                linear_color_stop(rgb(from), 0.0),
                                linear_color_stop(rgb(to), 1.0),
                            ))
                            .when(selected, |d| d.border_2().border_color(fg)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if selected { fg } else { fg.opacity(0.55) })
                            .child(WALLPAPER_NAMES[i]),
                    )
            })
            .collect();

        let swatches: Vec<_> = ACCENTS
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                div()
                    .id(("swatch", i))
                    .size(px(54.0))
                    .bg(rgb(c))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .when(self.accent == c, |d| d.border_2().border_color(fg))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.accent = c;
                        this.save_soon(cx);
                        cx.notify();
                    }))
            })
            .collect();

        // `name` is a `&'static str` label; the element id needs a stable key.
        let caption = |s: &'static str| {
            div()
                .mt(px(16.0))
                .mb(px(6.0))
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg.opacity(0.6))
                .child(s)
        };

        let gap = self.slider(
            Slider {
                id: "gap-track",
                kind: Drag::Gap,
                min: "0",
                max: "28",
                value: format!("{} px", self.gap.round() as u32),
                ratio: (self.gap - GAP_MIN) / (GAP_MAX - GAP_MIN),
            },
            cx,
        );

        let alpha = self.slider(
            Slider {
                id: "alpha-track",
                kind: Drag::Opacity,
                min: "0",
                max: "100",
                value: format!("{}%", (self.tile_alpha * 100.0).round() as u32),
                ratio: (self.tile_alpha - TILE_ALPHA_MIN) / (1.0 - TILE_ALPHA_MIN),
            },
            cx,
        );

        let reset = div()
            .id("settings-reset")
            .mt(px(26.0))
            .h(px(42.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(fg.opacity(0.35))
            .rounded(px(2.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(fg.opacity(0.85))
                    .child("Reset to Defaults"),
            )
            .on_click(cx.listener(|this, _, _, cx| this.reset_config(cx)));

        div()
            .id("settings-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(MARGIN))
            .child(
                div()
                    .w_full()
                    .max_w(px(560.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .pb(px(28.0))
                    .child(pivot_header("Settings".into(), "Theme".into()))
                    .child(caption("Theme"))
                    .child(div().flex().gap(px(24.0)).children(themes))
                    .child(caption("Wallpaper"))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.0))
                            .children(wallpapers),
                    )
                    .child(caption("Accent Color"))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(swatches),
                    )
                    .child(caption("Tile Gap"))
                    .child(gap)
                    .child(caption("Tile Transparency"))
                    .child(alpha)
                    .child(reset),
            )
    }

    fn app_page(&self, i: usize) -> impl IntoElement {
        let a = &APPS[i];
        let app_hex = a.color.unwrap_or(self.accent);
        div()
            .size_full()
            .px(px(MARGIN))
            .child(pivot_header("App".into(), a.name.into()))
            .child(
                div()
                    .mt(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .child(
                        div()
                            .size(px(120.0))
                            .rounded_full()
                            .bg(rgb(app_hex))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(glyph(a.icon, 60.0, on_color(app_hex))),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::LIGHT)
                            .child(format!(
                                "This is where the {} app would live. Press ← to go back to Start.",
                                a.name
                            )),
                    ),
            )
    }
    fn page_element(&self, page: Page, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match page {
            Page::Start => {
                let (layout, rows) = self.pack();
                self.start_page(&layout, rows, window, cx)
                    .into_any_element()
            }
            Page::Apps => self.apps_page(cx).into_any_element(),
            Page::Settings => self.settings_page(cx).into_any_element(),
            Page::App(i) => self.app_page(i).into_any_element(),
        }
    }

    /// The Windows Phone style page change: the outgoing page stays laid out at
    /// full size inside a clip that shrinks from the right, so it reads as a page
    /// being turned away to reveal the new one beneath. A soft crease shadow
    /// trails the moving edge.
    fn page_flip(
        &self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.page_morph {
            return None;
        }
        let prev = self.prev_page?;
        let old_page = self.page_element(prev, window, cx);
        let backdrop = if prev == Page::Start {
            hsla(0.0, 0.0, 0.0, 0.0)
        } else {
            self.page_bg()
        };
        let crease = hsla(0.0, 0.0, 0.0, if self.dark { 0.55 } else { 0.28 });
        let w = width.max(1.0);

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .h_full()
                .overflow_hidden()
                .bg(backdrop)
                // The outgoing page is decorative while it peels away.
                .block_mouse_except_scroll()
                // The old page keeps its full width; only the clip narrows.
                .child(div().w(px(w)).h_full().child(old_page))
                .child(
                    div()
                        .absolute()
                        .right(px(0.0))
                        .top(px(0.0))
                        .w(px(24.0))
                        .h_full()
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(hsla(0.0, 0.0, 0.0, 0.0), 0.0),
                            linear_color_stop(crease, 1.0),
                        )),
                )
                .with_animation(
                    ("page-flip", self.nav),
                    Animation::new(Duration::from_millis(420)).with_easing(ease_out_quint()),
                    move |el, d| {
                        let turn = (1.0 - d).powf(1.35);
                        el.w(px(w * turn))
                            .opacity((0.35 + 0.65 * (1.0 - d)).min(1.0))
                    },
                )
                .into_any_element(),
        )
    }

    /// Focus the All Apps search field the first frame the page is shown.
    fn maybe_focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Apps {
            if self.search_pending {
                self.search_pending = false;
                self.search_focus.focus(window, cx);
            }
        } else {
            self.search_pending = false;
        }
    }
}

// ───────────────────────────── root ─────────────────────────────

/// Final write on shutdown, so a change made inside the debounce window is kept.
impl Drop for Metro {
    fn drop(&mut self) {
        save_config(&self.config());
    }
}

impl Render for Metro {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Auto-fit: re-derive columns / cell size / font scale from the width the page area
        // is *actually* laid out at. `window.viewport_size()` can disagree with the real
        // layout width (window chrome / borders / DPI), which made the grid too wide and
        // clipped the right-hand tiles, so it is only the fallback for the first frame.
        let width = if self.measured_w > 0.0 {
            self.measured_w
        } else {
            f32::from(window.viewport_size().width)
        };
        self.relayout(width);
        let me = cx.entity();

        self.maybe_focus_search(window, cx);

        let page = self.page_element(self.page, window, cx);
        let page_backdrop = if self.page == Page::Start {
            hsla(0.0, 0.0, 0.0, 0.0)
        } else {
            self.page_bg()
        };
        let animated = div().size_full().bg(page_backdrop).child(page);
        let flip = self.page_flip(width, window, cx);

        let dim = (1.0 - self.brightness) * 0.6;
        let status_bar = self.status_bar(cx);
        let panel_layer = self.panel_layer(window, cx);

        div()
            .id("root")
            .relative()
            .size_full()
            .text_color(self.fg())
            .font_family("Segoe UI")
            // A drag that starts on the status bar and travels down opens a panel;
            // the half of the bar it started on picks which one.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if let Some(drag) = this.drag {
                    this.apply_drag(drag, f32::from(e.position.x));
                    cx.notify();
                } else if let Some((x, y)) = this.pull_start
                    && e.dragging()
                    && f32::from(e.position.y) - y > 22.0
                {
                    this.pull_start = None;
                    let kind = this.panel_for_x(x);
                    this.open_panel(kind, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.pull_start = None;
                    if this.drag.take().is_some() {
                        // Brightness / gap / transparency were being scrubbed.
                        this.save_soon(cx);
                        cx.notify();
                    }
                }),
            )
            .child(self.wallpaper_view())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    // The status bar is an overlay; this reserves its height in flow.
                    .child(div().h(px(STATUS_H)))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .relative()
                            .overflow_hidden()
                            // Invisible probe: reports the real width of this area every frame and
                            // asks for a re-render only when it actually changed (no feedback loop).
                            .child(
                                canvas(
                                    move |bounds, _window, cx| {
                                        let w = f32::from(bounds.size.width);
                                        me.update(cx, |this, cx| {
                                            if (this.measured_w - w).abs() > 0.5 {
                                                this.measured_w = w;
                                                cx.notify();
                                            }
                                        });
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(animated)
                            // Book-page turn: the outgoing page is clipped away to the left.
                            .when_some(flip, |d, f| d.child(f)),
                    )
                    .child(self.nav_bar(cx)),
            )
            // Screen brightness: dim everything below the panels.
            .when(dim > 0.001, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .size_full()
                        .bg(hsla(0.0, 0.0, 0.0, dim)),
                )
            })
            // Sliding panel sits above the content but under the status bar.
            .when_some(panel_layer, |d, layer| d.child(layer))
            .child(status_bar)
    }
}

/// Bounds for the main (host) window that lands on the user's desktop:
/// `WINDOW_WIDTH` wide, full usable screen height, centred horizontally. The
/// height excludes taskbar/dock areas, so the bottom of the UI stays reachable.
fn main_window_bounds(cx: &App) -> Bounds<Pixels> {
    let width = px(WINDOW_WIDTH);
    match cx.primary_display() {
        Some(display) => {
            let screen = display.visible_bounds();
            let x =
                f32::from(screen.origin.x) + (f32::from(screen.size.width) - WINDOW_WIDTH) / 2.0;
            Bounds {
                origin: point(px(x), screen.origin.y),
                size: size(width, screen.size.height),
            }
        }
        // No display information (headless): fall back to a phone-proportioned window.
        None => Bounds::centered(None, size(width, px(1064.0)), cx),
    }
}

fn main() {
    application().with_assets(AppAssets).run(|cx| {
        init(cx);

        let bounds = main_window_bounds(cx);
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(320.0), px(480.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Start".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(Metro::new),
        )
        .expect("failed to open window");
    });
}

#[cfg(test)]
mod tests {
    // Deliberately not `use super::*`: the crate-root glob import of `gpui_kit`
    // brings a `test` macro that would shadow Rust's built-in `#[test]`.
    use super::{
        CONFIG_VERSION, Config, GAP_MAX, TILE_ALPHA_MIN, TileSize, WALLPAPER_NAMES, app_index,
        default_tiles, load_config, save_config,
    };

    #[test]
    fn default_tiles_reference_real_apps() {
        let tiles = default_tiles();
        assert!(!tiles.is_empty());
        for tile in &tiles {
            assert!(
                app_index(&tile.app).is_some(),
                "{} is not in the app list",
                tile.app
            );
        }
    }

    #[test]
    fn config_round_trips_through_json() {
        let mut config = Config::default();
        config.dark = false;
        config.accent = 0xE51400;
        config.wallpaper = 3;
        config.gap = 14.5;
        config.tile_alpha = 0.75;
        config.brightness = 0.6;
        config.tiles[0].size = TileSize::Large;

        let json = serde_json::to_string(&config).expect("serialise");
        let parsed: Config = serde_json::from_str(&json).expect("parse");

        assert_eq!(parsed.version, CONFIG_VERSION);
        assert!(!parsed.dark);
        assert_eq!(parsed.accent, 0xE51400);
        assert_eq!(parsed.wallpaper, 3);
        assert_eq!(parsed.gap, 14.5);
        assert_eq!(parsed.tile_alpha, 0.75);
        assert_eq!(parsed.brightness, 0.6);
        assert_eq!(parsed.tiles.len(), config.tiles.len());
        assert_eq!(parsed.tiles[0].app, config.tiles[0].app);
        assert!(matches!(parsed.tiles[0].size, TileSize::Large));
    }

    #[test]
    fn sanitize_drops_unknown_apps_and_clamps_values() {
        let json = r#"{
            "accent": 1193046,
            "wallpaper": 99,
            "gap": 500.0,
            "tile_alpha": -1.0,
            "brightness": 5.0,
            "tiles": [
                { "app": "Clock", "size": "large" },
                { "app": "No Such App", "size": "small" }
            ]
        }"#;
        let config: Config = serde_json::from_str(json).expect("parse");
        let config = config.sanitized();

        // Missing fields fall back to defaults.
        assert!(config.dark);
        assert_eq!(config.wallpaper, WALLPAPER_NAMES.len() - 1);
        assert_eq!(config.gap, GAP_MAX);
        assert_eq!(config.tile_alpha, TILE_ALPHA_MIN);
        assert_eq!(config.brightness, 1.0);
        // The bogus tile is dropped, the real one is kept.
        assert_eq!(config.tiles.len(), 1);
        assert_eq!(config.tiles[0].app, "Clock");
    }

    #[test]
    fn save_then_load_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "horn-config-test-{}-{}.json",
            std::process::id(),
            CONFIG_VERSION
        ));
        // SAFETY: this test owns the variable for its duration; nothing else reads it.
        unsafe {
            std::env::set_var("HORN_CONFIG", &path);
        }

        let mut config = Config::default();
        config.accent = 0x008A00;
        config.gap = 4.0;
        config.wallpaper = 2;
        config.tiles.retain(|tile| tile.app != "Games");

        save_config(&config);
        let loaded = load_config().expect("config should load back");

        assert_eq!(loaded.accent, 0x008A00);
        assert_eq!(loaded.gap, 4.0);
        assert_eq!(loaded.wallpaper, 2);
        assert!(!loaded.tiles.iter().any(|tile| tile.app == "Games"));

        assert!(
            path.exists(),
            "config file should exist at {}",
            path.display()
        );
        let _ = std::fs::remove_file(&path);
        unsafe {
            std::env::remove_var("HORN_CONFIG");
        }
    }
}
