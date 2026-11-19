//! All Apps list, app page and book-page transitions.

use crate::prelude::*;

impl Metro {
    /// One row of the All Apps list: rectangular icon plate, name, pinned hint.
    pub(crate) fn app_row(&self, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = self.fg();
        let a = &APPS[i];
        let app_hex = a.color.unwrap_or(self.accent);
        let pinned = self.tiles.iter().any(|t| t.app == i);
        div()
            .id(("app-row", i))
            .flex()
            .items_center()
            .gap(px(14.0))
            .py(px(8.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| this.open_app(i, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| this.toggle_pin(i, cx)),
            )
            .child(
                // Rectangular (not circular) icon border in the list view.
                div()
                    .w(px(46.0))
                    .h(px(46.0))
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(fg.opacity(0.25))
                    .bg(rgb(app_hex))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(a.icon, 22.0, on_color(app_hex))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::LIGHT)
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(a.name),
                    )
                    .when(pinned, |d| {
                        d.child(
                            div()
                                .mt(px(1.0))
                                .text_size(px(11.0))
                                .text_color(fg.opacity(0.5))
                                .child("Pinned to Start"),
                        )
                    }),
            )
    }

    pub(crate) fn apps_page(&self, key: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = key;
        let fg = self.fg();
        let query = self.search.trim().to_lowercase();

        let mut order: Vec<usize> = (0..APPS.len())
            .filter(|&i| {
                query.is_empty()
                    || APPS[i].name.to_lowercase().contains(&query)
                    || APPS[i].category.name().to_lowercase().contains(&query)
            })
            .collect();
        if self.app_sort == AppSort::Alphabetical {
            order.sort_by_key(|&i| APPS[i].name.to_lowercase());
        } else {
            order.sort_by_key(|&i| (APPS[i].category.rank(), APPS[i].name.to_lowercase()));
        }

        let mut list: Vec<AnyElement> = Vec::new();
        let mut current_category: Option<Category> = None;
        let mut current_letter: Option<char> = None;
        for &i in &order {
            match self.app_sort {
                AppSort::Category => {
                    let category = APPS[i].category;
                    if current_category != Some(category) {
                        current_category = Some(category);
                        list.push(
                            div()
                                .mt(px(16.0))
                                .mb(px(2.0))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg.opacity(0.55))
                                .child(category.name())
                                .into_any_element(),
                        );
                    }
                }
                AppSort::Alphabetical => {
                    let letter = APPS[i]
                        .name
                        .chars()
                        .next()
                        .unwrap_or('?')
                        .to_ascii_uppercase();
                    if current_letter != Some(letter) {
                        current_letter = Some(letter);
                        list.push(
                            div()
                                .mt(px(16.0))
                                .mb(px(2.0))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg.opacity(0.55))
                                .child(letter.to_string())
                                .into_any_element(),
                        );
                    }
                }
            }
            list.push(self.app_row(i, cx).into_any_element());
        }
        if list.is_empty() {
            list.push(
                div()
                    .mt(px(44.0))
                    .w_full()
                    .text_center()
                    .text_size(px(13.0))
                    .text_color(fg.opacity(0.55))
                    .child("No apps match your search")
                    .into_any_element(),
            );
        }

        let searching = !self.search.is_empty();
        let search = div()
            .id("apps-search")
            .track_focus(&self.search_focus)
            .mt(px(6.0))
            .w_full()
            .h(px(44.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(self.cell_bg())
            .rounded(px(2.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| {
                this.search_focus.focus(window, cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = e.keystroke.modifiers;
                let command = m.control || m.alt || m.platform || m.function;
                match e.keystroke.key.as_str() {
                    "backspace" => {
                        this.search.pop();
                    }
                    "escape" => this.search.clear(),
                    _ => {
                        if !command && let Some(text) = e.keystroke.key_char.as_deref() {
                            this.search.push_str(text);
                        }
                    }
                }
                cx.notify();
            }))
            .child(
                div()
                    .id("apps-search-icon")
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.search_focus.focus(window, cx);
                            cx.notify();
                        }),
                    )
                    .child(glyph(IconName::Search, 17.0, fg.opacity(0.6))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(15.0))
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(if searching { fg } else { fg.opacity(0.45) })
                    .child(if searching {
                        self.search.clone()
                    } else {
                        "Search apps".to_string()
                    }),
            )
            .when(searching, |d| {
                d.child(
                    div()
                        .id("apps-search-clear")
                        .size(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(fg.opacity(0.15))
                        .cursor_pointer()
                        .active(|s| s.opacity(0.6))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search.clear();
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .child(glyph(IconName::X, 12.0, fg)),
                )
            });

        let app_header = div()
            .relative()
            .w_full()
            .h(px(72.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(54.0))
                    .line_height(px(60.0))
                    .font_weight(FontWeight::LIGHT)
                    .text_color(fg)
                    .child("Apps"),
            )
            .child(
                div()
                    .id("apps-sort-menu")
                    .size(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.app_sort_menu = !this.app_sort_menu;
                        cx.notify();
                    }))
                    .child(glyph(IconName::Ellipsis, 22.0, fg)),
            );

        div()
            .id("apps-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(MARGIN))
            .child(
                div()
                    .w_full()
                    .max_w(px(560.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .pb(px(28.0))
                    .child(app_header)
                    .child(search)
                    .children(list),
            )
    }

    /// Placeholder app page. With `anim` set, the header slides in, the icon pops in and the
    /// text trails after it — the "app content arrives as the splash fades" feel.
    pub(crate) fn app_page(&self, i: usize, anim: Option<usize>) -> impl IntoElement {
        let a = &APPS[i];
        let app_hex = a.color.unwrap_or(self.accent);

        let icon = div()
            .size(px(120.0))
            .rounded_full()
            .bg(rgb(app_hex))
            .flex()
            .items_center()
            .justify_center()
            .child(glyph(a.icon, 60.0, on_color(app_hex)));
        let icon: AnyElement = match anim {
            Some(k) => icon
                .with_animation(
                    ("app-icon", k),
                    Animation::new(Duration::from_millis(520)).with_easing(ease_out_quint()),
                    |el, d| el.size(px(120.0 * (0.5 + 0.5 * d))).opacity(d),
                )
                .into_any_element(),
            None => icon.into_any_element(),
        };

        let text = div()
            .w_full()
            .text_size(px(18.0))
            .font_weight(FontWeight::LIGHT)
            .child(format!(
                "This is where the {} app would live. Press ← to go back to Start.",
                a.name
            ));
        let text: AnyElement = match anim {
            Some(k) => text
                .with_animation(
                    ("app-text", k),
                    Animation::new(Duration::from_millis(640)).with_easing(ease_out_quint()),
                    |el, d| el.relative().left(px(70.0 * (1.0 - d))).opacity(d),
                )
                .into_any_element(),
            None => text.into_any_element(),
        };

        div()
            .size_full()
            .px(px(MARGIN))
            .child(pivot_header("App".into(), a.name.into(), anim))
            .child(
                div()
                    .mt(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    // Fixed 120×120 slot so the text below doesn't jump while the icon grows.
                    .child(
                        div()
                            .size(px(120.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon),
                    )
                    .child(text),
            )
    }

    /// `entering` = this is the page being shown (as opposed to the outgoing page that is
    /// still painted while it peels away): only then do the page-enter animations run.
    pub(crate) fn page_element(
        &self,
        page: Page,
        entering: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = entering.then_some(self.nav);
        match page {
            Page::Start => {
                let (layout, rows) = self.tile_layout();
                self.start_page(&layout, rows, entering, window, cx)
                    .into_any_element()
            }
            Page::Apps => self.apps_page(key, cx).into_any_element(),
            Page::Settings => self.settings_page(key, cx).into_any_element(),
            Page::App(i) => self.app_page(i, key).into_any_element(),
        }
    }

    /// The Windows Phone style page change: the outgoing page stays laid out at
    /// full size inside a clip that shrinks from the right, so it reads as a page
    /// being turned away to reveal the new one beneath. A soft crease shadow
    /// trails the moving edge.
    pub(crate) fn page_flip(
        &self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.page_morph {
            return None;
        }
        let prev = self.prev_page?;
        let old_page = self.page_element(prev, false, window, cx);
        let backdrop = if prev == Page::Start {
            hsla(0.0, 0.0, 0.0, 0.0)
        } else {
            self.page_bg()
        };
        let crease = hsla(0.0, 0.0, 0.0, if self.dark { 0.55 } else { 0.28 });
        let w = width.max(1.0);

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .h_full()
                .overflow_hidden()
                .bg(backdrop)
                // The outgoing page is decorative while it peels away.
                .block_mouse_except_scroll()
                // The old page keeps its full width; only the clip narrows.
                .child(div().w(px(w)).h_full().child(old_page))
                .child(
                    div()
                        .absolute()
                        .right(px(0.0))
                        .top(px(0.0))
                        .w(px(24.0))
                        .h_full()
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(hsla(0.0, 0.0, 0.0, 0.0), 0.0),
                            linear_color_stop(crease, 1.0),
                        )),
                )
                .with_animation(
                    ("page-flip", self.nav),
                    Animation::new(Duration::from_millis(420)).with_easing(ease_out_quint()),
                    move |el, d| {
                        let turn = (1.0 - d).powf(1.35);
                        el.w(px(w * turn))
                            .opacity((0.35 + 0.65 * (1.0 - d)).min(1.0))
                    },
                )
                .into_any_element(),
        )
    }

    /// Focus the All Apps search field the first frame the page is shown.
    pub(crate) fn maybe_focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Apps {
            if self.search_pending {
                self.search_pending = false;
                self.search_focus.focus(window, cx);
            }
        } else {
            self.search_pending = false;
        }
    }
}
