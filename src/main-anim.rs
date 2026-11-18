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
//!   • App launch: press a tile -> it squishes, release and the tile zooms to fill the page
//!     while every other tile "turnstiles" away (staggered by distance from the tapped tile).
//!     The app's content then slides in. Going back turns the tiles back in, row by row.
//!   • Right-click a tile -> edit mode: ✕ unpins, ↔ cycles small → medium → wide → large
//!   • Small tiles are icon-only: captions, badges and live info are hidden
//!   • Long labels are clamped with an ellipsis, so tile text never spills out
//!   • In edit mode, DRAG a tile onto another tile to take its place; everything re-packs.
//!     Dropping on empty space sends it to the end.
//!   • Arrow at the end of the tiles -> All Apps: category headings, a search field at
//!     the top, rectangular icon borders (right-click an app to pin/unpin)
//!   • Settings tile -> theme + wallpaper + accent, tile gap and tile transparency
//!   • The status bar never hides: it is the always-on top layer. Pull down on its
//!     left half for notifications, on its right half for the system panel. The panel
//!     follows the pointer while you pull; release past ~30 % to keep it open, otherwise
//!     it springs back. Quick actions / notifications fly in one after another.
//!     Clicking outside slides it back up.
//!   • Bottom bar: ← back (also leaves edit mode / closes the panel / closes the switcher),
//!     ⊞ start, ⌕ apps.  LONG-PRESS ⊞ (hold ~0.45 s) -> running apps: cards fly in, tap a
//!     card to resume it (it zooms to full screen), ✕ flicks it away.
use gpui_kit::assets::{Assets as ComponentAssets, IconName, icon_assets};
use gpui_kit::base::{Easing, Transition, transition};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::borrow::Cow;
use std::time::{Duration, Instant};

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
const NAV_H: f32 = 52.0; // bottom navigation bar height
const TILE_ALPHA_MIN: f32 = 0.30; // most transparent a tile may get

// Motion constants (ms)
const LAUNCH_MS: u64 = 640; // whole launch splash (zoom + fade)
const LAUNCH_SWAP_MS: u64 = 280; // moment the app page is swapped in (zoom finished)
const HOME_HOLD_MS: u64 = 450; // long-press threshold on the Start button
const ENTER_WINDOW_MS: u128 = 950; // tiles keep their turnstile-in wrapper for this long
const PULL_COMMIT: f32 = 0.30; // fraction of the panel that must be pulled to keep it open
const MAX_RUNNING: usize = 8;

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

// ───────────────────────────── motion helpers ─────────────────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Quartic ease-out, input clamped to 0..=1.
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(4)
}

/// Cubic ease-in-out, input clamped to 0..=1.
fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

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

#[derive(Clone, Copy, PartialEq)]
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

/// The page that shows a given app.
fn page_for(app: usize) -> Page {
    if app == SETTINGS {
        Page::Settings
    } else {
        Page::App(app)
    }
}

