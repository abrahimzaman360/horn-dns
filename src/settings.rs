//! Settings page and the lock-screen settings section.

use crate::prelude::*;

impl Metro {
    pub(crate) fn lock_settings_section(&self, cx: &mut Context<Self>) -> Div {
        let fg = self.fg();
        let enabled = self.lock.pin_hash.is_some();
        let toggle = div()
            .id("lock-enable")
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| {
                this.lock.set_enabled(!this.lock.pin_hash.is_some());
                this.save_soon(cx);
                cx.notify();
            }))
            .child(div().text_size(px(13.0)).child("Screen Lock"))
            .child(
                div()
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(if enabled {
                        rgb(self.accent).into()
                    } else {
                        self.cell_bg()
                    })
                    .flex()
                    .items_center()
                    .when(enabled, |d| d.justify_end())
                    .when(!enabled, |d| d.justify_start())
                    .child(div().mx(px(3.0)).size(px(16.0)).rounded_full().bg(fg)),
            );

        let mut lengths = Vec::new();
        for len in [4usize, 5, 6] {
            let selected = self.lock.pin_len == len;
            lengths.push(
                div()
                    .id(("lock-length", len))
                    .flex_1()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(2.0))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .bg(if selected {
                        rgb(self.accent).into()
                    } else {
                        self.cell_bg()
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.lock.pin_len = len;
                        this.lock.pin_hash = Some(hash_pin(default_pin_for_len(len)));
                        this.lock.entry.clear();
                        this.save_soon(cx);
                        cx.notify();
                    }))
                    .child(div().text_size(px(12.0)).child(format!("{len}-digit PIN"))),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(toggle)
            .child(div().mt(px(8.0)).flex().gap(px(6.0)).children(lengths))
    }

    pub(crate) fn settings_page(
        &self,
        key: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fg = self.fg();

        let themes: Vec<_> = [("Dark", true), ("Light", false)]
            .into_iter()
            .map(|(name, dark)| {
                let selected = self.dark == dark;
                div()
                    .id(name)
                    .text_size(px(34.0))
                    .font_weight(FontWeight::LIGHT)
                    .cursor_pointer()
                    .text_color(if selected { fg } else { fg.opacity(0.45) })
                    .when(selected, |d| d.border_b_3().border_color(fg))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dark = dark;
                        this.save_soon(cx);
                        cx.notify();
                    }))
            })
            .collect();

        let wallpapers: Vec<_> = (0..WALLPAPER_NAMES.len())
            .map(|i| {
                let (from, to) = self.wallpaper_pair(i);
                let selected = self.wallpaper == i;
                div()
                    .id(("wallpaper", i))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.wallpaper = i;
                        this.save_soon(cx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(92.0))
                            .h(px(58.0))
                            .bg(linear_gradient(
                                135.0,
                                linear_color_stop(rgb(from), 0.0),
                                linear_color_stop(rgb(to), 1.0),
                            ))
                            .when(selected, |d| d.border_2().border_color(fg)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if selected { fg } else { fg.opacity(0.55) })
                            .child(WALLPAPER_NAMES[i]),
                    )
            })
            .collect();

        let wallpaper_images: Vec<_> = self
            .wallpaper_files
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let selected = self.wallpaper_file.as_deref() == Some(path.as_path());
                let pick = path.clone();
                let label = wallpaper_label(path);
                div()
                    .id(("wallpaper-file", i))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.wallpaper_file = Some(pick.clone());
                        this.save_soon(cx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(92.0))
                            .h(px(58.0))
                            .overflow_hidden()
                            .bg(self.cell_bg())
                            .when(selected, |d| d.border_2().border_color(fg))
                            .child(img(path.clone()).object_fit(ObjectFit::Cover).size_full()),
                    )
                    .child(
                        div()
                            .w(px(92.0))
                            .text_size(px(12.0))
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(if selected { fg } else { fg.opacity(0.55) })
                            .child(label),
                    )
            })
            .collect();

        let swatches: Vec<_> = ACCENTS
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                div()
                    .id(("swatch", i))
                    .size(px(54.0))
                    .bg(rgb(c))
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .when(self.accent == c, |d| d.border_2().border_color(fg))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.accent = c;
                        this.save_soon(cx);
                        cx.notify();
                    }))
            })
            .collect();

        // `name` is a `&'static str` label; the element id needs a stable key.
        let caption = |s: &'static str| {
            div()
                .mt(px(16.0))
                .mb(px(6.0))
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg.opacity(0.6))
                .child(s)
        };

        let gap = self.slider(
            Slider {
                id: "gap-track",
                kind: Drag::Gap,
                min: "0",
                max: "28",
                value: format!("{} px", self.gap.round() as u32),
                ratio: (self.gap - GAP_MIN) / (GAP_MAX - GAP_MIN),
            },
            cx,
        );

        let alpha = self.slider(
            Slider {
                id: "alpha-track",
                kind: Drag::Opacity,
                min: "0",
                max: "100",
                value: format!("{}%", (self.tile_alpha * 100.0).round() as u32),
                ratio: (self.tile_alpha - TILE_ALPHA_MIN) / (1.0 - TILE_ALPHA_MIN),
            },
            cx,
        );

        let liquid_glass = self.liquid_glass;
        let liquid_glass_toggle = div()
            .id("setting-liquid-glass")
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.liquid_glass = !this.liquid_glass;
                this.save_soon(cx);
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(fg)
                    .child("Liquid Glass"),
            )
            .child(
                div()
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(if liquid_glass {
                        rgb(self.accent).into()
                    } else {
                        self.cell_bg()
                    })
                    .flex()
                    .items_center()
                    .when(liquid_glass, |d| d.justify_end())
                    .when(!liquid_glass, |d| d.justify_start())
                    .child(div().mx(px(3.0)).size(px(16.0)).rounded_full().bg(fg)),
            );

        let auto_adjust = self.auto_adjust_tiles;
        let auto_adjust_toggle = div()
            .id("setting-auto-adjust")
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.auto_adjust_tiles = !this.auto_adjust_tiles;
                if !this.auto_adjust_tiles {
                    this.seed_manual_positions();
                }
                this.save_soon(cx);
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(fg)
                    .child("Auto-Adjust Tiles"),
            )
            .child(
                div()
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(if auto_adjust {
                        rgb(self.accent).into()
                    } else {
                        self.cell_bg()
                    })
                    .flex()
                    .items_center()
                    .when(auto_adjust, |d| d.justify_end())
                    .when(!auto_adjust, |d| d.justify_start())
                    .child(div().mx(px(3.0)).size(px(16.0)).rounded_full().bg(fg)),
            );

        let tile_borders = self.tile_borders;
        let tile_borders_toggle = div()
            .id("setting-tile-borders")
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tile_borders = !this.tile_borders;
                this.save_soon(cx);
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(fg)
                    .child("Tile Borders"),
            )
            .child(
                div()
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(if tile_borders {
                        rgb(self.accent).into()
                    } else {
                        self.cell_bg()
                    })
                    .flex()
                    .items_center()
                    .when(tile_borders, |d| d.justify_end())
                    .when(!tile_borders, |d| d.justify_start())
                    .child(div().mx(px(3.0)).size(px(16.0)).rounded_full().bg(fg)),
            );

        let force_accent = self.force_accent_live_tiles;
        let force_accent_toggle = div()
            .id("setting-force-accent-live")
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.force_accent_live_tiles = !this.force_accent_live_tiles;
                this.save_soon(cx);
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(fg)
                    .child("Force Accent Color"),
            )
            .child(
                div()
                    .w(px(38.0))
                    .h(px(22.0))
                    .rounded_full()
                    .bg(if force_accent {
                        rgb(self.accent).into()
                    } else {
                        self.cell_bg()
                    })
                    .flex()
                    .items_center()
                    .when(force_accent, |d| d.justify_end())
                    .when(!force_accent, |d| d.justify_start())
                    .child(div().mx(px(3.0)).size(px(16.0)).rounded_full().bg(fg)),
            );

        let reset = div()
            .id("settings-reset")
            .mt(px(26.0))
            .h(px(42.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(fg.opacity(0.35))
            .rounded(px(2.0))
            .cursor_pointer()
            .active(|s| s.opacity(0.6))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(fg.opacity(0.85))
                    .child("Reset to Defaults"),
            )
            .on_click(cx.listener(|this, _, _, cx| this.reset_config(cx)));

        div()
            .id("settings-scroll")
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
                    .child(pivot_header("Settings".into(), "Theme".into(), key))
                    .child(caption("Theme"))
                    .child(div().flex().gap(px(24.0)).children(themes))
                    .child(force_accent_toggle)
                    .child(caption("Wallpapers"))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.0))
                            .when(self.wallpaper_files.is_empty(), |d| {
                                d.child(
                                    div()
                                        .py(px(6.0))
                                        .text_size(px(12.0))
                                        .text_color(fg.opacity(0.5))
                                        .child(format!(
                                            "No images in {} — drop some in and reopen this page",
                                            wallpaper_dir().display()
                                        )),
                                )
                            })
                            .children(wallpaper_images),
                    )
                    .child(caption("Gradient Presets"))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.0))
                            .children(wallpapers),
                    )
                    .child(caption("Accent Color"))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .children(swatches),
                    )
                    .child(caption("Appearance"))
                    .child(liquid_glass_toggle)
                    .when(self.liquid_glass, |d| {
                        d.child(caption("Tile Transparency")).child(alpha)
                    })
                    .child(caption("Tiles"))
                    .child(auto_adjust_toggle)
                    .child(tile_borders_toggle)
                    .child(caption("Tile Gap"))
                    .child(gap)
                    .child(caption("Lock Screen"))
                    .child(self.lock_settings_section(cx))
                    .child(reset),
            )
    }
}
