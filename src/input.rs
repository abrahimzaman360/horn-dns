//! Pointer gestures, long-press handling and quick toggles.

use crate::prelude::*;

impl Metro {
    /// Start the long-press timer for a tile. Arming lifts it into edit mode; a
    /// following drag moves it, and a release without movement opens the size menu.
    pub(crate) fn begin_tile_hold(&mut self, id: usize, x: f32, y: f32, cx: &mut Context<Self>) {
        self.hold_token = self.hold_token.wrapping_add(1);
        let token = self.hold_token;
        self.tile_hold = Some(TileHold {
            id,
            token,
            start: (x, y),
            armed: false,
            moved: false,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(TILE_HOLD_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(hold) = this.tile_hold else { return };
                if hold.token == token && !hold.moved {
                    this.tile_hold = Some(TileHold {
                        armed: true,
                        ..hold
                    });
                    this.edit = Some(hold.id);
                    this.editing = true;
                    this.tile_menu = Some(TileMenu { id: hold.id });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Long-press on empty Start area enters tile edit mode (wiggle + bottom bar).
    pub(crate) fn begin_empty_hold(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.empty_token = self.empty_token.wrapping_add(1);
        let token = self.empty_token;
        self.empty_hold = Some((token, x, y));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(PAGE_HOLD_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some((t, _, _)) = this.empty_hold else {
                    return;
                };
                if t == token {
                    this.empty_hold = None;
                    this.suppress_page_click = true;
                    this.editing = true;
                    this.edit = None;
                    this.tile_menu = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Leave tile editing mode and put the floating layers away.
    pub(crate) fn exit_editing(&mut self, cx: &mut Context<Self>) {
        if self.editing || self.edit.is_some() || self.tile_menu.is_some() || self.add_sheet {
            self.editing = false;
            self.edit = None;
            self.tile_menu = None;
            self.add_sheet = false;
            cx.notify();
        }
    }

    pub(crate) fn pointer_move(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        // A tile press-and-hold: small movements before it arms cancel it (that is
        // just a click), movements after it arms turn the hold into a drag.
        if let Some(hold) = self.tile_hold {
            let dx = f32::from(e.position.x) - hold.start.0;
            let dy = f32::from(e.position.y) - hold.start.1;
            if hold.armed {
                // GPUI starts a drag at ~2px; mirror that so the menu only opens
                // when the press really stayed put.
                if (dx.abs() > 2.0 || dy.abs() > 2.0)
                    && let Some(active) = self.tile_hold.as_mut()
                {
                    active.moved = true;
                }
            } else if dx.abs() > 10.0 || dy.abs() > 10.0 {
                self.tile_hold = None;
            }
        }
        // A long press on empty space is cancelled by a scroll-sized movement.
        if let Some((_, x, y)) = self.empty_hold {
            let dx = f32::from(e.position.x) - x;
            let dy = f32::from(e.position.y) - y;
            if dx.abs() > 10.0 || dy.abs() > 10.0 {
                self.empty_hold = None;
            }
        }
        self.pointer = (f32::from(e.position.x), f32::from(e.position.y));
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

    pub(crate) fn pointer_up(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.pointer = (x, y);
        self.home_press = None;
        self.empty_hold = None;
        // Finish a tile press: a drag ends the selection, a long-press without a
        // move opens the size menu where the finger let go.
        if let Some(hold) = self.tile_hold.take()
            && hold.armed
        {
            if hold.moved {
                // Dragged: GPUI already suppresses the click that would follow.
                self.edit = None;
            } else {
                // Long-pressed in place: this release opens the size menu, so the
                // click that follows must not launch the app.
                self.suppress_click = Some(hold.id);
                self.tile_menu = Some(TileMenu { id: hold.id });
            }
            cx.notify();
        }
        let was_slider = self.drag.take().is_some();
        if self.pulling {
            // Release: keep the panel if it was pulled far enough, else spring it back.
            self.pulling = false;
            self.pull_start = None;
            let ph = Self::panel_height_for(self.vh, self.panel.unwrap_or(PanelKind::System));
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
            // Brightness / gap / transparency were being scrubbed.
            self.save_soon(cx);
            cx.notify();
        }
    }

    // ── sliders ──

    /// Map a pointer x-position onto the slider being dragged. Settings sliders
    /// live in a centred, max-width column; the panel slider spans the panel.
    pub(crate) fn apply_drag(&mut self, kind: Drag, x: f32) {
        let (left, track) = match kind {
            Drag::Brightness => (
                PANEL_PAD,
                (self.measured_w - 2.0 * PANEL_PAD - 12.0 - 20.0).max(1.0),
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

    pub(crate) fn toggle_wifi(&mut self) {
        self.wifi = !self.wifi;
        if self.wifi {
            self.airplane = false;
        }
    }

    pub(crate) fn toggle_bluetooth(&mut self) {
        self.bluetooth = !self.bluetooth;
        if self.bluetooth {
            self.airplane = false;
        }
    }

    pub(crate) fn toggle_airplane(&mut self) {
        self.airplane = !self.airplane;
        if self.airplane {
            self.wifi = false;
            self.bluetooth = false;
        }
    }

    /// One quick-action tile. `slot` (0..=5) staggers its fly-in when the panel opens.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn quick_action(
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
            .when(active, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(7.0))
                        .right(px(7.0))
                        .size(px(18.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(ink)
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(self.accent))
                        .child("✓"),
                )
            })
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
}
