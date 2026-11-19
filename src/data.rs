//! Static app catalogue, notification data and layout constants.

use crate::prelude::*;

// ───────────────────────────── data ─────────────────────────────

pub(crate) const ACCENTS: [u32; 20] = [
    0xA4C400, 0x60A917, 0x008A00, 0x00ABA9, 0x1BA1E2, 0x0050EF, 0x6A00FF, 0xAA00FF, 0xF472D0,
    0xD80073, 0xA20025, 0xE51400, 0xFA6800, 0xF0A30A, 0xE3C800, 0x825A2C, 0x6D8764, 0x647687,
    0x76608A, 0x87794E,
];

// Layout constants (logical px)
pub(crate) const MARGIN: f32 = 20.0; // left/right page margin (lists, settings)
pub(crate) const TILE_MARGIN: f32 = 6.0; // left/right margin in the tiles view (tight, phone-like)
pub(crate) const DEFAULT_GAP: f32 = 10.0; // default gap between tiles (user adjustable)
pub(crate) const GAP_MIN: f32 = 0.0;
pub(crate) const GAP_MAX: f32 = 28.0;
pub(crate) const TARGET_CELL: f32 = 78.0; // preferred small-cell size; actual size stretches to fill the width
pub(crate) const PANEL_PAD: f32 = 20.0; // panel inner margin
pub(crate) const STATUS_H: f32 = 26.0; // status bar height (always the top layer)
pub(crate) const NAV_H: f32 = 52.0; // bottom navigation bar height
pub(crate) const TILE_ALPHA_MIN: f32 = 0.30; // most transparent a tile may get
pub(crate) const MAX_MANUAL_ROWS: u32 = 200; // limits corrupted configs from creating an unbounded canvas
pub(crate) const WINDOW_WIDTH: f32 = 476.0; // main (host) window width; height fills the screen

// Motion constants (ms)
pub(crate) const LAUNCH_MS: u64 = 640; // whole launch splash (zoom + fade)
pub(crate) const LAUNCH_SWAP_MS: u64 = 280; // moment the app page is swapped in (zoom finished)
pub(crate) const HOME_HOLD_MS: u64 = 450; // long-press threshold on the Start button
pub(crate) const ENTER_WINDOW_MS: u128 = 950; // tiles keep their turnstile-in wrapper for this long
pub(crate) const PULL_COMMIT: f32 = 0.30; // fraction of the panel that must be pulled to keep it open
pub(crate) const MAX_RUNNING: usize = 8;
pub(crate) const TILE_HOLD_MS: u64 = 420; // long-press on a tile: lift it / open the size menu
pub(crate) const PAGE_HOLD_MS: u64 = 500; // long-press on empty Start area: enter tile edit mode

// Wallpaper presets: (top-left, bottom-right). Each theme gets its own set so the
// foreground text stays readable without a heavy scrim.
pub(crate) const WALLPAPERS_DARK: [(u32, u32); 5] = [
    (0x0B1B2B, 0x123B4C), // Dusk
    (0x0A0A0A, 0x262626), // Graphite
    (0x170B29, 0x2C1A46), // Plum
    (0x061F2B, 0x0B4F6C), // Ocean
    (0x2B1206, 0x6C2A0B), // Ember
];
pub(crate) const WALLPAPERS_LIGHT: [(u32, u32); 5] = [
    (0xE7EEF5, 0xBBD6E6), // Dusk
    (0xF2F2F2, 0xD6D6D6), // Graphite
    (0xF2EAF7, 0xD7C6E8), // Plum
    (0xE4F5FA, 0xB9DEEC), // Ocean
    (0xFCEFE4, 0xF2CDB4), // Ember
];
pub(crate) const WALLPAPER_NAMES: [&str; 5] = ["Dusk", "Graphite", "Plum", "Ocean", "Ember"];

/// Pivot-style groups used by the All Apps list (declared order = display order).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Communication,
    Media,
    Productivity,
    Games,
    Tools,
    System,
}

impl Category {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Category::Communication => "Communication",
            Category::Media => "Media",
            Category::Productivity => "Productivity",
            Category::Games => "Games",
            Category::Tools => "Tools",
            Category::System => "System",
        }
    }

    pub(crate) fn rank(self) -> usize {
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

pub(crate) struct AppDef {
    pub(crate) name: &'static str,
    pub(crate) icon: IconName,
    pub(crate) category: Category,
    /// None = follows the system accent colour
    pub(crate) color: Option<u32>,
}

const fn def(name: &'static str, icon: IconName, category: Category, color: Option<u32>) -> AppDef {
    AppDef {
        name,
        icon,
        category,
        color,
    }
}

pub(crate) static APPS: &[AppDef] = &[
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
pub(crate) const SETTINGS: usize = 12;

/// A notification shown in the panel pulled down from the left of the status bar.
pub(crate) struct Note {
    pub(crate) app: usize,
    pub(crate) title: &'static str,
    pub(crate) body: &'static str,
    pub(crate) when: &'static str,
}

pub(crate) static NOTES: &[Note] = &[
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

pub(crate) fn app_index(name: &str) -> Option<usize> {
    APPS.iter().position(|app| app.name == name)
}
