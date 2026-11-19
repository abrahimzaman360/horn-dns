//! Tile menus, edit bar, add sheet, launch plate and switcher cards.

use crate::prelude::*;

impl Metro {
    pub(crate) fn tile_menu_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.editing && self.edit.is_none() {
            return None;
        }
        let menu = self.tile_menu?;
        let tile = self.tiles.iter().find(|t| t.id == menu.id)?;
        let app = &APPS[tile.app];
        let fg = self.fg();
        let width = 196.0;
        let height = 274.0;
        let (layout, _) = self.tile_layout();
        let tile_ix = self.tiles.iter().position(|t| t.id == menu.id)?;
        let pos = layout[tile_ix];
        let (cw, ch) = tile.size.cells();
        let max_x = (self.cols as f32 - cw as f32).max(0.0) * (self.cell + self.gap);
        let tile_x = TILE_MARGIN + pos.0 * max_x;
        let tile_y = STATUS_H + 8.0 - f32::from(self.start_scroll.offset().y)
            + pos.1 * (self.cell + self.gap);
        let tile_w = cw as f32 * self.cell + (cw as f32 - 1.0) * self.gap;
        let tile_h = ch as f32 * self.cell + (ch as f32 - 1.0) * self.gap;
        let preferred_x = tile_x + tile_w + 8.0;
        let preferred_y = tile_y + tile_h * 0.5 - height * 0.5;
        let x = if preferred_x + width <= self.measured_w {
            preferred_x
        } else {
            (tile_x - width - 8.0).max(8.0)
        }
        .min(self.measured_w - width - 8.0)
        .max(8.0);
        let y = preferred_y
            .min(self.vh - NAV_H - height - 8.0)
            .max(STATUS_H + 8.0);
        let tile_id = tile.id;

        let sizes = [
            (TileSize::Small, "Small"),
            (TileSize::Medium, "Medium"),
            (TileSize::Wide, "Wide"),
            (TileSize::Large, "Full"),
        ];
        let live_enabled = tile.live_enabled;
        let mut rows: Vec<AnyElement> = Vec::with_capacity(sizes.len() + 2);
        for (slot, (size, name)) in sizes.into_iter().enumerate() {
            let current = tile.size == size;
            rows.push(
                div()
                    .id(("menu-size", slot))
                    .h(px(34.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(2.0))
                    .cursor_pointer()
                    .when(current, |d| d.bg(self.cell_bg()))
                    .active(|s| s.opacity(0.6))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.resize_tile(tile_id, size, cx);
                        this.tile_menu = None;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(if current { fg } else { fg.opacity(0.85) })
                            .child(name),
                    )
                    .when(current, |d| {
                        d.child(div().text_size(px(13.0)).text_color(fg).child("✓"))
                    })
                    .into_any_element(),
            );
        }

