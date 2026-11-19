//! Status bar, navigation bar, tiles and the Start page.

use crate::prelude::*;

impl Metro {
    /// Always painted above everything else (see `render`): it never hides on its
    /// own and stays visible while a panel is slid down underneath it.
    pub(crate) fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
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
                hsla(0.0, 0.0, 0.05, 0.16)
            } else {
                hsla(0.0, 0.0, 1.0, 0.22)
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
                    .absolute()
                    .left_0()
                    .right_0()
                    .mx_auto()
                    .w(px(22.0))
                    .h(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(14.0))
                            .rounded_full()
                            .bg(hsla(0.0, 0.0, 0.02, 1.0))
                            .border_1()
                            .border_color(hsla(0.0, 0.0, 0.12, 1.0)),
                    ),
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

    pub(crate) fn nav_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let back_disabled = self.page == Page::Start
            && !self.editing
            && self.tile_menu.is_none()
            && !self.add_sheet
            && self.panel.is_none()
            && !self.switcher
            && self.edit.is_none()
            && self.launch.is_none();
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
                    .child(glyph(
                        IconName::ArrowLeft,
                        26.0,
                        if back_disabled { fg.opacity(0.25) } else { fg },
                    ))
                    .when(!back_disabled, |d| {
                        d.cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.back(cx)))
                    }),
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

    /// Base plate colour for a tile, honouring the "Force Accent Color" option.
    pub(crate) fn tile_color_hex(&self, t: &Tile) -> u32 {
        if self.force_accent_live_tiles && t.live_enabled {
            self.accent
        } else {
            APPS[t.app].color.unwrap_or(self.accent)
        }
    }

    /// Colour the ink decision should use once liquid-glass translucency is
    /// taken into account. Gradient presets have a known backdrop, so we can
    /// composite the tile over it; user wallpapers are arbitrary, so we keep the
    /// tile's own swatch and preserve Metro's white-on-colour look.
    pub(crate) fn tile_bg_hex(&self, color_hex: u32) -> u32 {
        if !self.liquid_glass || self.tile_alpha >= 0.999 || self.wallpaper_file.is_some() {
            return color_hex;
        }
        let (from, to) = self.wallpaper_colors();
        let behind = blend_hex(from, to, 0.5);
        blend_hex(behind, color_hex, self.tile_alpha)
    }

    pub(crate) fn face(&self, face: &Face, t: &Tile, ink: Hsla) -> Div {
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
                let caption_ink = tile_ink(cols[((r - 1) * c) % 4]);
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
            Face::Music => match t.size {
                TileSize::Small | TileSize::Medium => base
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(app.icon, 56.0 * s, ink))
                    .when(!small, |d| d.child(label(app.name, s))),
                TileSize::Wide => base
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(10.0 * s))
                    .child(glyph(app.icon, 44.0 * s, ink))
                    .when(t.live_enabled, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0 * s))
                                .child(
                                    div()
                                        .text_size(px(12.0 * s))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_ellipsis()
                                        .child("Neon Rain"),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.0 * s))
                                        .text_color(ink)
                                        .text_ellipsis()
                                        .child("Fishmind"),
                                ),
                        )
                    })
                    .child(label(app.name, s)),
                TileSize::Large => base
                    .p(px(14.0 * s))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.0 * s))
                    .when(t.live_enabled, |d| {
                        d.child(div().flex().items_center().justify_between().child(glyph(
                            app.icon,
                            38.0 * s,
                            ink,
                        )))
                    })
                    .when(t.live_enabled, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(4.0 * s))
                                .child(
                                    div()
                                        .text_size(px(16.0 * s))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_ellipsis()
                                        .child("Neon Rain"),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0 * s))
                                        .text_color(ink)
                                        .text_ellipsis()
                                        .child("Fishmind"),
                                ),
                        )
                    })
                    .child(label(app.name, s)),
            },
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
                            .text_size(px(((self.date.len().max(1) as f32 * -0.18 + 12.0) * s)
                                .clamp(8.0, 13.0)))
                            .line_height(px((13.0 * s).min(16.0)))
                            .text_color(ink)
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
    /// `ink` must be derived from the same plate colour used by `tile_view` so a
    /// forced accent / translucent tile never ends up with mismatched text.
    pub(crate) fn tile_content(&self, t: &Tile, h: f32, ink: Hsla) -> AnyElement {
        // Small tiles are icon-only: no captions, badges or live info.
        if t.size == TileSize::Small {
            return self.face(&Face::Icon, t, ink).into_any_element();
        }

        if t.live_enabled && t.faces.len() > 1 && t.tick > 0 {
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
    pub(crate) fn tile_view(
        &self,
        ix: usize,
        pos: (f32, f32),
        entering: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.tiles[ix];
        let id = t.id;
        let (cw, ch) = t.size.cells();
        let (cell, g) = (self.cell, self.gap);

        // target geometry from the packer …
        let max_x = (self.cols as f32 - cw as f32).max(0.0) * (cell + g);
        let tx = pos.0 * max_x;
        let ty = pos.1 * (cell + g);
        let tw = cw as f32 * cell + (cw as f32 - 1.0) * g;
        let th = ch as f32 * cell + (ch as f32 - 1.0) * g;
        // … and the animated values actually painted this frame
        let x = self.tween(id, "x", tx, window, cx);
        let y = self.tween(id, "y", ty, window, cx);
        let w = self.tween(id, "w", tw, window, cx);
        let h = self.tween(id, "h", th, window, cx);

        let selected = self.edit == Some(id);
        let editing = self.editing || selected;
        let color_hex = self.tile_color_hex(t);
        let color = rgb(color_hex).opacity(if self.liquid_glass {
            self.tile_alpha
        } else {
            1.0
        });
        // Choose ink from the colour the plate really shows (after translucency),
        // not from the app's swatch. Tiles follow Metro's white-on-colour look.
        let bg_hex = self.tile_bg_hex(color_hex);
        let ink = tile_ink(bg_hex);
        let inverse = tile_inverse_ink(bg_hex);
        let content = self.tile_content(t, h, ink);
        let payload = DraggedTile {
            id,
            w,
            h,
            color: bg_hex,
            icon: APPS[t.app].icon,
        };

        // Turnstile: tiles turn *away* (staggered by distance) when another tile is
        // launched, and turn *in* (staggered row by row) whenever Start is shown again.
        let (turn, delay) = if let Some(l) = self.launch.filter(|_| entering) {
            if l.tile == Some(id) {
                (Turn::Idle, 0.0)
            } else {
                let dist = (pos.0 - l.from.0 as f32).abs() + (pos.1 - l.from.1 as f32).abs();
                (Turn::Leave(l.n), (dist * 22.0).min(160.0))
            }
        } else if entering && self.nav_at.elapsed().as_millis() < ENTER_WINDOW_MS {
            (
                Turn::Enter(self.nav),
                (pos.1 * 45.0 + pos.0 * 12.0).min(320.0),
            )
        } else {
            (Turn::Idle, 0.0)
        };

        // Edit controls scale with the tile so a small tile doesn't get huge buttons
        // and a large tile doesn't get lost little ones.
        let (btn, icon_px): (f32, f32) = match t.size {
            TileSize::Small => (20.0, 11.0),
            TileSize::Medium | TileSize::Wide => (26.0, 14.0),
            TileSize::Large => (34.0, 18.0),
        };
        let inset = (btn * 0.18).max(3.0);
        let circle_btn = |bid: (&'static str, usize), icon: IconName| {
            div()
                .id(bid)
                .absolute()
                .size(px(btn))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(ink)
                .child(glyph(icon, icon_px, inverse))
        };

        let plate = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(color)
            .when(self.tile_borders, |d| {
                d.border_1().border_color(ink.opacity(0.55))
            })
            .when(selected, |d| d.border_2().border_color(ink))
            .child(content)
            .when(editing, |d| {
                d.child(
                    circle_btn(("unpin", id), IconName::X)
                        .top(px(inset))
                        .right(px(inset))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tiles.retain(|t| t.id != id);
                            this.edit = None;
                            this.save_soon(cx);
                            cx.stop_propagation();
                            cx.notify();
                        })),
                )
                .child(
                    circle_btn(("resize", id), IconName::ArrowLeftRight)
                        .bottom(px(inset))
                        .right(px(inset))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(t) = this.tiles.iter_mut().find(|t| t.id == id) {
                                t.size = t.size.next();
                            }
                            this.save_soon(cx);
                            cx.stop_propagation();
                            cx.notify();
                        })),
                )
            });

        let tile = div()
            .id(("tile", id))
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .cursor_pointer()
            // Metro "press" feel: the tile shrinks while held
            .active(|s| s.p(px(5.0)))
            .when(self.edit.is_some() && !selected && !self.editing, |d| {
                d.opacity(0.7)
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.tile_menu = None;
                    this.edit = Some(id);
                    this.editing = true;
                    cx.notify();
                }),
            )
            // Press-and-hold lifts the tile; dragging from there moves it, and
            // releasing without moving opens the size menu.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.tile_menu = None;
                    this.begin_tile_hold(id, f32::from(e.position.x), f32::from(e.position.y), cx);
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.suppress_click == Some(id) {
                    this.suppress_click = None;
                    cx.stop_propagation();
                    return;
                }
                if this.edit.is_some() || this.editing {
                    this.edit = None;
                    this.save_soon(cx);
                    cx.stop_propagation();
                    cx.notify();
                } else {
                    // Zoom the tile open; the rest turnstile away.
                    this.launch_from_tile(id, cx);
                }
            }))
            // Drag to replace (edit mode only): ghost follows the pointer, target highlights.
            .when(self.edit.is_some() || self.editing, |d| {
                d.on_drag(
                    payload,
                    |p: &DraggedTile, _: Point<Pixels>, _: &mut Window, cx: &mut App| {
                        cx.new(|_| p.clone())
                    },
                )
            })
            .drag_over::<DraggedTile>(|s, _, _, _| s.opacity(0.55))
            .on_drop(cx.listener(move |this, d: &DraggedTile, window, cx| {
                this.drop_handled = true;
                if this.auto_adjust_tiles {
                    this.move_tile(d.id, id, cx);
                } else {
                    let position = window.mouse_position();
                    this.pointer = (f32::from(position.x), f32::from(position.y));
                    this.place_tile_at_pointer(d.id, cx);
                }
            }))
            .child(turnstile(plate, w, h, id, turn, delay));

        // Windows Phone-style wiggle while the tiles are in edit mode. Tiles carry a
        // per-tile phase so they do not sway in lockstep.
        if self.editing {
            let phase = id as f32 * 0.7;
            tile.with_animation(
                ("tile-wiggle", id),
                Animation::new(Duration::from_millis(760))
                    .repeat()
                    .with_max_fps(30.0),
                move |el, d| {
                    let t = d * std::f32::consts::TAU + phase;
                    el.left(px(x + t.sin() * 1.6))
                },
            )
            .into_any_element()
        } else {
            tile.into_any_element()
        }
    }

    // ── pages ──

    pub(crate) fn start_page(
        &self,
        layout: &[(f32, f32)],
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
            // Long-press on empty space (not on a tile — tiles stop propagation)
            // enters tile edit mode.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    if !this.editing && this.launch.is_none() {
                        this.begin_empty_hold(f32::from(e.position.x), f32::from(e.position.y), cx);
                    }
                }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if this.suppress_page_click {
                    // This release ended the long press that entered edit mode.
                    this.suppress_page_click = false;
                    return;
                }
                if this.editing {
                    this.exit_editing(cx);
                } else if this.edit.is_some() {
                    this.edit = None;
                    cx.notify();
                }
            }))
            // Dropping on empty space sends the tile to the end of the list.
            // (If a tile already handled this drop, `drop_handled` skips it.)
            .on_drop(cx.listener(|this, d: &DraggedTile, window, cx| {
                if !this.drop_handled {
                    if this.auto_adjust_tiles {
                        this.move_to_end(d.id, cx);
                    } else {
                        let position = window.mouse_position();
                        this.pointer = (f32::from(position.x), f32::from(position.y));
                        this.place_tile_at_pointer(d.id, cx);
                    }
                }
                this.drop_handled = false;
            }))
            .child(
                div()
                    .pl(px(TILE_MARGIN))
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
}
