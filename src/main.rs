//! Windows 10 "Metro" Start screen, built on GPUI Kit (https://gpui-kit.com).
//!
//! The crate is split into feature modules; this file is the integration point:
//! it wires the modules together, defines the top-level `Metro` application
//! state and opens the host window.
//!
//! Layout
//!   * Tiles live on a grid of small cells (Small, Medium, Wide, Large) and the
//!     grid auto-fits the window width on every frame.
//!   * A first-fit packer flows tiles into the grid, so gaps are back-filled.
//!   * A selectable wallpaper sits behind the tiles.
//!
//! Module map
//!   * `assets`      embedded icons
//!   * `config`      persisted settings and JSON I/O
//!   * `data`        app catalogue and layout constants
//!   * `input`       pointer gestures, long-press and quick toggles
//!   * `lock`        PIN lock state
//!   * `lock_screen` PIN lock screen overlay
//!   * `model`       tiles, pages and transient UI state types
//!   * `motion`      easing / colour helpers and animated primitives
//!   * `navigation`  page changes, app launch and task switcher
//!   * `overlays`    tile menus, edit bar, add sheet, launch / switcher layers
//!   * `pages`       All Apps list, app page and book-page transitions
//!   * `panels`      system and notification panels
//!   * `prelude`     shared imports
//!   * `render`      root view composition
//!   * `settings`    settings page
//!   * `state`       Metro construction, layout packing and persistence
//!   * `theme`       wallpaper and theme colours
//!   * `tiles`       status bar, navigation bar, tiles and Start page
//!   * `wallpapers`  wallpaper discovery on disk

mod assets;
mod config;
mod data;
mod input;
mod lock;
mod lock_screen;
mod model;
mod motion;
mod navigation;
mod overlays;
mod pages;
mod panels;
mod prelude;
mod render;
mod settings;
mod state;
mod theme;
mod tiles;
mod wallpapers;

use crate::prelude::*;

pub(crate) struct Metro {
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
    liquid_glass: bool,
    auto_adjust_tiles: bool,
    tile_borders: bool,
    force_accent_live_tiles: bool,
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
    // Last root-relative pointer position, used to place manual tile drops.
    pointer: (f32, f32),
    // app search
    search: String,
    search_focus: FocusHandle,
    search_pending: bool,
    app_sort: AppSort,
    app_sort_menu: bool,
    lock: LockState,
    boot_visible: bool,
    shutdown_confirm: bool,
    // tile editing / long-press on the Start screen
    editing: bool,
    tile_hold: Option<TileHold>,
    hold_token: usize,
    pin_delete_held: bool,
    pin_delete_used: bool,
    tile_menu: Option<TileMenu>,
    suppress_click: Option<usize>,
    empty_hold: Option<(usize, f32, f32)>,
    empty_token: usize,
    add_sheet: bool,
    suppress_page_click: bool,
    // wallpapers found on disk
    wallpaper_files: Vec<PathBuf>,
    wallpaper_file: Option<PathBuf>,
    // debounced config persistence
    save_token: usize,
}

/// Restore bounds for the Lumia shell's fullscreen root window.
fn main_window_bounds(cx: &App) -> Bounds<Pixels> {
    cx.primary_display()
        .map(|display| display.bounds())
        // No display information (headless): keep a sensible restore size.
        .unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(800.0)), cx))
}

fn main() {
    application().with_assets(AppAssets).run(|cx| {
        init(cx);

        let bounds = main_window_bounds(cx);
        open_window(
            WindowOptions {
                // The primary Metro window is the desktop shell, not an app window:
                // ask the host compositor to put it on the entire display at startup.
                window_bounds: Some(WindowBounds::Fullscreen(bounds)),
                window_min_size: Some(size(px(320.0), px(480.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Lumia Shell".into()),
                    ..Default::default()
                }),
                app_id: Some("com.horn.lumia-shell".into()),
                is_movable: false,
                is_resizable: false,
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(Metro::new),
        )
        .expect("failed to open window");
    });
}

#[cfg(test)]
mod tests;
