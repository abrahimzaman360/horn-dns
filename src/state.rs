//! Metro construction, layout packing and debounced persistence.

use crate::prelude::*;

impl Metro {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        // Restore the last session; a missing or broken file means first run.
        let config = load_config().unwrap_or_default();
        let tiles = tiles_from_config(&config.tiles);
        let next_id = tiles.len();
        let lock = LockState::new(
            config.pin_hash.clone(),
            config.pin_len,
            config.lock_wallpaper.as_deref(),
            resolve_wallpaper,
        );

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

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(7000))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.boot_visible = false;
                this.lock.lock_now();
                this.lock.locked = true;
                this.launch = None;
                this.panel = None;
                this.panel_closing = false;
                this.switcher = false;
                this.switcher_closing = false;
                this.edit = None;
                this.editing = false;
                this.tile_menu = None;
                this.add_sheet = false;
                this.history.clear();
                this.page = Page::Start;
                this.prev_page = None;
                this.page_morph = false;
                this.nav += 1;
                cx.notify();
            });
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
            brightness: config.brightness,
            liquid_glass: config.liquid_glass,
            auto_adjust_tiles: config.auto_adjust_tiles,
            tile_borders: config.tile_borders,
            force_accent_live_tiles: config.force_accent_live_tiles,
            wifi: true,
            bluetooth: false,
            airplane: false,
            quiet: false,
            night_light: false,
            battery_saver: false,
            notes_cleared: false,
            running: Vec::new(),
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
            wallpaper: config.wallpaper,
            wallpaper_file: config.wallpaper_file.as_deref().and_then(resolve_wallpaper),
            wallpaper_files: scan_wallpapers(),
            gap: config.gap,
            tile_alpha: config.tile_alpha,
            pointer: (0.0, 0.0),
            search: String::new(),
            search_focus: cx.focus_handle(),
            search_pending: false,
            app_sort: config.app_sort,
            app_sort_menu: false,
            lock,
            boot_visible: true,
            shutdown_confirm: false,
            editing: false,
            tile_hold: None,
            hold_token: 0,
            pin_delete_held: false,
            pin_delete_used: false,
            tile_menu: None,
            suppress_click: None,
            empty_hold: None,
            empty_token: 0,
            add_sheet: false,
            suppress_page_click: false,
            save_token: 0,
        };
        me.stamp();
        me
    }

    /// Local wall-clock via jiff 1.x (system time zone).
    /// Note: jiff 1.0 renamed `Zoned` to `ZonedDateTime`.
    pub(crate) fn stamp(&mut self) {
        let now = jiff::ZonedDateTime::now();
        self.time = now.strftime("%-I:%M").to_string();
        self.date = now.strftime("%A, %B %-d, %Y").to_string();
    }

    pub(crate) fn heartbeat(&mut self, cx: &mut Context<Self>) {
        self.secs += 1;
        self.drop_handled = false;
        self.stamp();
        if self.edit.is_none() && !self.editing {
            let secs = self.secs;
            for t in self.tiles.iter_mut() {
                if t.live_enabled && t.faces.len() > 1 && (secs + t.offset).is_multiple_of(6) {
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
    pub(crate) fn relayout(&mut self, width: f32) {
        let gap = self.gap;
        let avail = (width - 2.0 * TILE_MARGIN).max(200.0);
        let cols =
            ((((avail + gap) / (TARGET_CELL + gap)) / 2.0).round() as usize * 2).clamp(4, 16);
        self.cell = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        self.cols = cols;
        self.scale = (self.cell / 70.0).clamp(0.85, 1.5);
        self.resizing = (width - self.last_w).abs() > 0.5;
        self.last_w = width;
    }

    /// Height of the page area (between status bar and nav bar).
    pub(crate) fn page_h(&self) -> f32 {
        if self.measured_h > 0.0 {
            self.measured_h
        } else {
            (self.vh - STATUS_H - NAV_H).max(100.0)
        }
    }

    /// First-fit packer: every tile goes to the first free spot (row-major),
    /// so smaller tiles back-fill holes. Returns (col, row) per tile and the row count.
    pub(crate) fn pack(&self) -> (Vec<(usize, usize)>, usize) {
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
            for row in occ.iter_mut().skip(r).take(ch) {
                for cell in row.iter_mut().skip(c).take(cw) {
                    *cell = true;
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

    /// Manual tile positions are fractions of available horizontal travel and vertical grid rows.
    pub(crate) fn tile_layout(&self) -> (Vec<(f32, f32)>, usize) {
        let (packed, packed_rows) = self.pack();
        (
            packed
                .into_iter()
                .zip(self.tiles.iter())
                .map(|((col, row), tile)| {
                    let max_col = self.cols.saturating_sub(tile.size.cells().0).max(1);
                    (col as f32 / max_col as f32, row as f32)
                })
                .collect(),
            packed_rows,
        )
    }

    pub(crate) fn seed_new_manual_position(&mut self, id: usize) {
        let Some(ix) = self.tiles.iter().position(|tile| tile.id == id) else {
            return;
        };
        let (cw, ch) = self.tiles[ix].size.cells();
        let current = self.tile_layout().0;
        let max_col = self.cols.saturating_sub(cw).max(1);
        for row in 0..MAX_MANUAL_ROWS as usize {
            for col in 0..=self.cols.saturating_sub(cw) {
                let left = col as f32;
                let top = row as f32;
                let occupied = self.tiles.iter().enumerate().any(|(other_ix, other)| {
                    if other_ix == ix {
                        return false;
                    }
                    let (ow, oh) = other.size.cells();
                    let (ox, oy) = current[other_ix];
                    let oleft = ox * self.cols.saturating_sub(ow).max(1) as f32;
                    left < oleft + ow as f32
                        && left + cw as f32 > oleft
                        && top < oy + oh as f32
                        && top + ch as f32 > oy
                });
                if !occupied {
                    self.tiles[ix].manual_x = Some(left / max_col as f32);
                    self.tiles[ix].manual_y = Some(top);
                    return;
                }
            }
        }
    }

    pub(crate) fn resize_tile(&mut self, id: usize, size: TileSize, cx: &mut Context<Self>) {
        let Some(ix) = self.tiles.iter().position(|tile| tile.id == id) else {
            return;
        };
        let old_cols = self.tiles[ix].size.cells().0;
        let old_x = self.tiles[ix].manual_x;
        let (new_cols, _) = size.cells();
        if let Some(x) = old_x {
            let step = self.cell + self.gap;
            let old_left = x * (self.cols.saturating_sub(old_cols) as f32) * step;
            let new_max = (self.cols.saturating_sub(new_cols) as f32) * step;
            self.tiles[ix].manual_x = Some(if new_max > 0.0 {
                (old_left / new_max).clamp(0.0, 1.0)
            } else {
                0.0
            });
        }
        self.tiles[ix].size = size;
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn seed_manual_positions(&mut self) {
        let packed = self.pack().0;
        for (tile, (col, row)) in self.tiles.iter_mut().zip(packed) {
            let max_col = self.cols.saturating_sub(tile.size.cells().0).max(1);
            tile.manual_x.get_or_insert(col as f32 / max_col as f32);
            tile.manual_y.get_or_insert(row as f32);
        }
    }

    pub(crate) fn place_tile_at_pointer(&mut self, id: usize, cx: &mut Context<Self>) {
        let Some(ix) = self.tiles.iter().position(|tile| tile.id == id) else {
            return;
        };
        let (cw, _) = self.tiles[ix].size.cells();
        let cell_step = self.cell + self.gap;
        let canvas_x = (self.pointer.0 - TILE_MARGIN).max(0.0);
        let canvas_y =
            (self.pointer.1 - STATUS_H - 8.0 - f32::from(self.start_scroll.offset().y)).max(0.0);
        let col = ((canvas_x / cell_step).round() as usize).min(self.cols.saturating_sub(cw));
        let row = (canvas_y / cell_step).round() as usize;

        let target = self.tiles.remove(ix);
        let packed = self.pack().0;
        let mut nearest = 0usize;
        let mut nearest_distance = usize::MAX;
        for (index, &(c, r)) in packed.iter().enumerate() {
            if index == ix {
                continue;
            }
            let distance = c.abs_diff(col) + r.abs_diff(row);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = index.min(self.tiles.len());
            }
        }
        nearest = nearest.min(self.tiles.len());
        self.tiles.insert(nearest, target);

        let packed = self.pack().0;
        for (tile, (col, row)) in self.tiles.iter_mut().zip(packed) {
            let max_col = self.cols.saturating_sub(tile.size.cells().0).max(1);
            tile.manual_x = Some(col as f32 / max_col as f32);
            tile.manual_y = Some(row as f32);
        }
        self.edit = None;
        self.save_soon(cx);
        cx.notify();
    }

    /// Animated value for any keyed channel. While the window itself is being resized the
    /// tween is effectively instant, so layouts track the window edge without lag.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tween_in(
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
    pub(crate) fn tween(
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
    pub(crate) fn move_tile(&mut self, from_id: usize, to_id: usize, cx: &mut Context<Self>) {
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

    pub(crate) fn move_to_end(&mut self, from_id: usize, cx: &mut Context<Self>) {
        if let Some(from) = self.tiles.iter().position(|t| t.id == from_id) {
            let t = self.tiles.remove(from);
            self.tiles.push(t);
        }
        self.save_soon(cx);
        cx.notify();
    }

    // ── persistence ──

    /// Snapshot of everything worth keeping between runs.
    pub(crate) fn config(&self) -> Config {
        Config {
            version: CONFIG_VERSION,
            dark: self.dark,
            accent: self.accent,
            wallpaper: self.wallpaper,
            wallpaper_file: self
                .wallpaper_file
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            gap: self.gap,
            tile_alpha: self.tile_alpha,
            liquid_glass: self.liquid_glass,
            auto_adjust_tiles: self.auto_adjust_tiles,
            tile_borders: self.tile_borders,
            force_accent_live_tiles: self.force_accent_live_tiles,
            app_sort: self.app_sort,
            pin_hash: self.lock.pin_hash.map(|hash| format!("{hash:016x}")),
            pin_len: self.lock.pin_len,
            lock_wallpaper: self
                .lock
                .wallpaper
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            brightness: self.brightness,
            tiles: self
                .tiles
                .iter()
                .map(|t| TileConfig {
                    app: APPS[t.app].name.to_string(),
                    size: t.size,
                    live_enabled: t.live_enabled,
                    manual_x: t.manual_x,
                    manual_y: t.manual_y,
                })
                .collect(),
        }
    }

    /// Debounced save: slider drags and rapid edits collapse into one write.
    pub(crate) fn save_soon(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn rebuild_tiles(&mut self, configs: &[TileConfig]) {
        self.tiles = tiles_from_config(configs);
        self.next_id = self.tiles.len();
        self.edit = None;
    }

    pub(crate) fn reset_config(&mut self, cx: &mut Context<Self>) {
        let defaults = Config::default();
        self.dark = defaults.dark;
        self.accent = defaults.accent;
        self.wallpaper = defaults.wallpaper;
        self.wallpaper_file = None;
        self.gap = defaults.gap;
        self.tile_alpha = defaults.tile_alpha;
        self.liquid_glass = defaults.liquid_glass;
        self.auto_adjust_tiles = defaults.auto_adjust_tiles;
        self.tile_borders = defaults.tile_borders;
        self.force_accent_live_tiles = defaults.force_accent_live_tiles;
        self.app_sort = defaults.app_sort;
        self.app_sort_menu = false;
        self.lock.reset_defaults();
        self.brightness = defaults.brightness;
        self.editing = false;
        self.edit = None;
        self.tile_menu = None;
        self.add_sheet = false;
        self.rebuild_tiles(&defaults.tiles);
        self.save_soon(cx);
        cx.notify();
    }

    // ── navigation ──
}
