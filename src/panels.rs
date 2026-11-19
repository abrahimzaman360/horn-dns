//! System / notification panels pulled down from the status bar.

use crate::prelude::*;

impl Metro {
    pub(crate) fn panel_height_for(viewport_h: f32, kind: PanelKind) -> f32 {
        match kind {
            PanelKind::System => 332.0_f32.min(viewport_h - STATUS_H).max(300.0),
            PanelKind::Notifications => (viewport_h * 0.66).clamp(320.0, 560.0),
        }
    }

    pub(crate) fn open_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
        self.panel = Some(kind);
        self.panel_closing = false;
        self.panel_token += 1;
        self.panel_open_n += 1;
        self.edit = None;
        self.editing = false;
        self.tile_menu = None;
        self.add_sheet = false;
        self.pull_start = None;
        cx.notify();
    }

    /// Dismiss with the slide-up animation; the panel stays mounted until it has
    /// travelled out of view, so the exit is animated instead of popping away.
    pub(crate) fn close_panel(&mut self, cx: &mut Context<Self>) {
        if self.panel.is_none() || self.panel_closing {
            return;
        }
        self.panel_closing = true;
        self.pull_start = None;
        self.pulling = false;
        self.drag = None;
        self.panel_token += 1;
        let token = self.panel_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(360))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.panel_token == token {
                    this.panel = None;
                    this.panel_closing = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Left half of the status bar = notifications, right half = system panel.
    pub(crate) fn panel_for_x(&self, x: f32) -> PanelKind {
        if x < self.measured_w * 0.5 {
            PanelKind::Notifications
        } else {
            PanelKind::System
        }
    }

    pub(crate) fn toggle_panel(&mut self, x: f32, cx: &mut Context<Self>) {
        let kind = self.panel_for_x(x);
        if self.panel == Some(kind) {
            self.close_panel(cx);
        } else {
            self.open_panel(kind, cx);
        }
    }

    // ── pointer tracking (root + gesture catcher) ──

    /// One labelled slider row: min label, track, value, max label.
    pub(crate) fn slider(&self, spec: Slider, cx: &mut Context<Self>) -> Div {
        let Slider {
            id,
            kind,
            min,
            max,
            value,
            ratio,
        } = spec;
        let fg = self.fg();
        let accent: Hsla = rgb(self.accent).into();
        let ratio = ratio.clamp(0.0, 1.0);
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .w(px(24.0))
                    .text_size(px(10.0))
                    .text_color(fg.opacity(0.55))
                    .child(min),
            )
            .child(
                div()
                    .id(id)
                    .relative()
                    .flex_1()
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            this.drag = Some(kind);
                            this.apply_drag(kind, f32::from(e.position.x));
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(4.0))
                            .bg(self.cell_bg())
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(0.0))
                                    .h_full()
                                    .w(relative(ratio))
                                    .bg(accent)
                                    .child(
                                        div()
                                            .absolute()
                                            .right(px(-8.0))
                                            .top(px(-6.0))
                                            .size(px(16.0))
                                            .rounded_full()
                                            .bg(accent),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .w(px(44.0))
                    .text_size(px(11.0))
                    .text_color(fg.opacity(0.75))
                    .child(value),
            )
            .child(
                div()
                    .w(px(28.0))
                    .text_size(px(10.0))
                    .text_color(fg.opacity(0.5))
                    .child(max),
            )
    }

    pub(crate) fn panel_heading(
        &self,
        overline: &'static str,
        title: &'static str,
        with_settings: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let fg = self.fg();
        let settings = div()
            .id("panel-heading-settings")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_panel(cx);
                this.go(Page::Settings, cx);
            }))
            .child(glyph(IconName::Settings, 16.0, fg));
        let close = div()
            .id("panel-close")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| this.close_panel(cx)))
            .child(glyph(IconName::ChevronUp, 20.0, fg));

        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg.opacity(0.55))
                            .child(overline),
                    )
                    .child(
                        div()
                            .text_size(px(28.0))
                            .line_height(px(32.0))
                            .font_weight(FontWeight::LIGHT)
                            .child(title),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .when(with_settings, |d| d.child(settings))
                    .child(close),
            )
    }

    /// System panel: quick toggles, brightness, accent + theme shortcuts.
    pub(crate) fn system_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let fg = self.fg();
        let caption = |text: &'static str| {
            div()
                .mt(px(2.0))
                .mb(px(-4.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg.opacity(0.55))
                .child(text)
        };

        let row1 = div()
            .flex()
            .gap(px(8.0))
            .child(self.quick_action(
                "qa-wifi",
                "Wi-Fi",
                IconName::Wifi,
                self.wifi,
                Self::toggle_wifi,
                0,
                cx,
            ))
            .child(self.quick_action(
                "qa-bt",
                "Bluetooth",
                IconName::Bluetooth,
                self.bluetooth,
                Self::toggle_bluetooth,
                1,
                cx,
            ))
            .child(self.quick_action(
                "qa-plane",
                "Airplane mode",
                IconName::Plane,
                self.airplane,
                Self::toggle_airplane,
                2,
                cx,
            ));

        let row2 = div()
            .flex()
            .gap(px(8.0))
            .child(self.quick_action(
                "qa-quiet",
                "Quiet hours",
                IconName::Bell,
                self.quiet,
                |this| this.quiet = !this.quiet,
                3,
                cx,
            ))
            .child(self.quick_action(
                "qa-night",
                "Night light",
                IconName::Moon,
                self.night_light,
                |this| this.night_light = !this.night_light,
                4,
                cx,
            ))
            .child(self.quick_action(
                "qa-battery",
                "Battery saver",
                IconName::Battery,
                self.battery_saver,
                |this| this.battery_saver = !this.battery_saver,
                5,
                cx,
            ));

        let brightness_ratio = self.brightness.clamp(0.0, 1.0);
        let brightness = div()
            .w_full()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(14.0))
            .child(
                div()
                    .id("brightness-track")
                    .relative()
                    .flex_1()
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, _, cx| {
                            this.drag = Some(Drag::Brightness);
                            this.apply_drag(Drag::Brightness, f32::from(e.position.x));
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(4.0))
                            .bg(self.cell_bg())
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(0.0))
                                    .h(px(4.0))
                                    .bg(rgb(self.accent))
                                    .w(relative(brightness_ratio))
                                    .child(
                                        div()
                                            .absolute()
                                            .right(px(-8.0))
                                            .top(px(-4.0))
                                            .size(px(12.0))
                                            .rounded_full()
                                            .bg(rgb(self.accent)),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .id("brightness-auto")
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.brightness = 1.0;
                        this.save_soon(cx);
                        cx.notify();
                    }))
                    .child(glyph(IconName::Sun, 18.0, fg.opacity(0.75))),
            );

        let profile = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .size(px(40.0))
                    .rounded_full()
                    .bg(rgb(self.accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(IconName::Users, 20.0, on_color(self.accent))),
            )
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Ibrahim Zaman"),
            );

        let lock = div()
            .id("system-profile-lock")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| this.lock_now(cx)))
            .child(glyph(IconName::Lock, 16.0, fg));

        let shutdown = div()
            .id("system-profile-shutdown")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| {
                this.shutdown_confirm = true;
                this.close_panel(cx);
                cx.notify();
            }))
            .child(glyph(IconName::Power, 16.0, fg));

        let settings = div()
            .id("system-profile-settings")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_panel(cx);
                this.go(Page::Settings, cx);
            }))
            .child(glyph(IconName::Settings, 16.0, fg));

        let dismiss = div()
            .id("system-profile-dismiss")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(self.cell_bg())
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| this.close_panel(cx)))
            .child(glyph(IconName::ChevronUp, 20.0, fg));

        let header = div()
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .child(profile)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(lock)
                    .child(shutdown)
                    .child(settings)
                    .child(dismiss),
            );

        div()
            .id("panel-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(PANEL_PAD))
            .pt(px(STATUS_H + 10.0))
            .pb(px(PANEL_PAD))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(header)
            .child(caption("Quick Actions"))
            .child(row1)
            .child(row2)
            .child(caption("Brightness"))
            .child(brightness)
    }

    /// Notification panel: swipe down from the left half of the status bar.
    pub(crate) fn notifications_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let fg = self.fg();

        let clear = div()
            .id("notes-clear")
            .px(px(12.0))
            .h(px(30.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .bg(self.cell_bg())
            .rounded(px(2.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .child(glyph(IconName::X, 13.0, fg.opacity(0.75)))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(fg.opacity(0.8))
                    .child("Clear all"),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.notes_cleared = true;
                cx.notify();
            }));

        let body: Div = if self.notes_cleared {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .child(glyph(IconName::BellOff, 30.0, fg.opacity(0.35)))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(fg.opacity(0.55))
                        .child("You're all caught up"),
                )
        } else {
            let mut rows: Vec<AnyElement> = Vec::with_capacity(NOTES.len());
            for (i, n) in NOTES.iter().enumerate() {
                rows.push(self.note_row(n, i, cx));
            }
            div().flex().flex_col().children(rows)
        };

        div()
            .id("notes-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(PANEL_PAD))
            .pt(px(STATUS_H + 10.0))
            .pb(px(PANEL_PAD))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.panel_heading("Notifications", "Notifications", false, cx))
            .child(
                div().flex().items_center().justify_start().child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(fg.opacity(0.55))
                        .child(if self.notes_cleared {
                            "No new notifications"
                        } else {
                            "New in the last 24 hours"
                        }),
                ),
            )
            .child(body)
            .child(div().flex_1())
            .child(div().w_full().flex().justify_end().child(clear))
    }

    /// One notification; rows slide in from the left one after another.
    pub(crate) fn note_row(&self, n: &Note, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let fg = self.fg();
        let a = &APPS[n.app];
        let hex = a.color.unwrap_or(self.accent);
        let app = n.app;
        let open_n = self.panel_open_n;
        let delay = 55.0 * i as f32;
        let total = 320.0 + 55.0 * NOTES.len() as f32;
        div()
            .id(("note", i))
            .relative()
            .flex()
            .items_start()
            .gap(px(10.0))
            .py(px(8.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| this.open_app(app, cx)))
            .child(
                div()
                    .size(px(34.0))
                    .rounded(px(3.0))
                    .bg(rgb(hex))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(a.icon, 17.0, on_color(hex))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(n.title),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(fg.opacity(0.5))
                                    .child(n.when),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(15.0))
                            .text_color(fg.opacity(0.72))
                            .child(n.body),
                    ),
            )
            .with_animation(
                ("note-in", open_n * 100 + i),
                Animation::new(Duration::from_millis(total as u64)),
                move |el, d| {
                    let p = ((d * total - delay) / 320.0).clamp(0.0, 1.0);
                    let e = ease_out(p);
                    el.left(px(-56.0 * (1.0 - e))).opacity(e)
                },
            )
            .into_any_element()
    }

    /// The sliding surface itself. It is mounted while a panel is open (or still
    /// animating out) and driven by a reversible `transition`. While the user is
    /// pulling it, the transition collapses to ~1 ms so the panel tracks the pointer;
    /// on release it eases the rest of the way open (or back up).
    pub(crate) fn panel_layer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let viewport_h = f32::from(window.viewport_size().height);
        let panel_h = Self::panel_height_for(viewport_h, self.panel.unwrap_or(PanelKind::System));
        let open = self.panel.is_some() && !self.panel_closing;

        let (slide_target, slide_ms) = if self.pulling {
            (self.pull_y.clamp(0.0, panel_h), 1)
        } else if open {
            (panel_h, 320)
        } else {
            (0.0, 320)
        };
        let slide = transition(
            ("panel", "slide"),
            slide_target,
            Transition::new(Duration::from_millis(slide_ms)).easing(Easing::EaseInOut),
            window,
            cx,
        );
        let scrim = transition(
            ("panel", "scrim"),
            (slide_target / panel_h).clamp(0.0, 1.0),
            Transition::new(Duration::from_millis(if self.pulling { 1 } else { 260 }))
                .easing(Easing::EaseInOut),
            window,
            cx,
        );

        if slide < 0.5 {
            return None;
        }
        let slide = slide.clamp(0.0, panel_h);
        let top = slide - panel_h;

        let scrim = div()
            .id("panel-scrim")
            .absolute()
            .left(px(0.0))
            .top(px(slide))
            .w_full()
            .h(px((viewport_h - slide).max(0.0)))
            .bg(hsla(
                0.0,
                0.0,
                0.0,
                scrim * if self.dark { 0.55 } else { 0.3 },
            ))
            .occlude()
            .when(open && !self.pulling, |d| {
                d.on_click(cx.listener(|this, _, _, cx| this.close_panel(cx)))
            });

        let content = match self.panel {
            Some(PanelKind::Notifications) => self.notifications_panel(cx),
            _ => self.system_panel(cx),
        };

        let panel = div()
            .id("panel")
            .absolute()
            .left(px(0.0))
            .top(px(top))
            .w_full()
            .h(px(panel_h))
            .bg(if self.dark {
                hsla(0.0, 0.0, 0.08, 0.78)
            } else {
                hsla(0.0, 0.0, 1.0, 0.82)
            })
            .border_b_1()
            .border_color(self.fg().opacity(0.12))
            .shadow_lg()
            .occlude()
            .child(content);

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .child(scrim)
                .child(panel)
                .into_any_element(),
        )
    }

    // ── tile size menu (long-press a tile, release without moving) ──
}
