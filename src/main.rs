//! Windows Phone 8 "Metro" Start screen, built on GPUI Kit (https://gpui-kit.com).
//!
//! Layout
//!   • Tiles live on a grid of small cells. Small = 1×1, Medium = 2×2, Wide = 4×2, Large = 4×4.
//!   • The grid auto-fits the window: column count and cell size are recomputed from the
//!     window width on every frame (4 columns on a phone-sized window, more when it's wider),
//!     and fonts / icons scale with the cell size.
//!   • A first-fit packer flows tiles into the grid, so gaps are back-filled just like the
//!     real Start screen. Tiles glide to their new place / size (GPUI Base `transition`).
//!
//! Interactions
//!   • Live tiles flip on their own (staggered), clock ticks every second
//!   • Press a tile -> it squishes, release opens the app
//!   • Right-click a tile -> edit mode: ✕ unpins, ↔ cycles small → medium → wide → large
//!   • In edit mode, DRAG a tile onto another tile to take its place; everything re-packs.
//!     Dropping on empty space sends it to the end.
//!   • Arrow at the end of the tiles -> All Apps (right-click an app to pin/unpin)
//!   • Settings tile -> light/dark theme + 20 classic Windows Phone accent colours
//!   • Bottom bar: ← back (also leaves edit mode), ⊞ start, ⌕ apps

use gpui_kit::assets::{Assets as ComponentAssets, IconName, icon_assets};
use gpui_kit::base::{Easing, Transition, transition};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::borrow::Cow;
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
        X
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
const GAP: f32 = 10.0; // gap between tiles
const TARGET_CELL: f32 = 78.0; // preferred small-cell size; actual size stretches to fill the width

struct AppDef {
    name: &'static str,
    icon: IconName,
    /// None = follows the system accent colour
    color: Option<u32>,
}

const fn def(name: &'static str, icon: IconName, color: Option<u32>) -> AppDef {
    AppDef { name, icon, color }
}

static APPS: &[AppDef] = &[
    def("Phone", IconName::Phone, None),                     // 0
    def("People", IconName::Users, None),                    // 1
    def("Messaging", IconName::MessageSquare, None),         // 2
    def("Mail", IconName::Mail, None),                       // 3
    def("Calendar", IconName::CalendarDays, Some(0x0050EF)), // 4
    def("Photos", IconName::Image, Some(0x008A00)),          // 5
    def("Music", IconName::Music, Some(0xFA6800)),           // 6
    def("Store", IconName::ShoppingBag, Some(0x60A917)),     // 7
    def("Weather", IconName::Sun, Some(0x1BA1E2)),           // 8
    def("Maps", IconName::Map, Some(0xAA00FF)),              // 9
    def("Internet", IconName::Globe, Some(0x0050EF)),        // 10
    def("Clock", IconName::Clock, Some(0x00ABA9)),           // 11
    def("Settings", IconName::Settings, Some(0x647687)),     // 12
    def("Games", IconName::Gamepad2, Some(0x60A917)),        // 13
    def("Camera", IconName::Camera, Some(0x76608A)),         // 14
    def("Notes", IconName::StickyNote, Some(0xF0A30A)),      // 15
];
const SETTINGS: usize = 12;

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
            .child(glyph(self.icon.clone(), (self.h * 0.4).min(64.0), white()))
    }
}

// ───────────────────────────── state ─────────────────────────────

struct Metro {
    tiles: Vec<Tile>,
    next_id: usize,
    accent: u32,
    dark: bool,
    page: Page,
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
        let avail = (width - 2.0 * MARGIN).max(200.0);
        let cols =
            ((((avail + GAP) / (TARGET_CELL + GAP)) / 2.0).round() as usize * 2).clamp(4, 16);
        self.cell = (avail - GAP * (cols as f32 - 1.0)) / cols as f32;
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
        self.page = page;
        self.nav += 1;
        self.edit = None;
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            self.edit = None;
        } else if self.page != Page::Start {
            self.page = self.history.pop().unwrap_or(Page::Start);
            self.nav += 1;
        }
        cx.notify();
    }

    fn home(&mut self, cx: &mut Context<Self>) {
        self.history.clear();
        self.page = Page::Start;
        self.edit = None;
        self.nav += 1;
        cx.notify();
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

    // ── theme ──

    fn fg(&self) -> Hsla {
        if self.dark { white() } else { black() }
    }
    fn bg(&self) -> Hsla {
        if self.dark { black() } else { white() }
    }
}

