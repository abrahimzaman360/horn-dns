//! Root view composition and shutdown persistence.

use crate::prelude::*;

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

        if self.boot_visible {
            return div()
                .id("lumen-boot-root")
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0.0, 0.0, 0.02, 1.0))
                .occlude()
                .child(
                    div()
                        .text_size(px(44.0))
                        .font_weight(FontWeight::LIGHT)
                        .text_color(white())
                        .child("LUMEN OS.Y")
                        .with_animation(
                            ("lumen-boot-logo", 1u32),
                            Animation::new(Duration::from_millis(7000))
                                .with_easing(ease_out_quint()),
                            |el, d| {
                                let highlight = d.clamp(0.0, 1.0);
                                el.opacity(1.0).text_color(hsla(
                                    0.0,
                                    0.0,
                                    0.78 + highlight * 0.22,
                                    1.0,
                                ))
                            },
                        ),
                )
                .into_any_element();
        }

        if self.lock.locked {
            let lock_layer = self.lock_layer(cx);
            return div()
                .id("lumen-lock-root")
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .when_some(lock_layer, |d, layer| d.child(layer))
                .into_any_element();
        }

        self.maybe_focus_search(window, cx);
        self.lock_prepare(window, cx);

        let page = self.page_element(self.page, true, window, cx);
        let page_backdrop = if self.page == Page::Start {
            hsla(0.0, 0.0, 0.0, 0.0)
        } else {
            self.page_bg()
        };
        let animated = div().size_full().bg(page_backdrop).child(page);
        let flip = self.page_flip(width, window, cx);

        let dim = (1.0 - self.brightness) * 0.6;
        let status_bar = (!self.lock.locked).then(|| self.status_bar(cx));
        let switcher_layer = self.switcher_layer(window, cx);
        let launch_layer = self.launch_layer(width);
        let panel_layer = self.panel_layer(window, cx);
        let edit_bar = self.edit_bar(cx);
        let add_sheet = self.add_sheet_layer(cx);
        let tile_menu = self.tile_menu_layer(cx);
        let app_sort_menu = self.app_sort_menu_layer(cx);
        let lock_layer = self.lock_layer(cx);
        let shutdown_confirm = self.shutdown_confirm.then(|| {
            let fg = self.fg();
            div()
                .id("shutdown-dialog")
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0.0, 0.0, 0.0, 0.58))
                .occlude()
                .child(
                    div()
                        .w(px(300.0))
                        .p(px(20.0))
                        .rounded(px(6.0))
                        .bg(self.panel_bg())
                        .border_1()
                        .border_color(fg.opacity(0.18))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap(px(18.0))
                        .child(
                            div()
                                .text_size(px(20.0))
                                .font_weight(FontWeight::LIGHT)
                                .text_color(fg)
                                .child("Shut down Lumen OS.Y?"),
                        )
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .id("shutdown-cancel")
                                        .flex_1()
                                        .h(px(40.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(3.0))
                                        .bg(self.cell_bg())
                                        .cursor_pointer()
                                        .active(|s| s.opacity(0.6))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.shutdown_confirm = false;
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(fg)
                                                .child("Cancel"),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("shutdown-confirm")
                                        .flex_1()
                                        .h(px(40.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(3.0))
                                        .bg(rgb(0xE51400))
                                        .cursor_pointer()
                                        .active(|s| s.opacity(0.6))
                                        .on_click(cx.listener(|_, _, _, cx| {
                                            cx.shutdown();
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(white())
                                                .child("Shut down"),
                                        ),
                                ),
                        ),
                )
        });

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
                    cx.listener(|this, e: &MouseUpEvent, _, cx| {
                        this.pointer_up(f32::from(e.position.x), f32::from(e.position.y), cx)
                    }),
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
                cx.listener(|this, e: &MouseUpEvent, _, cx| {
                    this.pointer_up(f32::from(e.position.x), f32::from(e.position.y), cx)
                }),
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
            // PIN lock screen sits above app content and every transient panel.
            .when_some(lock_layer, |d, layer| d.child(layer))
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
            // Tile edit affordances, size menu, then the gesture catcher.
            .when_some(edit_bar, |d, bar| d.child(bar))
            .when_some(add_sheet, |d, sheet| d.child(sheet))
            .when_some(tile_menu, |d, menu| d.child(menu))
            .when_some(app_sort_menu, |d, menu| d.child(menu))
            .when_some(catcher, |d, c| d.child(c))
            .when_some(status_bar, |d, bar| d.child(bar))
            .when_some(shutdown_confirm, |d, dialog| d.child(dialog))
            .when(self.boot_visible, |d| {
                d.child(
                    div()
                        .id("lumen-boot")
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(hsla(0.0, 0.0, 0.02, 1.0))
                        .occlude()
                        .child(
                            div()
                                .text_size(px(44.0))
                                .font_weight(FontWeight::LIGHT)
                                .text_color(white())
                                .child("LUMEN OS.Y")
                                .with_animation(
                                    ("lumen-boot", 1u32),
                                    Animation::new(Duration::from_millis(7000))
                                        .with_easing(ease_out_quint()),
                                    |el, d| {
                                        let highlight = d.clamp(0.0, 1.0);
                                        el.opacity(1.0).text_color(hsla(
                                            0.0,
                                            0.0,
                                            0.78 + highlight * 0.22,
                                            1.0,
                                        ))
                                    },
                                ),
                        ),
                )
            })
            .into_any_element()
    }
}

/// Final write on shutdown, so a change made inside the debounce window is kept.
impl Drop for Metro {
    fn drop(&mut self) {
        save_config(&self.config());
    }
}
