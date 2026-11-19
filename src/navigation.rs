//! Page navigation, app launching and the running-app switcher.

use crate::prelude::*;

impl Metro {
    pub(crate) fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.lock.locked || self.page == page {
            return;
        }
        self.history.push(self.page);
        self.flip_to(page, cx);
    }

    /// Remember an app as "running" (most recent first).
    pub(crate) fn note_running(&mut self, page: Page) {
        if let Some(app) = app_of(page) {
            self.running.retain(|&a| a != app);
            self.running.insert(0, app);
            self.running.truncate(MAX_RUNNING);
        }
    }

    /// Swap pages with the book-page (Windows Phone) reveal: the outgoing page is
    /// kept alive for the duration of the peel, then dropped.
    pub(crate) fn flip_to(&mut self, page: Page, cx: &mut Context<Self>) {
        self.prev_page = Some(self.page);
        self.page = page;
        self.nav += 1;
        self.nav_at = Instant::now();
        self.edit = None;
        self.editing = false;
        self.tile_menu = None;
        self.add_sheet = false;
        self.tile_hold = None;
        self.empty_hold = None;
        self.page_morph = true;
        if page == Page::Settings {
            // Pick up wallpapers dropped into the folder since launch.
            self.wallpaper_files = scan_wallpapers();
        }
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

    pub(crate) fn back(&mut self, cx: &mut Context<Self>) {
        if self.lock_back(cx) {
            return;
        }
        if self.launch.is_some() {
            return;
        }
        if self.tile_menu.is_some() || self.add_sheet || self.editing {
            self.exit_editing(cx);
        } else if self.panel.is_some() {
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

    pub(crate) fn lock_now(&mut self, cx: &mut Context<Self>) {
        self.lock.lock_now();
        self.panel = None;
        self.panel_closing = false;
        self.switcher = false;
        self.launch = None;
        self.edit = None;
        self.editing = false;
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn lock_back(&mut self, cx: &mut Context<Self>) -> bool {
        if self.lock.locked {
            if self.lock.pin_prompt {
                self.lock.back_to_overlay();
            } else {
                self.lock.backspace();
            }
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn home(&mut self, cx: &mut Context<Self>) {
        if self.launch.is_some() {
            return;
        }
        self.history.clear();
        self.editing = false;
        self.tile_menu = None;
        self.add_sheet = false;
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

    pub(crate) fn open_app(&mut self, app: usize, cx: &mut Context<Self>) {
        if self.boot_visible || self.lock.locked {
            return;
        }
        self.panel = None;
        self.panel_closing = false;
        self.panel_token += 1;
        self.begin_launch(
            Launch {
                app,
                tile: None,
                from: (0, 0),
                rect: (MARGIN, STATUS_H + 72.0, 46.0, 46.0),
                n: 0,
            },
            cx,
        );
    }

    pub(crate) fn toggle_pin(&mut self, app: usize, cx: &mut Context<Self>) {
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
            if !self.auto_adjust_tiles {
                self.seed_new_manual_position(self.next_id - 1);
            }
        }
        self.save_soon(cx);
        cx.notify();
    }

    // ── app launch (zoom + turnstile) ──

    /// Start the launch splash for a tapped Start tile.
    pub(crate) fn launch_from_tile(&mut self, id: usize, cx: &mut Context<Self>) {
        let (layout, _) = self.tile_layout();
        let Some(ix) = self.tiles.iter().position(|t| t.id == id) else {
            return;
        };
        let t = &self.tiles[ix];
        let (c, r) = layout[ix];
        let (cw, ch) = t.size.cells();
        let (cell, g) = (self.cell, self.gap);
        let max_x = (self.cols as f32 - cw as f32).max(0.0) * (cell + g);
        let x = TILE_MARGIN + c * max_x;
        let y = STATUS_H + 8.0 + r * (cell + g) + f32::from(self.start_scroll.offset().y);
        let w = cw as f32 * cell + (cw as f32 - 1.0) * g;
        let h = ch as f32 * cell + (ch as f32 - 1.0) * g;
        let app = t.app;
        self.begin_launch(
            Launch {
                app,
                tile: Some(id),
                from: (c.round() as usize, r.round() as usize),
                rect: (x, y, w, h),
                n: 0,
            },
            cx,
        );
    }

    /// Start the launch splash from a card in the running-apps switcher.
    pub(crate) fn launch_from_card(&mut self, app: usize, cx: &mut Context<Self>) {
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
    pub(crate) fn begin_launch(&mut self, mut l: Launch, cx: &mut Context<Self>) {
        if self.boot_visible || self.lock.locked || self.launch.is_some() {
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
    pub(crate) fn commit_open(&mut self, app: usize, cx: &mut Context<Self>) {
        if self.boot_visible || self.lock.locked {
            self.launch = None;
            cx.notify();
            return;
        }
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

    pub(crate) fn open_switcher(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn close_switcher(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn dismiss_app(&mut self, app: usize, cx: &mut Context<Self>) {
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
    pub(crate) fn home_down(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn switcher_geo(&self, width: f32) -> SwGeo {
        let gap = 12.0;
        let avail = (width - 2.0 * MARGIN).max(160.0);
        let cols = (((avail + gap) / (170.0 + gap)).floor() as usize).clamp(2, 6);
        let cw = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        SwGeo {
            cols,
            cw,
            ch: (cw * 1.4).round(),
            gap,
            top: 52.0,
        }
    }

    pub(crate) fn card_xy(i: usize, g: SwGeo) -> (f32, f32) {
        (
            MARGIN + (i % g.cols) as f32 * (g.cw + g.gap),
            g.top + (i / g.cols) as f32 * (g.ch + g.gap),
        )
    }

    // ── panels ──
}
