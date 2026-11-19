//! PIN lock screen overlay and its preparation pass.

use crate::prelude::*;

impl Metro {
    pub(crate) fn lock_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.boot_visible || !self.lock.locked {
            return None;
        }
        let fg = white();
        let background = match self
            .lock
            .wallpaper
            .as_ref()
            .or(self.wallpaper_file.as_ref())
        {
            Some(path) => div()
                .absolute()
                .size_full()
                .bg(hsla(0.0, 0.0, 0.03, 1.0))
                .child(img(path.clone()).object_fit(ObjectFit::Cover).size_full())
                .into_any_element(),
            None => div()
                .absolute()
                .size_full()
                .bg(hsla(0.0, 0.0, 0.03, 1.0))
                .into_any_element(),
        };

        let date = self.date.clone();
        let time = self.time.clone();
        let weather = div()
            .flex()
            .items_end()
            .gap(px(8.0))
            .child(glyph(IconName::Sun, 26.0, fg))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(30.0))
                            .line_height(px(32.0))
                            .font_weight(FontWeight::LIGHT)
                            .text_color(fg)
                            .child("24°"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(fg.opacity(0.72))
                            .child("Sunny"),
                    ),
            );

        let calendar = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(30.0))
                    .font_weight(FontWeight::LIGHT)
                    .text_color(fg)
                    .child("18"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("Design review"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(fg.opacity(0.72))
                            .child("4:30 PM"),
                    ),
            );

        let mut notification_widgets: Vec<AnyElement> = Vec::new();
        for (i, note) in NOTES.iter().take(5).enumerate() {
            let app = &APPS[note.app];
            let app_index = note.app;
            notification_widgets.push(
                div()
                    .id(("lock-widget", i as u32))
                    .relative()
                    .size(px(54.0))
                    .rounded(px(3.0))
                    .bg(rgb(app.color.unwrap_or(self.accent)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.lock.locked {
                            this.open_app(app_index, cx);
                        }
                    }))
                    .child(glyph(
                        app.icon,
                        24.0,
                        on_color(app.color.unwrap_or(self.accent)),
                    ))
                    .into_any_element(),
            );
        }

        let pin_fg = white();
        let mut pin_dots = Vec::with_capacity(self.lock.pin_len);
        for i in 0..self.lock.pin_len {
            pin_dots.push(
                div()
                    .w(px(22.0))
                    .h(px(22.0))
                    .border_1()
                    .border_color(pin_fg.opacity(0.82))
                    .rounded_full()
                    .bg(if i < self.lock.entry.len() {
                        pin_fg
                    } else {
                        hsla(0.0, 0.0, 1.0, 0.08)
                    }),
            );
        }

        let mut pin_keys: Vec<AnyElement> = Vec::with_capacity(12);
        for (slot, digit) in ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
            .into_iter()
            .enumerate()
        {
            pin_keys.push(
                div()
                    .id(("lock-pin-key", slot as u32))
                    .size(px(76.0))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(pin_fg.opacity(0.34))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.55).bg(pin_fg.opacity(0.16)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(ch) = digit.chars().next() {
                            this.lock.add_digit(ch);
                            if this.lock.entry.len() == this.lock.pin_len {
                                this.lock.verify();
                            }
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(28.0))
                            .font_weight(FontWeight::LIGHT)
                            .text_color(pin_fg)
                            .child(digit),
                    )
                    .into_any_element(),
            );
        }

        let pin_background = match self
            .lock
            .wallpaper
            .as_ref()
            .or(self.wallpaper_file.as_ref())
        {
            Some(path) => div()
                .absolute()
                .size_full()
                .bg(hsla(0.0, 0.0, 0.02, 1.0))
                .child(img(path.clone()).object_fit(ObjectFit::Cover).size_full())
                .into_any_element(),
            None => div()
                .absolute()
                .size_full()
                .bg(hsla(0.0, 0.0, 0.02, 1.0))
                .into_any_element(),
        };

        let pin_surface = div()
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .bg(hsla(0.0, 0.0, 0.02, 1.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .pt(px(72.0))
            .pb(px(72.0))
            .child(pin_background)
            .child(
                div()
                    .text_size(px(64.0))
                    .line_height(px(68.0))
                    .font_weight(FontWeight::LIGHT)
                    .text_color(pin_fg)
                    .child(time.clone()),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(15.0))
                    .text_color(pin_fg.opacity(0.78))
                    .child(date.clone()),
            )
            .child(
                div()
                    .mt(px(46.0))
                    .text_size(px(13.0))
                    .text_color(if self.lock.error {
                        hsla(0.0, 0.80, 0.72, 1.0)
                    } else {
                        pin_fg.opacity(0.82)
                    })
                    .child(if self.lock.error {
                        "Try again"
                    } else {
                        "Enter PIN"
                    }),
            )
            .child(div().mt(px(18.0)).flex().gap(px(14.0)).children(pin_dots))
            .child(
                div()
                    .mt(px(38.0))
                    .w(px(268.0))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(18.0))
                    .children(pin_keys)
                    .child(
                        div()
                            .id("lock-pin-back")
                            .size(px(76.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .active(|s| s.opacity(0.55).bg(pin_fg.opacity(0.16)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.lock.back_to_overlay();
                                cx.notify();
                            }))
                            .child(glyph(IconName::Lock, 22.0, pin_fg)),
                    )
                    .child(
                        div()
                            .id("lock-pin-zero")
                            .size(px(76.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(pin_fg.opacity(0.34))
                            .cursor_pointer()
                            .active(|s| s.opacity(0.55).bg(pin_fg.opacity(0.16)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.lock.add_digit('0');
                                if this.lock.entry.len() == this.lock.pin_len {
                                    this.lock.verify();
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(28.0))
                                    .font_weight(FontWeight::LIGHT)
                                    .text_color(pin_fg)
                                    .child("0"),
                            ),
                    )
                    .child(
                        div()
                            .id("lock-pin-delete")
                            .size(px(76.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .active(|s| s.opacity(0.55).bg(pin_fg.opacity(0.16)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.pin_delete_held = true;
                                    this.pin_delete_used = false;
                                    let token = this.hold_token.wrapping_add(1);
                                    this.hold_token = token;
                                    cx.spawn(async move |this, cx| {
                                        cx.background_executor()
                                            .timer(Duration::from_millis(TILE_HOLD_MS))
                                            .await;
                                        let _ = this.update(cx, |this, cx| {
                                            if !this.pin_delete_held || this.hold_token != token {
                                                return;
                                            }
                                            this.lock.clear_entry();
                                            this.pin_delete_used = true;
                                            cx.notify();
                                        });
                                    })
                                    .detach();
                                }),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.pin_delete_held = false;
                                    cx.notify();
                                }),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.pin_delete_used {
                                    this.pin_delete_used = false;
                                } else {
                                    this.lock.backspace();
                                }
                                cx.notify();
                            }))
                            .child(glyph(IconName::X, 22.0, pin_fg)),
                    ),
            );

        let drag_offset = self.lock.swipe_offset;
        let unlock_offset = if self.lock.unlocking { self.vh } else { 0.0 };
        let drag_progress = (self.lock.swipe_offset / (self.vh * 0.50).max(1.0)).clamp(0.0, 1.0);
        let overlay_content_opacity = 1.0 - drag_progress;
        let lock_screen = div()
            .id("lock-screen")
            .absolute()
            .left(px(0.0))
            .top(px(-drag_offset - unlock_offset))
            .size_full()
            .overflow_hidden()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    if !this.lock.pin_prompt {
                        this.lock.begin_swipe(f32::from(e.position.y));
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.lock.swipe_start.is_some() {
                    this.lock.move_swipe(f32::from(e.position.y));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, e: &MouseUpEvent, _, cx| {
                    if this.lock.swipe_start.is_some() {
                        this.lock.end_swipe(this.vh);
                        cx.notify();
                    }
                    let _ = e;
                }),
            )
            .child(background)
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .size_full()
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(hsla(0.0, 0.0, 0.0, 0.10), 0.0),
                        linear_color_stop(hsla(0.0, 0.0, 0.0, 0.55), 1.0),
                    )),
            )
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(MARGIN))
                    .opacity(overlay_content_opacity)
                    .child(
                        div()
                            .absolute()
                            .left(px(MARGIN))
                            .bottom(px(46.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(64.0))
                                    .line_height(px(68.0))
                                    .font_weight(FontWeight::LIGHT)
                                    .text_color(fg)
                                    .child(time),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::LIGHT)
                                    .text_color(fg.opacity(0.92))
                                    .child(date.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(30.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(26.0))
                                    .child(weather)
                                    .child(calendar),
                            )
                            .child(
                                div()
                                    .mt(px(32.0))
                                    .w(px(330.0))
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(22.0))
                                    .children(notification_widgets),
                            ),
                    )
                    .when(!self.lock.pin_prompt, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .bottom(px(12.0))
                                .w_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .w(px(34.0))
                                        .h(px(2.0))
                                        .rounded_full()
                                        .bg(fg.opacity(0.78)),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(fg.opacity(0.68))
                                        .child("Swipe up"),
                                ),
                        )
                    }),
            );

        let stack = div()
            .id("lock-stack")
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .overflow_hidden()
            .child(pin_surface);

        if self.lock.spring_back {
            let drag_offset = self.lock.swipe_offset;
            Some(
                stack
                    .child(lock_screen.with_animation(
                        ("lock-spring-back", self.lock.spring_back_n),
                        Animation::new(Duration::from_millis(420)).with_easing(ease_out_quint()),
                        move |el, progress| el.top(px(-drag_offset * (1.0 - progress))),
                    ))
                    .into_any_element(),
            )
        } else if self.lock.overlay_revealing {
            let vh = self.vh;
            let drag_offset = self.lock.swipe_offset;
            Some(
                stack
                    .child(lock_screen.with_animation(
                        ("lock-reveal", self.lock.overlay_return_n + 1),
                        Animation::new(Duration::from_millis(520)).with_easing(ease_out_quint()),
                        move |el, progress| {
                            let final_offset = vh;
                            let top = drag_offset + (final_offset - drag_offset) * progress;
                            el.top(px(-top))
                        },
                    ))
                    .into_any_element(),
            )
        } else if self.lock.overlay_returning {
            let vh = self.vh;
            Some(
                stack
                    .child(lock_screen.with_animation(
                        ("lock-return", self.lock.overlay_return_n),
                        Animation::new(Duration::from_millis(360)).with_easing(ease_out_quint()),
                        move |el, progress| el.top(px(-vh * (1.0 - progress))),
                    ))
                    .into_any_element(),
            )
        } else if self.lock.pin_prompt {
            Some(stack.into_any_element())
        } else if unlock_offset > 0.0 {
            Some(
                stack
                    .child(lock_screen.with_animation(
                        ("lock-unlock", self.lock.unlock_n),
                        Animation::new(Duration::from_millis(360)).with_easing(ease_out_quint()),
                        move |el, progress| el.top(px(-unlock_offset * progress)),
                    ))
                    .into_any_element(),
            )
        } else {
            Some(stack.child(lock_screen).into_any_element())
        }
    }

    pub(crate) fn lock_prepare(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.lock.spring_back && !self.lock.spring_back_scheduled {
            self.lock.spring_back_scheduled = true;
            let token = self.lock.spring_back_n;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(420))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.lock.spring_back && this.lock.spring_back_n == token {
                        this.lock.finish_spring_back();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if self.lock.overlay_revealing && !self.lock.overlay_reveal_scheduled {
            self.lock.overlay_reveal_scheduled = true;
            let token = self.lock.overlay_return_n.wrapping_add(1);
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(520))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.lock.overlay_revealing
                        && this.lock.overlay_return_n.wrapping_add(1) == token
                    {
                        this.lock.overlay_revealing = false;
                        this.lock.overlay_reveal_scheduled = false;
                        this.lock.swipe_offset = 0.0;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if self.lock.overlay_returning && !self.lock.overlay_return_scheduled {
            self.lock.overlay_return_scheduled = true;
            let token = self.lock.overlay_return_n;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(360))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.lock.overlay_returning && this.lock.overlay_return_n == token {
                        this.lock.finish_overlay_return();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if self.lock.unlocking && !self.lock.unlock_scheduled {
            self.lock.unlock_scheduled = true;
            let token = self.lock.unlock_n;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(360))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.lock.unlocking && this.lock.unlock_n == token {
                        this.lock.complete_unlock();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}