// ───────────────────────────── view helpers ─────────────────────────────

fn label(name: &'static str, s: f32) -> Div {
    div()
        .absolute()
        .left(px(10.0 * s))
        .bottom(px(8.0 * s))
        .text_size(px(13.0 * s))
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
    fn status_bar(&self) -> impl IntoElement {
        let fg = self.fg();
        div()
            .h(px(26.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(12.0))
            .text_color(fg)
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(2.0))
                    .children((0..4).map(|i| div().w(px(3.0)).h(px(4.0 + 3.0 * i as f32)).bg(fg))),
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
                    .on_click(cx.listener(|this, _, _, cx| this.go(Page::Apps, cx))),
            )
    }

    // ── tiles ──

    fn face(&self, face: &Face, t: &Tile) -> Div {
        let app = &APPS[t.app];
        let s = self.scale;
        let small = t.size == TileSize::Small;
        let base = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .text_color(white());

        match face {
            Face::Icon => {
                let icon_px = match t.size {
                    TileSize::Small => 30.0,
                    TileSize::Medium | TileSize::Wide => 56.0,
                    TileSize::Large => 88.0,
                } * s;
                base.flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(app.icon.clone(), icon_px, white()))
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
                        .child(*head),
                )
                .child(
                    div()
                        .text_size(px(13.0 * s))
                        .text_color(white().opacity(0.85))
                        .child(*body),
                )
                .child(label(app.name, s)),
            Face::Big { value, caption } => base
                .child(
                    div()
                        .absolute()
                        .left(px(10.0 * s))
                        .top(px(14.0 * s))
                        .text_size(px(60.0 * s))
                        .font_weight(FontWeight::LIGHT)
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
                base.flex()
                    .flex_wrap()
                    .children((0..n).map(|i| {
                        div()
                            .w(relative(1.0 / c as f32))
                            .h(relative(1.0 / r as f32))
                            .bg(rgb(cols[i % 4]))
                    }))
                    .child(label(app.name, s))
            }
            Face::Clock => base
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(46.0 * s))
                        .font_weight(FontWeight::LIGHT)
                        .child(self.time.clone()),
                )
                .child(div().text_size(px(12.0 * s)).child(self.date.clone()))
                .child(label(app.name, s)),
        }
    }

    /// Front face, or (old face sliding up + new face sliding in from below).
    /// The wrapper ID includes `t.tick`, so every flip replays a fresh one-shot animation.
    fn tile_content(&self, t: &Tile, h: f32) -> AnyElement {
        if t.faces.len() > 1 && t.tick > 0 {
            let anim = || Animation::new(Duration::from_millis(650)).with_easing(ease_out_quint());
            let out = self.face(&t.faces[t.prev], t).with_animation(
                ("face-out", t.tick),
                anim(),
                move |el, d| el.top(px(-h * d)),
            );
            let inn = self.face(&t.faces[t.idx], t).with_animation(
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
            self.face(&t.faces[t.idx], t).into_any_element()
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
        let (cell, g) = (self.cell, GAP);

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
        let color = rgb(color_hex);
        let content = self.tile_content(t, h);
        let payload = DraggedTile {
            id,
            w,
            h,
            color: color_hex,
            icon: APPS[t.app].icon.clone(),
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
                .bg(white())
                .child(glyph(icon, 14.0, black()))
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
                    this.go(
                        if a == SETTINGS {
                            Page::Settings
                        } else {
                            Page::App(a)
                        },
                        cx,
                    );
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
                    .when(editing, |d| d.border_2().border_color(white()))
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
        let grid_w = self.cols as f32 * self.cell + (self.cols as f32 - 1.0) * GAP;
        let grid_h = if rows == 0 {
            0.0
        } else {
            rows as f32 * (self.cell + GAP) - GAP
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

    fn apps_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = rgb(self.accent);
        let mut order: Vec<usize> = (0..APPS.len()).collect();
        order.sort_by_key(|&i| APPS[i].name);

        let rows: Vec<_> = order
            .into_iter()
            .map(|i| {
                let a = &APPS[i];
                let pinned = self.tiles.iter().any(|t| t.app == i);
                div()
                    .id(("app-row", i))
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.5))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.go(
                            if i == SETTINGS {
                                Page::Settings
                            } else {
                                Page::App(i)
                            },
                            cx,
                        )
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| this.toggle_pin(i, cx)),
                    )
                    .child(
                        div()
                            .size(px(46.0))
                            .rounded_full()
                            .bg(rgb(a.color.unwrap_or(self.accent)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(glyph(a.icon.clone(), 22.0, white())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(26.0))
                            .font_weight(FontWeight::LIGHT)
                            .child(a.name),
                    )
                    .when(pinned, |d| {
                        d.child(div().text_size(px(12.0)).text_color(accent).child("pinned"))
                    })
            })
            .collect();

        div()
            .id("apps-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(MARGIN))
            .child(pivot_header(
                "RIGHT-CLICK AN APP TO PIN IT".into(),
                "apps".into(),
            ))
            .children(rows)
    }

    fn settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let accent = rgb(self.accent);

        let themes: Vec<_> = [("dark", true), ("light", false)]
            .into_iter()
            .map(|(name, dark)| {
                div()
                    .id(name)
                    .text_size(px(34.0))
                    .font_weight(FontWeight::LIGHT)
                    .cursor_pointer()
                    .text_color(if self.dark == dark {
                        accent.into()
                    } else {
                        fg.opacity(0.5)
                    })
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dark = dark;
                        cx.notify();
                    }))
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

        let caption = |s: &'static str| {
            div()
                .mt(px(14.0))
                .mb(px(6.0))
                .text_size(px(15.0))
                .text_color(fg.opacity(0.6))
                .child(s)
        };

        div()
            .id("settings-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(MARGIN))
            .child(pivot_header("SETTINGS".into(), "theme".into()))
            .child(caption("background"))
            .child(div().flex().gap(px(24.0)).children(themes))
            .child(caption("accent color"))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(swatches),
            )
    }

    fn app_page(&self, i: usize) -> impl IntoElement {
        let a = &APPS[i];
        div()
            .size_full()
            .px(px(MARGIN))
            .child(pivot_header(a.name.to_uppercase(), a.name.to_lowercase()))
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
                            .bg(rgb(a.color.unwrap_or(self.accent)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(glyph(a.icon.clone(), 60.0, white())),
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
}

// ───────────────────────────── root ─────────────────────────────

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

        let page: AnyElement = match self.page {
            Page::Start => {
                let (layout, rows) = self.pack();
                self.start_page(&layout, rows, window, cx)
                    .into_any_element()
            }
            Page::Apps => self.apps_page(cx).into_any_element(),
            Page::Settings => self.settings_page(cx).into_any_element(),
            Page::App(i) => self.app_page(i).into_any_element(),
        };

        // Turnstile-lite: new pages slide in from the right while fading up.
        let animated = div().size_full().child(page).with_animation(
            ("page", self.nav),
            Animation::new(Duration::from_millis(320)).with_easing(ease_out_quint()),
            |el, d| el.left(px(36.0 * (1.0 - d))).opacity(d),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(self.bg())
            .text_color(self.fg())
            .font_family("Segoe UI")
            .child(self.status_bar())
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
                    .child(animated),
            )
            .child(self.nav_bar(cx))
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
