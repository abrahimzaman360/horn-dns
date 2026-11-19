//! Core data model for tiles, pages and transient UI state.

use crate::prelude::*;

/// The live-tile faces an app starts with (also used when restoring a config).
pub(crate) fn default_faces(app: usize) -> Vec<Face> {
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
            Music,
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

pub(crate) fn default_badge(app: usize) -> Option<u32> {
    match APPS.get(app).map(|a| a.name) {
        Some("Phone") => Some(2),
        Some("Messaging") => Some(3),
        Some("Mail") => Some(12),
        _ => None,
    }
}

/// Rebuild tiles from config entries, restoring stable ids and flip stagger.
pub(crate) fn tiles_from_config(configs: &[TileConfig]) -> Vec<Tile> {
    configs
        .iter()
        .filter_map(|tc| app_index(&tc.app).map(|app| (app, tc)))
        .enumerate()
        .map(|(i, (app, tc))| {
            let mut tile = Tile::new(app, tc.size, default_faces(app), default_badge(app));
            tile.id = i;
            tile.offset = (i as u64 * 5) % 6;
            tile.live_enabled = tc.live_enabled;
            tile.manual_x = tc.manual_x;
            tile.manual_y = tc.manual_y;
            tile
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TileSize {
    Small,
    Medium,
    Wide,
    Large,
}

impl TileSize {
    /// (columns, rows) in grid cells
    pub(crate) fn cells(self) -> (usize, usize) {
        match self {
            TileSize::Small => (1, 1),
            TileSize::Medium => (2, 2),
            TileSize::Wide => (4, 2),
            TileSize::Large => (4, 4),
        }
    }
    pub(crate) fn next(self) -> Self {
        match self {
            TileSize::Small => TileSize::Medium,
            TileSize::Medium => TileSize::Wide,
            TileSize::Wide => TileSize::Large,
            TileSize::Large => TileSize::Small,
        }
    }
}

#[derive(Clone)]
pub(crate) enum Face {
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
    Music,
}

pub(crate) struct Tile {
    pub(crate) id: usize, // stable identity (indexes change when tiles are re-ordered)
    pub(crate) app: usize,
    pub(crate) size: TileSize,
    pub(crate) faces: Vec<Face>,
    pub(crate) badge: Option<u32>,
    pub(crate) idx: usize,
    pub(crate) prev: usize,
    pub(crate) tick: usize,
    pub(crate) offset: u64,
    pub(crate) live_enabled: bool,
    pub(crate) manual_x: Option<f32>,
    pub(crate) manual_y: Option<f32>,
}

impl Tile {
    pub(crate) fn new(app: usize, size: TileSize, faces: Vec<Face>, badge: Option<u32>) -> Self {
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
            live_enabled: true,
            manual_x: None,
            manual_y: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Page {
    Start,
    Apps,
    Settings,
    App(usize),
}

/// The page that shows a given app.
pub(crate) fn page_for(app: usize) -> Page {
    if app == SETTINGS {
        Page::Settings
    } else {
        Page::App(app)
    }
}

/// The app a page belongs to (None for Start / All Apps).
pub(crate) fn app_of(page: Page) -> Option<usize> {
    match page {
        Page::App(i) => Some(i),
        Page::Settings => Some(SETTINGS),
        _ => None,
    }
}

/// Which surface slides down from the status bar.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PanelKind {
    /// Quick toggles, brightness, accent — pulled from the right half.
    System,
    /// Notification list — pulled from the left half.
    Notifications,
}

/// Slider currently being dragged (the pointer is tracked on the root).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Drag {
    Brightness,
    Gap,
    Opacity,
}

/// Description of one labelled slider row.
pub(crate) struct Slider {
    pub(crate) id: &'static str,
    pub(crate) kind: Drag,
    pub(crate) min: &'static str,
    pub(crate) max: &'static str,
    pub(crate) value: String,
    pub(crate) ratio: f32,
}

/// How a tile takes part in a page transition.
#[derive(Clone, Copy)]
pub(crate) enum Turn {
    Idle,
    /// Turn in (payload = animation key, so it replays per navigation).
    Enter(usize),
    /// Turn away because another tile is being launched (payload = launch number).
    Leave(usize),
}

/// A running app launch: a coloured plate grows from `rect` to the full page area.
#[derive(Clone, Copy)]
pub(crate) struct Launch {
    pub(crate) app: usize,
    /// Tile that was tapped (it stays put under the plate while the rest turnstile away).
    pub(crate) tile: Option<usize>,
    /// Grid cell of the tapped tile, used to stagger the others by distance.
    pub(crate) from: (usize, usize),
    /// Start rectangle in root coordinates: (x, y, w, h).
    pub(crate) rect: (f32, f32, f32, f32),
    pub(crate) n: usize,
}

/// Geometry of the running-apps card grid.
#[derive(Clone, Copy)]
pub(crate) struct SwGeo {
    pub(crate) cols: usize,
    pub(crate) cw: f32,
    pub(crate) ch: f32,
    pub(crate) gap: f32,
    pub(crate) top: f32,
}

/// A press-and-hold in progress on a tile: arming lifts the tile, moving drags it,
/// releasing without moving opens the size menu.
#[derive(Clone, Copy)]
pub(crate) struct TileHold {
    pub(crate) id: usize,
    pub(crate) token: usize,
    pub(crate) start: (f32, f32),
    pub(crate) armed: bool,
    pub(crate) moved: bool,
}

/// The floating size menu opened by a long press that did not move.
#[derive(Clone, Copy)]
pub(crate) struct TileMenu {
    pub(crate) id: usize,
}

/// Payload carried while a tile is being dragged. Also renders the drag "ghost".
#[derive(Clone)]
pub(crate) struct DraggedTile {
    pub(crate) id: usize,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) color: u32,
    pub(crate) icon: IconName,
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
                tile_ink(self.color),
            ))
    }
}