/// The app a page belongs to (None for Start / All Apps).
fn app_of(page: Page) -> Option<usize> {
    match page {
        Page::App(i) => Some(i),
        Page::Settings => Some(SETTINGS),
        _ => None,
    }
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

/// How a tile takes part in a page transition.
#[derive(Clone, Copy)]
enum Turn {
    Idle,
    /// Turn in (payload = animation key, so it replays per navigation).
    Enter(usize),
    /// Turn away because another tile is being launched (payload = launch number).
    Leave(usize),
}

/// A running app launch: a coloured plate grows from `rect` to the full page area.
#[derive(Clone, Copy)]
struct Launch {
    app: usize,
    /// Tile that was tapped (it stays put under the plate while the rest turnstile away).
    tile: Option<usize>,
    /// Grid cell of the tapped tile, used to stagger the others by distance.
    from: (usize, usize),
    /// Start rectangle in root coordinates: (x, y, w, h).
    rect: (f32, f32, f32, f32),
    n: usize,
}

/// Geometry of the running-apps card grid.
#[derive(Clone, Copy)]
struct SwGeo {
    cols: usize,
    cw: f32,
    ch: f32,
    gap: f32,
    top: f32,
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
    nav_at: Instant,     // when the last navigation happened
    edit: Option<usize>, // id of the tile in edit mode
    drop_handled: bool,
    time: String,
    date: String,
    secs: u64,
    // responsive layout, recomputed every frame from the *measured* content width
    measured_w: f32, // real laid-out width of the page area (0 until first measurement)
    measured_h: f32, // real laid-out height of the page area
    vh: f32,         // window viewport height (last frame)
    cols: usize,
    cell: f32,
    scale: f32,
    last_w: f32,
    resizing: bool,
    start_scroll: ScrollHandle,
    // app launch zoom
    launch: Option<Launch>,
    launch_n: usize,
    // panels (swipe down from the status bar)
    panel: Option<PanelKind>,
    panel_closing: bool,
    panel_token: usize,
    panel_open_n: usize, // bumps only when a panel opens, drives the staggered fly-in
    pull_start: Option<(f32, f32)>,
    pulling: bool,
    pull_y: f32,
    drag: Option<Drag>,
    brightness: f32,
    wifi: bool,
    bluetooth: bool,
    airplane: bool,
    quiet: bool,
    night_light: bool,
    battery_saver: bool,
    notes_cleared: bool,
    // running apps / task switcher (long-press Start)
    running: Vec<usize>, // most recent first
    switcher: bool,
    switcher_closing: bool,
    switcher_token: usize,
    switcher_n: usize,
    switcher_scroll: ScrollHandle,
    dismissing: Option<usize>,
    dismiss_n: usize,
    home_press: Option<usize>,
    press_n: usize,
    home_long: bool,
    // appearance
    wallpaper: usize,
    gap: f32,
    tile_alpha: f32,
    // app search
    search: String,
    search_focus: FocusHandle,
    search_pending: bool,
}

impl Metro {
    fn new(cx: &mut Context<Self>) -> Self {
        use Face::*;
        use TileSize::*;
        let mut tiles = vec![
            Tile::new(0, Medium, vec![Icon], Some(2)),
            Tile::new(
                1,
                Medium,
                vec![
                    Mosaic([0xD80073, 0x6A00FF, 0x00ABA9, 0xFA6800]),
                    Text {
                        head: "Sara",
                        body: "is now on Numerals",
                    },
                ],
                None,
            ),
            Tile::new(
                2,
                Wide,
                vec![
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
                Some(3),
            ),
            Tile::new(
                3,
                Medium,
                vec![
                    Icon,
                    Text {
                        head: "Fishmind Labs",
                        body: "Weekly build report",
                    },
                ],
                Some(12),
            ),
            Tile::new(
                4,
                Medium,
                vec![
                    Big {
                        value: "18",
                        caption: "Wednesday",
                    },
                    Text {
                        head: "Up next",
                        body: "Design review · 4:30 PM",
                    },
                ],
                None,
            ),
            Tile::new(
                8,
                Wide,
                vec![
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
                None,
            ),
            Tile::new(11, Medium, vec![Clock], None),
            Tile::new(
                6,
                Medium,
                vec![
                    Icon,
                    Text {
                        head: "Now playing",
                        body: "Neon Rain — Fishmind",
                    },
                ],
                None,
            ),
            Tile::new(
                5,
                Wide,
                vec![Mosaic([0x008A00, 0x1BA1E2, 0xF0A30A, 0xA20025]), Icon],
                None,
            ),
            Tile::new(
                7,
                Medium,
                vec![
                    Icon,
                    Big {
                        value: "12",
                        caption: "updates",
                    },
                ],
                None,
            ),
            Tile::new(9, Small, vec![Icon], None),
            Tile::new(10, Small, vec![Icon], None),
            Tile::new(14, Small, vec![Icon], None),
            Tile::new(15, Small, vec![Icon], None),
            Tile::new(SETTINGS, Medium, vec![Icon], None),
            Tile::new(13, Wide, vec![Icon], None),
        ];
        for (i, t) in tiles.iter_mut().enumerate() {
            t.id = i;
            t.offset = (i as u64 * 5) % 6; // stagger the flips
        }
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
            accent: 0xA4C400,
            dark: true,
            page: Page::Start,
            prev_page: None,
            page_morph: false,
            history: vec![],
            nav: 0,
            nav_at: Instant::now(),
            edit: None,
            drop_handled: false,
            time: String::new(),
            date: String::new(),
            secs: 0,
            measured_w: 0.0,
            measured_h: 0.0,
            vh: 760.0,
            cols: 4,
            cell: TARGET_CELL,
            scale: 1.0,
            last_w: 0.0,
            resizing: false,
            start_scroll: ScrollHandle::new(),
            launch: None,
            launch_n: 0,
            panel: None,
            panel_closing: false,
            panel_token: 0,
            panel_open_n: 0,
            pull_start: None,
            pulling: false,
            pull_y: 0.0,
            drag: None,
            brightness: 1.0,
            wifi: true,
            bluetooth: false,
            airplane: false,
            quiet: false,
            night_light: false,
            battery_saver: false,
            notes_cleared: false,
            // A few apps "already running" so the switcher has something to show.
            running: vec![2, 8, 6],
            switcher: false,
            switcher_closing: false,
            switcher_token: 0,
            switcher_n: 0,
            switcher_scroll: ScrollHandle::new(),
            dismissing: None,
            dismiss_n: 0,
            home_press: None,
            press_n: 0,
            home_long: false,
            wallpaper: 0,
            gap: DEFAULT_GAP,
            tile_alpha: 1.0,
            search: String::new(),
            search_focus: cx.focus_handle(),
            search_pending: false,
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

    /// Height of the page area (between status bar and nav bar).
    fn page_h(&self) -> f32 {
        if self.measured_h > 0.0 {
            self.measured_h
        } else {
            (self.vh - STATUS_H - NAV_H).max(100.0)
        }
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

    /// Animated value for any keyed channel. While the window itself is being resized the
    /// tween is effectively instant, so layouts track the window edge without lag.
    #[allow(clippy::too_many_arguments)]
    fn tween_in(
        &self,
        scope: &'static str,
        id: usize,
        key: &'static str,
        target: f32,
        ms: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> f32 {
        let eid: ElementId = (scope, id).into();
        let ms = if self.resizing { 1 } else { ms };
        transition(
            (eid, key),
            target,
            Transition::new(Duration::from_millis(ms)).easing(Easing::EaseOut),
            window,
            cx,
        )
    }

    /// Animated value for one tile channel.
    fn tween(
        &self,
        id: usize,
        key: &'static str,
        target: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> f32 {
        self.tween_in("tile-motion", id, key, target, 280, window, cx)
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
        cx.notify();
    }

    fn move_to_end(&mut self, from_id: usize, cx: &mut Context<Self>) {
        if let Some(from) = self.tiles.iter().position(|t| t.id == from_id) {
            let t = self.tiles.remove(from);
            self.tiles.push(t);
        }
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

    /// Remember an app as "running" (most recent first).
    fn note_running(&mut self, page: Page) {
        if let Some(app) = app_of(page) {
            self.running.retain(|&a| a != app);
            self.running.insert(0, app);
            self.running.truncate(MAX_RUNNING);
        }
    }

    /// Swap pages with the book-page (Windows Phone) reveal: the outgoing page is
    /// kept alive for the duration of the peel, then dropped.
    fn flip_to(&mut self, page: Page, cx: &mut Context<Self>) {
        self.prev_page = Some(self.page);
        self.page = page;
        self.nav += 1;
        self.nav_at = Instant::now();
        self.edit = None;
        self.page_morph = true;
        self.note_running(page);
        if page == Page::Apps {
            self.search_pending = true;
        }
        if self.switcher && !self.switcher_closing {
            self.close_switcher(cx);
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
        if self.launch.is_some() {
            return;
        }
        if self.panel.is_some() {
            self.close_panel(cx);
        } else if self.switcher && !self.switcher_closing {
            self.close_switcher(cx);
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
        if self.launch.is_some() {
            return;
        }
        self.history.clear();
        if self.switcher && !self.switcher_closing {
            self.close_switcher(cx);
        }
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
        self.go(page_for(app), cx);
    }

    fn toggle_pin(&mut self, app: usize, cx: &mut Context<Self>) {
        if let Some(pos) = self.tiles.iter().position(|t| t.app == app) {
            self.tiles.remove(pos);
        } else {
            let mut t = Tile::new(app, TileSize::Medium, vec![Face::Icon], None);
            t.id = self.next_id;
            self.next_id += 1;
            t.offset = self.secs;
            self.tiles.push(t);
        }
        cx.notify();
    }

    // ── app launch (zoom + turnstile) ──

    /// Start the launch splash for a tapped Start tile.
    fn launch_from_tile(&mut self, id: usize, cx: &mut Context<Self>) {
        let (layout, _) = self.pack();
        let Some(ix) = self.tiles.iter().position(|t| t.id == id) else {
            return;
        };
        let t = &self.tiles[ix];
        let (c, r) = layout[ix];
        let (cw, ch) = t.size.cells();
        let (cell, g) = (self.cell, self.gap);
        let x = MARGIN + c as f32 * (cell + g);
        let y = STATUS_H + 8.0 + r as f32 * (cell + g) + f32::from(self.start_scroll.offset().y);
        let w = cw as f32 * cell + (cw as f32 - 1.0) * g;
        let h = ch as f32 * cell + (ch as f32 - 1.0) * g;
        let app = t.app;
        self.begin_launch(
            Launch {
                app,
                tile: Some(id),
                from: (c, r),
                rect: (x, y, w, h),
                n: 0,
            },
            cx,
        );
    }

    /// Start the launch splash from a card in the running-apps switcher.
    fn launch_from_card(&mut self, app: usize, cx: &mut Context<Self>) {
        let Some(i) = self.running.iter().position(|&a| a == app) else {
            return;
        };
        if app_of(self.page) == Some(app) {
            // Already the foreground app: just drop the switcher.
            self.close_switcher(cx);
            return;
        }
        let g = self.switcher_geo(self.last_w);
        let (px_, py_) = Self::card_xy(i, g);
        let y = STATUS_H + py_ + f32::from(self.switcher_scroll.offset().y);
        self.begin_launch(
            Launch {
                app,
                tile: None,
                from: (0, 0),
                rect: (px_, y, g.cw, g.ch),
                n: 0,
            },
            cx,
        );
    }

    /// Phase 1 (0 → LAUNCH_SWAP_MS): plate zooms to full page, other tiles turnstile away.
    /// Phase 2: the app page is swapped in underneath, the plate fades out and the
    /// app's own content slides in.
    fn begin_launch(&mut self, mut l: Launch, cx: &mut Context<Self>) {
        if self.launch.is_some() {
            return;
        }
        self.launch_n += 1;
        l.n = self.launch_n;
        self.launch = Some(l);
        self.edit = None;
        let (n, app) = (l.n, l.app);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(LAUNCH_SWAP_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.launch.map(|l| l.n) == Some(n) {
                    this.commit_open(app, cx);
                }
            });
            cx.background_executor()
                .timer(Duration::from_millis(LAUNCH_MS - LAUNCH_SWAP_MS + 40))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.launch.map(|l| l.n) == Some(n) {
                    this.launch = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Swap straight to the app page (no page peel — the launch plate covers the switch).
    fn commit_open(&mut self, app: usize, cx: &mut Context<Self>) {
        let page = page_for(app);
        self.switcher = false;
        self.switcher_closing = false;
        self.switcher_token += 1;
        self.panel = None;
        self.panel_closing = false;
        self.panel_token += 1;
        self.pulling = false;
        if self.page != page {
            self.history.push(self.page);
        }
        self.page = page;
        self.prev_page = None;
        self.page_morph = false;
        self.nav += 1;
        self.nav_at = Instant::now();
        self.edit = None;
        self.note_running(page);
        cx.notify();
    }

    // ── running apps switcher ──

    fn open_switcher(&mut self, cx: &mut Context<Self>) {
        if self.launch.is_some() {
            return;
        }
        if self.switcher && !self.switcher_closing {
            self.close_switcher(cx);
            return;
        }
        self.switcher = true;
        self.switcher_closing = false;
        self.switcher_token += 1;
        self.switcher_n += 1;
        self.dismissing = None;
        self.edit = None;
        if self.panel.is_some() {
            self.panel = None;
            self.panel_closing = false;
            self.panel_token += 1;
        }
        cx.notify();
    }

    fn close_switcher(&mut self, cx: &mut Context<Self>) {
        if !self.switcher || self.switcher_closing {
            return;
        }
        self.switcher_closing = true;
        self.switcher_token += 1;
        let token = self.switcher_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(260))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.switcher_token == token {
                    this.switcher = false;
                    this.switcher_closing = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Flick a card away; the remaining cards glide into the gap.
    fn dismiss_app(&mut self, app: usize, cx: &mut Context<Self>) {
        if self.dismissing.is_some() {
            return;
        }
        self.dismissing = Some(app);
        self.dismiss_n += 1;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(270))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.running.retain(|&a| a != app);
                this.dismissing = None;
                if app_of(this.page) == Some(app) {
                    // The foreground app was closed: fall back to Start behind the switcher.
                    this.page = Page::Start;
                    this.prev_page = None;
                    this.page_morph = false;
                    this.history.clear();
                    this.nav += 1;
                    this.nav_at = Instant::now();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Press on the Start button: a long press opens the switcher instead of going home.
    fn home_down(&mut self, cx: &mut Context<Self>) {
        self.press_n += 1;
        let n = self.press_n;
        self.home_press = Some(n);
        self.home_long = false;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(HOME_HOLD_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.home_press == Some(n) {
                    this.home_press = None;
                    this.home_long = true;
                    this.open_switcher(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn switcher_geo(&self, width: f32) -> SwGeo {
        let gap = 12.0;
        let avail = (width - 2.0 * MARGIN).max(160.0);
        let cols = (((avail + gap) / (170.0 + gap)).floor() as usize).clamp(2, 6);
        let cw = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        SwGeo {
            cols,
            cw,
            ch: (cw * 1.4).round(),
            gap,
            top: 64.0,
        }
    }

    fn card_xy(i: usize, g: SwGeo) -> (f32, f32) {
        (
            MARGIN + (i % g.cols) as f32 * (g.cw + g.gap),
            g.top + (i / g.cols) as f32 * (g.ch + g.gap),
        )
    }

    // ── panels ──

    fn panel_height_for(viewport_h: f32) -> f32 {
        (viewport_h * 0.66).clamp(320.0, 560.0)
    }

    fn open_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
        self.panel = Some(kind);
        self.panel_closing = false;
        self.panel_token += 1;
        self.panel_open_n += 1;
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
        self.pulling = false;
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

    // ── pointer tracking (root + gesture catcher) ──

    fn pointer_move(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag {
            self.apply_drag(drag, f32::from(e.position.x));
            cx.notify();
        } else if let Some((x, y)) = self.pull_start
            && e.dragging()
        {
            let dy = f32::from(e.position.y) - y;
            if !self.pulling && self.panel.is_none() && dy > 6.0 {
                // Mount the panel right away; it then tracks the pointer 1:1.
                let kind = self.panel_for_x(x);
                self.pulling = true;
                self.panel = Some(kind);
                self.panel_closing = false;
                self.panel_token += 1;
                self.panel_open_n += 1;
                self.edit = None;
            }
            if self.pulling {
                self.pull_y = dy.max(0.0);
                cx.notify();
            }
        }
    }

    fn pointer_up(&mut self, cx: &mut Context<Self>) {
        self.home_press = None;
        let was_slider = self.drag.take().is_some();
        if self.pulling {
            // Release: keep the panel if it was pulled far enough, else spring it back.
            self.pulling = false;
            self.pull_start = None;
            let ph = Self::panel_height_for(self.vh);
            if self.pull_y > ph * PULL_COMMIT {
                self.panel_closing = false;
            } else {
                self.close_panel(cx);
            }
            cx.notify();
            return;
        }
        self.pull_start = None;
        if was_slider {
            cx.notify();
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

    /// One quick-action tile. `slot` (0..=5) staggers its fly-in when the panel opens.
    #[allow(clippy::too_many_arguments)]
    fn quick_action(
        &self,
        id: &'static str,
        name: &'static str,
        icon: IconName,
        active: bool,
        toggle: fn(&mut Self),
        slot: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let open_n = self.panel_open_n;
        let delay = 45.0 * slot as f32;
        let total = 320.0 + 45.0 * 5.0;
        div()
            .id(id)
            .relative()
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
            .with_animation(
                ("qa-in", open_n * 100 + slot),
                Animation::new(Duration::from_millis(total as u64)),
                move |el, d| {
                    let p = ((d * total - delay) / 320.0).clamp(0.0, 1.0);
                    let e = ease_out(p);
                    el.top(px((1.0 - e) * 28.0)).opacity(e)
                },
            )
            .into_any_element()
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
                0,
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
                1,
                cx,
            ))
            .child(self.quick_action(
                "qa-plane",
                "Airplane mode",
                IconName::Plane,
                self.airplane,
                Self::toggle_airplane,
                2,
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
                3,
                cx,
            ))
            .child(self.quick_action(
                "qa-night",
                "Night light",
                IconName::Moon,
                self.night_light,
                |this| this.night_light = !this.night_light,
                4,
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
                5,
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
                rows.push(self.note_row(n, i, cx));
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

    /// One notification; rows slide in from the left one after another.
    fn note_row(&self, n: &Note, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let fg = self.fg();
        let a = &APPS[n.app];
        let hex = a.color.unwrap_or(self.accent);
        let app = n.app;
        let open_n = self.panel_open_n;
        let delay = 55.0 * i as f32;
        let total = 320.0 + 55.0 * NOTES.len() as f32;
        div()
            .id(("note", i))
            .relative()
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
            .with_animation(
                ("note-in", open_n * 100 + i),
                Animation::new(Duration::from_millis(total as u64)),
                move |el, d| {
                    let p = ((d * total - delay) / 320.0).clamp(0.0, 1.0);
                    let e = ease_out(p);
                    el.left(px(-56.0 * (1.0 - e))).opacity(e)
                },
            )
            .into_any_element()
    }

    /// The sliding surface itself. It is mounted while a panel is open (or still
    /// animating out) and driven by a reversible `transition`. While the user is
    /// pulling it, the transition collapses to ~1 ms so the panel tracks the pointer;
    /// on release it eases the rest of the way open (or back up).
    fn panel_layer(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let viewport_h = f32::from(window.viewport_size().height);
        let panel_h = Self::panel_height_for(viewport_h);
        let open = self.panel.is_some() && !self.panel_closing;

        let (slide_target, slide_ms) = if self.pulling {
            (self.pull_y.clamp(0.0, panel_h), 1)
        } else if open {
            (panel_h, 320)
        } else {
            (0.0, 320)
        };
        let slide = transition(
            ("panel", "slide"),
            slide_target,
            Transition::new(Duration::from_millis(slide_ms)).easing(Easing::EaseInOut),
            window,
            cx,
        );
        let scrim = transition(
            ("panel", "scrim"),
            (slide_target / panel_h).clamp(0.0, 1.0),
            Transition::new(Duration::from_millis(if self.pulling { 1 } else { 260 }))
                .easing(Easing::EaseInOut),
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
            .when(open && !self.pulling, |d| {
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

    // ── launch splash ──

    /// The coloured plate that grows from the tapped tile / card to the full page area,
    /// then fades away to reveal the freshly opened app.
    fn launch_layer(&self, width: f32) -> Option<AnyElement> {
        let l = self.launch?;
        let a = &APPS[l.app];
        let hex = a.color.unwrap_or(self.accent);
        let (x0, y0, w0, h0) = l.rect;
        let (x1, y1, w1, h1) = (0.0, STATUS_H, width, self.page_h());
        let icon_px = (w0.min(h0) * 0.5).clamp(24.0, 64.0);
        let split = LAUNCH_SWAP_MS as f32 / LAUNCH_MS as f32;

        Some(
            div()
                .id("launch-plate")
                .absolute()
                .left(px(x0))
                .top(px(y0))
                .w(px(w0))
                .h(px(h0))
                .bg(rgb(hex))
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .child(glyph(a.icon, icon_px, on_color(hex)))
                .with_animation(
                    ("launch", l.n),
                    Animation::new(Duration::from_millis(LAUNCH_MS)),
                    move |el, d| {
                        let grow = ease_in_out((d / split).min(1.0));
                        let fade = ease_out(((d - split) / (1.0 - split)).clamp(0.0, 1.0));
                        el.left(px(lerp(x0, x1, grow)))
                            .top(px(lerp(y0, y1, grow)))
                            .w(px(lerp(w0, w1, grow)))
                            .h(px(lerp(h0, h1, grow)))
                            .opacity(1.0 - fade)
                    },
                )
                .into_any_element(),
        )
    }

    // ── running apps switcher ──

    /// One card: an app "thumbnail" that flies in (staggered), glides when its neighbours
    /// leave, zooms open on tap and flicks away on ✕.
    fn switcher_card(
        &self,
        i: usize,
        app: usize,
        g: SwGeo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let a = &APPS[app];
        let hex = a.color.unwrap_or(self.accent);
        let ink = on_color(hex);
        let inverse = inverse_ink(hex);
        let (tx, ty) = Self::card_xy(i, g);
        let x = self.tween_in("card-motion", app, "x", tx, 260, window, cx);
        let y = self.tween_in("card-motion", app, "y", ty, 260, window, cx);
        let (cw, ch) = (g.cw, g.ch);
        let sn = self.switcher_n;
        let dn = self.dismiss_n;

        let face = div()
            .absolute()
            .overflow_hidden()
            .bg(rgb(hex))
            .flex()
            .items_center()
            .justify_center()
            .child(glyph(a.icon, (cw * 0.36).min(72.0), ink))
            .child(label(a.name, 1.0).text_color(ink))
            .child(
                div()
                    .id(("card-close", app))
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(ink)
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .child(glyph(IconName::X, 14.0, inverse))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.dismiss_app(app, cx);
                    })),
            );

        let animated: AnyElement = if self.dismissing == Some(app) {
            // Flick up and away.
            face.with_animation(
                ("card-out", app * 1000 + dn % 1000),
                Animation::new(Duration::from_millis(260)),
                move |el, d| {
                    let e = ease_in_out(d);
                    el.left(px(0.0))
                        .top(px(-(ch + 30.0) * e))
                        .w(px(cw))
                        .h(px(ch))
                        .opacity(1.0 - e)
                },
            )
            .into_any_element()
        } else if self.switcher_closing {
            // Everyone shrinks back and drops a little while the scrim fades.
            face.with_animation(
                ("card-exit", app * 1000 + sn % 1000),
                Animation::new(Duration::from_millis(240)),
                move |el, d| {
                    let e = ease_in_out(d);
                    let s = 1.0 - 0.2 * e;
                    el.left(px(cw * (1.0 - s) / 2.0))
                        .top(px(ch * (1.0 - s) / 2.0 + 30.0 * e))
                        .w(px(cw * s))
                        .h(px(ch * s))
                        .opacity(1.0 - e)
                },
            )
            .into_any_element()
        } else {
            // Fly in from below, growing, one card after another.
            let delay = 55.0 * i.min(7) as f32;
            let total = 360.0 + delay;
            face.with_animation(
                ("card-in", app * 1000 + sn % 1000),
                Animation::new(Duration::from_millis(total as u64)),
                move |el, d| {
                    let p = ((d * total - delay) / 360.0).clamp(0.0, 1.0);
                    let e = ease_out(p);
                    let s = 0.55 + 0.45 * e;
                    el.left(px(cw * (1.0 - s) / 2.0))
                        .top(px(ch * (1.0 - s) / 2.0 + (1.0 - e) * 70.0))
                        .w(px(cw * s))
                        .h(px(ch * s))
                        .opacity(e)
                },
            )
            .into_any_element()
        };

        div()
            .id(("card", app))
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(cw))
            .h(px(ch))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.launch_from_card(app, cx);
            }))
            .child(animated)
            .into_any_element()
    }

    fn switcher_layer(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.switcher {
            return None;
        }
        let g = self.switcher_geo(self.last_w);
        let n = self.running.len();
        let rows = n.div_ceil(g.cols);
        let page_h = self.page_h();
        let content_h = (g.top + rows as f32 * (g.ch + g.gap) + 24.0).max(page_h);
        let closing = self.switcher_closing;
        let sn = self.switcher_n;

        let mut cards: Vec<AnyElement> = Vec::with_capacity(n);
        for (i, &app) in self.running.iter().enumerate() {
            cards.push(self.switcher_card(i, app, g, window, cx));
        }

        let header = div()
            .absolute()
            .left(px(MARGIN))
            .top(px(14.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(white().opacity(0.6))
                    .child("MULTITASKING"),
            )
            .child(
                div()
                    .text_size(px(28.0))
                    .line_height(px(32.0))
                    .font_weight(FontWeight::LIGHT)
                    .text_color(white())
                    .child("running apps"),
            );

        let scrim = div()
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .with_animation(
                (
                    if closing {
                        "sw-scrim-out"
                    } else {
                        "sw-scrim-in"
                    },
                    sn,
                ),
                Animation::new(Duration::from_millis(240)),
                move |el, d| {
                    let a = if closing { 1.0 - d } else { d };
                    el.bg(hsla(0.0, 0.0, 0.0, 0.66 * a))
                },
            );

        let scroll = div()
            .id("switcher-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.switcher_scroll)
            // Tapping the empty area between cards dismisses the switcher.
            .on_click(cx.listener(|this, _, _, cx| this.close_switcher(cx)))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(content_h))
                    .child(header)
                    .children(cards)
                    .when(n == 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(g.top + 60.0))
                                .w_full()
                                .text_center()
                                .text_size(px(14.0))
                                .text_color(white().opacity(0.6))
                                .child("No running apps"),
                        )
                    }),
            );

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(STATUS_H))
                .w_full()
                .h(px(page_h))
                .overflow_hidden()
                .occlude()
                .child(scrim)
                .child(scroll)
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

/// Pivot header. With `key` set, the big title slides in from the right and fades in
/// (the Windows Phone page-enter feel); the key makes it replay once per navigation.
fn pivot_header(small: String, big: String, key: Option<usize>) -> AnyElement {
    let header = div()
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
        );
    match key {
        Some(k) => header
            .with_animation(
                ("pivot", k),
                Animation::new(Duration::from_millis(460)).with_easing(ease_out_quint()),
                |el, d| el.relative().left(px(90.0 * (1.0 - d))).opacity(d),
            )
            .into_any_element(),
        None => header.into_any_element(),
    }
}

/// Wrap a tile's plate for a turnstile transition: the plate keeps its full size inside a
/// clip whose width collapses toward the left edge (like a card hinged on its left side),
/// fading as it goes. `delay` (ms) staggers tiles.
fn turnstile(plate: Div, w: f32, h: f32, id: usize, turn: Turn, delay: f32) -> AnyElement {
    let (key, leaving) = match turn {
        Turn::Idle => return plate.into_any_element(),
        Turn::Enter(k) => (k, false),
        Turn::Leave(k) => (k, true),
    };
    const DUR: f32 = 380.0;
    let total = DUR + delay;
    div()
        .relative()
        .h_full()
        .overflow_hidden()
        .child(plate.w(px(w)).h(px(h)))
        .with_animation(
            (
                if leaving { "tile-leave" } else { "tile-enter" },
                id * 100_000 + key % 100_000,
            ),
            Animation::new(Duration::from_millis(total as u64)),
            move |el, d| {
                let p = ((d * total - delay) / DUR).clamp(0.0, 1.0);
                let e = ease_out(p);
                let (s, a) = if leaving {
                    (1.0 - 0.8 * e, 1.0 - e)
                } else {
                    (0.25 + 0.75 * e, e)
                };
                el.w(px(w * s)).opacity(a)
            },
        )
        .into_any_element()
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
            // Press starts a potential pull. Dragging down makes the panel follow the
            // pointer (see `pointer_move`); a plain click toggles it.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    this.pull_start = Some((f32::from(e.position.x), f32::from(e.position.y)));
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, e: &MouseUpEvent, _, cx| {
                    if this.pulling {
                        return; // the pull is committed / cancelled in `pointer_up`
                    }
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
        let holding = self.home_press.is_some();
        let logo = div()
            .flex()
            .flex_wrap()
            .w(px(20.0))
            .gap(px(2.0))
            .when(holding, |d| d.opacity(0.55))
            .children((0..4).map(|_| div().size(px(9.0)).bg(fg)));

        div()
            .h(px(NAV_H))
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
                // Tap = Start, hold = running apps.
                btn("nav-start")
                    .child(logo)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, _, cx| this.home_down(cx)),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, cx| {
                            if this.home_press.take().is_some() {
                                cx.notify();
                            }
                        }),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.home_long {
                            // This release ended a long press that already opened the switcher.
                            this.home_long = false;
                        } else {
                            this.home(cx);
                        }
                    })),
            )
            .child(
                btn("nav-search")
                    .child(glyph(IconName::Search, 24.0, fg))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.launch.is_some() {
                            return;
                        }
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

    #[allow(clippy::too_many_arguments)]
    fn tile_view(
        &self,
        ix: usize,
        pos: (usize, usize),
        entering: bool,
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

        // Turnstile: tiles turn *away* (staggered by distance) when another tile is
        // launched, and turn *in* (staggered row by row) whenever Start is shown again.
        let (turn, delay) = if let Some(l) = self.launch.filter(|_| entering) {
            if l.tile == Some(id) {
                (Turn::Idle, 0.0)
            } else {
                let dist = (pos.0 as f32 - l.from.0 as f32).abs()
                    + (pos.1 as f32 - l.from.1 as f32).abs();
                (Turn::Leave(l.n), (dist * 22.0).min(160.0))
            }
        } else if entering && self.nav_at.elapsed().as_millis() < ENTER_WINDOW_MS {
            (
                Turn::Enter(self.nav),
                (pos.1 as f32 * 45.0 + pos.0 as f32 * 12.0).min(320.0),
            )
        } else {
            (Turn::Idle, 0.0)
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

        let plate = div()
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
                            cx.stop_propagation();
                            cx.notify();
                        })),
                )
            });

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
                } else {
                    // Zoom the tile open; the rest turnstile away.
                    this.launch_from_tile(id, cx);
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
            .child(turnstile(plate, w, h, id, turn, delay))
    }

    // ── pages ──

    fn start_page(
        &self,
        layout: &[(usize, usize)],
        rows: usize,
        entering: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fg = self.fg();
        let tiles: Vec<AnyElement> = (0..self.tiles.len())
            .map(|ix| {
                self.tile_view(ix, layout[ix], entering, window, cx)
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
            .track_scroll(&self.start_scroll)
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
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.launch.is_none() {
                                        this.go(Page::Apps, cx);
                                    }
                                })),
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

    fn apps_page(&self, key: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(pivot_header("All Apps".into(), "Apps".into(), key))
                    .child(search)
                    .children(list),
            )
    }

    fn settings_page(&self, key: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(pivot_header("Settings".into(), "Theme".into(), key))
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
                    .child(alpha),
            )
    }

    /// Placeholder app page. With `anim` set, the header slides in, the icon pops in and the
    /// text trails after it — the "app content arrives as the splash fades" feel.
    fn app_page(&self, i: usize, anim: Option<usize>) -> impl IntoElement {
        let a = &APPS[i];
        let app_hex = a.color.unwrap_or(self.accent);

        let icon = div()
            .size(px(120.0))
            .rounded_full()
            .bg(rgb(app_hex))
            .flex()
            .items_center()
            .justify_center()
            .child(glyph(a.icon, 60.0, on_color(app_hex)));
        let icon: AnyElement = match anim {
            Some(k) => icon
                .with_animation(
                    ("app-icon", k),
                    Animation::new(Duration::from_millis(520)).with_easing(ease_out_quint()),
                    |el, d| el.size(px(120.0 * (0.5 + 0.5 * d))).opacity(d),
                )
                .into_any_element(),
            None => icon.into_any_element(),
        };

        let text = div()
            .w_full()
            .text_size(px(18.0))
            .font_weight(FontWeight::LIGHT)
            .child(format!(
                "This is where the {} app would live. Press ← to go back to Start.",
                a.name
            ));
        let text: AnyElement = match anim {
            Some(k) => text
                .with_animation(
                    ("app-text", k),
                    Animation::new(Duration::from_millis(640)).with_easing(ease_out_quint()),
                    |el, d| el.relative().left(px(70.0 * (1.0 - d))).opacity(d),
                )
                .into_any_element(),
            None => text.into_any_element(),
        };

        div()
            .size_full()
            .px(px(MARGIN))
            .child(pivot_header("App".into(), a.name.into(), anim))
            .child(
                div()
                    .mt(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    // Fixed 120×120 slot so the text below doesn't jump while the icon grows.
                    .child(
                        div()
                            .size(px(120.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon),
                    )
                    .child(text),
            )
    }

    /// `entering` = this is the page being shown (as opposed to the outgoing page that is
    /// still painted while it peels away): only then do the page-enter animations run.
    fn page_element(
        &self,
        page: Page,
        entering: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = entering.then_some(self.nav);
        match page {
            Page::Start => {
                let (layout, rows) = self.pack();
                self.start_page(&layout, rows, entering, window, cx)
                    .into_any_element()
            }
            Page::Apps => self.apps_page(key, cx).into_any_element(),
            Page::Settings => self.settings_page(key, cx).into_any_element(),
            Page::App(i) => self.app_page(i, key).into_any_element(),
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
        let old_page = self.page_element(prev, false, window, cx);
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

impl Render for Metro {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Auto-fit: re-derive columns / cell size / font scale from the width the page area
        // is *actually* laid out at. `window.viewport_size()` can disagree with the real
        // layout width (window chrome / borders / DPI), which made the grid too wide and
        // clipped the right-hand tiles, so it is only the fallback for the first frame.
        self.vh = f32::from(window.viewport_size().height);
        let width = if self.measured_w > 0.0 {
            self.measured_w
        } else {
            f32::from(window.viewport_size().width)
        };
        self.relayout(width);
        let me = cx.entity();

        self.maybe_focus_search(window, cx);

        let page = self.page_element(self.page, true, window, cx);
        let page_backdrop = if self.page == Page::Start {
            hsla(0.0, 0.0, 0.0, 0.0)
        } else {
            self.page_bg()
        };
        let animated = div().size_full().bg(page_backdrop).child(page);
        let flip = self.page_flip(width, window, cx);

        let dim = (1.0 - self.brightness) * 0.6;
        let status_bar = self.status_bar(cx);
        let switcher_layer = self.switcher_layer(window, cx);
        let launch_layer = self.launch_layer(width);
        let panel_layer = self.panel_layer(window, cx);

        // While a pull or a slider drag is in progress, a transparent layer sits above the
        // (pointer-occluding) panel so the gesture keeps receiving move / up events.
        let catcher = (self.pulling || self.drag.is_some()).then(|| {
            div()
                .id("gesture-catcher")
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                    this.pointer_move(e, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| this.pointer_up(cx)),
                )
        });

        div()
            .id("root")
            .relative()
            .size_full()
            .text_color(self.fg())
            .font_family("Segoe UI")
            // A drag that starts on the status bar and travels down pulls a panel with it;
            // the half of the bar it started on picks which one.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                this.pointer_move(e, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.pointer_up(cx)),
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
                            // Invisible probe: reports the real size of this area every frame and
                            // asks for a re-render only when it actually changed (no feedback loop).
                            .child(
                                canvas(
                                    move |bounds, _window, cx| {
                                        let w = f32::from(bounds.size.width);
                                        let h = f32::from(bounds.size.height);
                                        me.update(cx, |this, cx| {
                                            if (this.measured_w - w).abs() > 0.5
                                                || (this.measured_h - h).abs() > 0.5
                                            {
                                                this.measured_w = w;
                                                this.measured_h = h;
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
            // Running-apps cards (long-press Start).
            .when_some(switcher_layer, |d, layer| d.child(layer))
            // App launch zoom plate, above the switcher so cards can zoom open too.
            .when_some(launch_layer, |d, layer| d.child(layer))
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
            .when_some(catcher, |d, c| d.child(c))
            .child(status_bar)
    }
}

fn main() {
    application().with_assets(AppAssets).run(|cx| {
        init(cx);

        let bounds = Bounds::centered(None, size(px(400.0), px(760.0)), cx);
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