        rows.push(
            div()
                .id("menu-live-tile")
                .h(px(34.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(2.0))
                .cursor_pointer()
                .when(live_enabled, |d| d.bg(self.cell_bg()))
                .active(|s| s.opacity(0.6))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tile) = this.tiles.iter_mut().find(|tile| tile.id == tile_id) {
                        tile.live_enabled = !tile.live_enabled;
                        tile.prev = tile.idx;
                        tile.tick = 0;
                    }
                    this.tile_menu = None;
                    this.save_soon(cx);
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(fg.opacity(0.85))
                        .child(if live_enabled {
                            "Disable Live Tile"
                        } else {
                            "Enable Live Tile"
                        }),
                )
                .when(live_enabled, |d| {
                    d.child(div().text_size(px(13.0)).text_color(fg).child("✓"))
                })
                .into_any_element(),
        );

        let card = div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .p(px(6.0))
            .bg(self.panel_bg())
            .rounded(px(2.0))
            .border_1()
            .border_color(fg.opacity(0.18))
            .shadow_lg()
            .occlude()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(4.0))
                    .pb(px(6.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg.opacity(0.55))
                            .child("TILE SIZE"),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(app.name),
                    ),
            )
            .children(rows)
            .child(div().h(px(1.0)).my(px(4.0)).bg(fg.opacity(0.12)))
            .child(
                div()
                    .id("menu-unpin")
                    .h(px(34.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(2.0))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(fg.opacity(0.85))
                            .child("Unpin from Start"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tiles.retain(|t| t.id != tile_id);
                        this.tile_menu = None;
                        this.edit = None;
                        this.save_soon(cx);
                        cx.notify();
                    })),
            );

        let catcher = div()
            .id("tile-menu-catcher")
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.tile_menu = None;
                    cx.notify();
                }),
            );

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .child(catcher)
                .child(card)
                .into_any_element(),
        )
    }

    // ── tile edit mode: bottom bar + Add sheet ──

    pub(crate) fn edit_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.editing || self.page != Page::Start {
            return None;
        }
        let fg = self.fg();
        let accent: Hsla = rgb(self.accent).into();
        let ink = on_color(self.accent);

        let step = |id: &'static str, label: &'static str, delta: f32| {
            div()
                .id(id)
                .size(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(2.0))
                .bg(self.cell_bg())
                .cursor_pointer()
                .active(|s| s.opacity(0.6))
                .child(div().text_size(px(16.0)).text_color(fg).child(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.gap = (this.gap + delta).clamp(GAP_MIN, GAP_MAX);
                    this.save_soon(cx);
                    cx.notify();
                }))
        };

        Some(
            div()
                .id("edit-bar")
                .absolute()
                .left(px(TILE_MARGIN))
                .right(px(TILE_MARGIN))
                .bottom(px(NAV_H + 10.0))
                .h(px(54.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .bg(self.panel_bg())
                .rounded(px(2.0))
                .border_1()
                .border_color(fg.opacity(0.16))
                .shadow_lg()
                .occlude()
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg.opacity(0.6))
                        .child("Gap"),
                )
                .child(step("gap-less", "−", -2.0))
                .child(
                    div()
                        .w(px(42.0))
                        .text_center()
                        .text_size(px(12.0))
                        .text_color(fg.opacity(0.85))
                        .child(format!("{} px", self.gap.round() as u32)),
                )
                .child(step("gap-more", "+", 2.0))
                .child(div().flex_1())
                .child(
                    div()
                        .id("edit-add")
                        .h(px(34.0))
                        .px(px(14.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(2.0))
                        .bg(accent)
                        .cursor_pointer()
                        .active(|s| s.opacity(0.7))
                        .child(div().text_size(px(13.0)).text_color(ink).child("Add +"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.add_sheet = !this.add_sheet;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn add_sheet_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.add_sheet || !self.editing || self.page != Page::Start {
            return None;
        }
        let fg = self.fg();
        let unpinned: Vec<usize> = (0..APPS.len())
            .filter(|&i| !self.tiles.iter().any(|t| t.app == i))
            .collect();

        let mut items: Vec<AnyElement> = Vec::with_capacity(unpinned.len());
        for i in unpinned {
            let app = &APPS[i];
            let hex = app.color.unwrap_or(self.accent);
            items.push(
                div()
                    .id(("add-app", i))
                    .w(px(72.0))
                    .py(px(6.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(2.0))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_pin(i, cx);
                        this.add_sheet = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .size(px(46.0))
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(fg.opacity(0.25))
                            .bg(rgb(hex))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(glyph(app.icon, 22.0, on_color(hex))),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(px(11.0))
                            .text_center()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(app.name),
                    )
                    .into_any_element(),
            );
        }

        let body: AnyElement = if items.is_empty() {
            div()
                .w_full()
                .py(px(24.0))
                .text_center()
                .text_size(px(12.0))
                .text_color(fg.opacity(0.55))
                .child("Every app is already pinned")
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .children(items)
                .into_any_element()
        };

        Some(
            div()
                .id("add-sheet")
                .absolute()
                .left(px(TILE_MARGIN))
                .right(px(TILE_MARGIN))
                .bottom(px(NAV_H + 72.0))
                .max_h(px(300.0))
                .overflow_y_scroll()
                .px(px(12.0))
                .pt(px(10.0))
                .pb(px(12.0))
                .bg(self.panel_bg())
                .rounded(px(2.0))
                .border_1()
                .border_color(fg.opacity(0.16))
                .shadow_lg()
                .occlude()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg.opacity(0.6))
                                .child("Add to Start"),
                        )
                        .child(
                            div()
                                .id("add-sheet-close")
                                .size(px(26.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(self.cell_bg())
                                .cursor_pointer()
                                .active(|s| s.opacity(0.6))
                                .child(glyph(IconName::X, 13.0, fg))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.add_sheet = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(body)
                .into_any_element(),
        )
    }
    // ── launch splash ──

    /// The coloured plate that grows from the tapped tile / card to the full page area,
    /// then fades away to reveal the freshly opened app.
    pub(crate) fn launch_layer(&self, width: f32) -> Option<AnyElement> {
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
                .child(glyph(a.icon, icon_px, tile_ink(hex)))
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
    pub(crate) fn switcher_card(
        &self,
        i: usize,
        app: usize,
        g: SwGeo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let a = &APPS[app];
        let hex = a.color.unwrap_or(self.accent);
        let ink = tile_ink(hex);
        let inverse = tile_inverse_ink(hex);
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

    pub(crate) fn switcher_layer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
                    .text_size(px(28.0))
                    .line_height(px(32.0))
                    .font_weight(FontWeight::LIGHT)
                    .text_color(white())
                    .child("Running Apps"),
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
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .bottom(px(12.0))
                            .w_full()
                            .flex()
                            .justify_center()
                            .child(
                                div()
                                    .id("running-clear-all")
                                    .h(px(34.0))
                                    .px(px(18.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(hsla(0.0, 0.0, 1.0, 0.14))
                                    .cursor_pointer()
                                    .active(|s| s.opacity(0.6))
                                    .text_size(px(12.0))
                                    .text_color(white())
                                    .child("Clear all")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.running.clear();
                                        this.dismissing = None;
                                        cx.notify();
                                    })),
                            ),
                    )
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

    pub(crate) fn app_sort_menu_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.page != Page::Apps || !self.app_sort_menu {
            return None;
        }
        let fg = self.fg();
        let category = div()
            .id("sort-category-overlay")
            .h(px(36.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(2.0))
            .cursor_pointer()
            .when(self.app_sort == AppSort::Category, |d| d.bg(self.cell_bg()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.app_sort = AppSort::Category;
                this.app_sort_menu = false;
                this.save_soon(cx);
                cx.notify();
            }))
            .child(div().text_size(px(12.0)).child("Category"));

        let alphabetical = div()
            .id("sort-alphabetical-overlay")
            .h(px(36.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(2.0))
            .cursor_pointer()
            .when(self.app_sort == AppSort::Alphabetical, |d| {
                d.bg(self.cell_bg())
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.app_sort = AppSort::Alphabetical;
                this.app_sort_menu = false;
                this.save_soon(cx);
                cx.notify();
            }))
            .child(div().text_size(px(12.0)).child("Alphabetically"));

        let width = 176.0;
        let right = (TILE_MARGIN).max(8.0);
        let top = STATUS_H + 8.0 + 72.0 - 8.0;
        Some(
            div()
                .id("apps-sort-overlay")
                .absolute()
                .right(px(right))
                .top(px(top))
                .w(px(width))
                .p(px(6.0))
                .rounded(px(4.0))
                .bg(self.panel_bg())
                .border_1()
                .border_color(fg.opacity(0.16))
                .shadow_lg()
                .occlude()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(28.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg.opacity(0.55))
                        .child("Sort by"),
                )
                .child(category)
                .child(alphabetical)
                .into_any_element(),
        )
    }
}
