//! Wallpaper and theme colours.

use crate::prelude::*;

impl Metro {
    pub(crate) fn wallpaper_pair(&self, i: usize) -> (u32, u32) {
        let list = if self.dark {
            &WALLPAPERS_DARK
        } else {
            &WALLPAPERS_LIGHT
        };
        list[i.min(list.len() - 1)]
    }

    pub(crate) fn wallpaper_colors(&self) -> (u32, u32) {
        self.wallpaper_pair(self.wallpaper)
    }

    pub(crate) fn wallpaper_view(&self) -> Div {
        let accent: Hsla = rgb(self.accent).into();
        let line = self.fg().opacity(0.05);
        let line_faint = self.fg().opacity(0.025);

        let base = div()
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .overflow_hidden();

        // A real wallpaper image (if one is selected) is cover-cropped behind a
        // light accent wash and a scrim so the tiles' ink stays readable.
        let base = if let Some(path) = &self.wallpaper_file {
            base.bg(rgb(0x101010))
                .child(img(path.clone()).object_fit(ObjectFit::Cover).size_full())
                .child(div().absolute().size_full().bg(linear_gradient(
                    135.0,
                    linear_color_stop(accent.opacity(0.08), 0.0),
                    linear_color_stop(accent.opacity(if self.dark { 0.26 } else { 0.14 }), 1.0),
                )))
                .child(div().absolute().size_full().bg(hsla(
                    0.0,
                    0.0,
                    0.0,
                    if self.dark { 0.28 } else { 0.10 },
                )))
        } else {
            let (from, to) = self.wallpaper_colors();
            base.bg(linear_gradient(
                135.0,
                linear_color_stop(rgb(from), 0.0),
                linear_color_stop(rgb(to), 1.0),
            ))
            .child(div().absolute().size_full().bg(linear_gradient(
                135.0,
                linear_color_stop(accent.opacity(0.0), 0.3),
                linear_color_stop(accent.opacity(if self.dark { 0.22 } else { 0.16 }), 1.0),
            )))
        };

        base.child(
            div()
                .absolute()
                .top(px(-150.0))
                .right(px(-120.0))
                .size(px(430.0))
                .rounded_full()
                .bg(accent)
                .opacity(if self.dark { 0.09 } else { 0.07 }),
        )
        .child(
            div()
                .absolute()
                .bottom(px(-180.0))
                .left(px(-140.0))
                .size(px(380.0))
                .rounded_full()
                .bg(accent)
                .opacity(if self.dark { 0.06 } else { 0.05 }),
        )
        .child(
            div()
                .absolute()
                .bottom(px(30.0))
                .right(px(-70.0))
                .size(px(220.0))
                .rounded_full()
                .border_1()
                .border_color(self.fg().opacity(0.06)),
        )
        .child(
            div()
                .absolute()
                .top(px(120.0))
                .left(px(-60.0))
                .w(px(560.0))
                .h(px(1.0))
                .bg(line),
        )
        .child(
            div()
                .absolute()
                .top(px(168.0))
                .left(px(-60.0))
                .w(px(560.0))
                .h(px(1.0))
                .bg(line_faint),
        )
    }

    // ── theme ──

    pub(crate) fn fg(&self) -> Hsla {
        if self.dark { white() } else { black() }
    }

    /// Opaque-ish backdrop for list / settings pages so text stays readable
    /// while a hint of the wallpaper still shows through.
    pub(crate) fn page_bg(&self) -> Hsla {
        if self.dark {
            hsla(0.0, 0.0, 0.06, 0.95)
        } else {
            hsla(0.0, 0.0, 1.0, 0.95)
        }
    }

    pub(crate) fn panel_bg(&self) -> Hsla {
        if self.dark {
            hsla(0.0, 0.0, 0.10, 1.0)
        } else {
            hsla(0.0, 0.0, 0.97, 1.0)
        }
    }

    pub(crate) fn cell_bg(&self) -> Hsla {
        if self.dark {
            hsla(0.0, 0.0, 0.18, 1.0)
        } else {
            hsla(0.0, 0.0, 0.88, 1.0)
        }
    }
}
