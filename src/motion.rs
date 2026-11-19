//! Easing, colour helpers and small animated view primitives.

use crate::prelude::*;

// ───────────────────────────── motion helpers ─────────────────────────────

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Quartic ease-out, input clamped to 0..=1.
pub(crate) fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(4)
}

/// Cubic ease-in-out, input clamped to 0..=1.
pub(crate) fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// WCAG relative luminance for an 0xRRGGBB colour.
pub(crate) fn relative_luminance(hex: u32) -> f32 {
    fn channel(c: u32) -> f32 {
        let c = c as f32 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel((hex >> 16) & 0xFF)
        + 0.7152 * channel((hex >> 8) & 0xFF)
        + 0.0722 * channel(hex & 0xFF)
}

/// WCAG contrast ratio between two relative luminances.
fn contrast_ratio(a: f32, b: f32) -> f32 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

/// Contrast ratio between two opaque colours.
pub(crate) fn contrast_ratio_hex(a: u32, b: u32) -> f32 {
    contrast_ratio(relative_luminance(a), relative_luminance(b))
}

/// True when white ink is the better choice for `hex`. White is preferred
/// whenever it reaches WCAG AA (4.5:1), which keeps the Metro look on dark,
/// saturated colours; black is used only when white would fall short.
pub(crate) fn prefers_white(hex: u32) -> bool {
    contrast_ratio_hex(hex, 0xFFFFFF) >= 4.5
}

/// Pick black or white text for a coloured surface with a guaranteed contrast
/// floor, rather than a rough brightness threshold. Used by small UI controls.
pub(crate) fn on_color(hex: u32) -> Hsla {
    if prefers_white(hex) { white() } else { black() }
}

/// Metro-style tile ink. Tiles are large colour fields, and Metro uses white
/// type on saturated/dark plates even when black would score marginally higher
/// on WCAG. White is used for everything darker than a light pink; only lime,
/// gold, notes-orange and pink plates keep black ink.
pub(crate) fn prefers_white_tile(hex: u32) -> bool {
    relative_luminance(hex) < 0.34
}

/// Ink for a live-tile plate.
pub(crate) fn tile_ink(hex: u32) -> Hsla {
    if prefers_white_tile(hex) {
        white()
    } else {
        black()
    }
}

/// The opposite ink, for controls drawn on top of `tile_ink`.
pub(crate) fn tile_inverse_ink(hex: u32) -> Hsla {
    if prefers_white_tile(hex) {
        black()
    } else {
        white()
    }
}

/// Alpha-composite `top` over `bottom` (both opaque 0xRRGGBB) and return the
/// resulting opaque colour. Used to decide ink for translucent tiles.
pub(crate) fn blend_hex(bottom: u32, top: u32, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let b = ((bottom >> shift) & 0xFF) as f32;
        let t = ((top >> shift) & 0xFF) as f32;
        (b + (t - b) * a).round().clamp(0.0, 255.0) as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

// ───────────────────────────── view helpers ─────────────────────────────

pub(crate) fn label(name: &'static str, s: f32) -> Div {
    div()
        .absolute()
        .left(px(10.0 * s))
        .right(px(8.0 * s))
        .bottom(px(8.0 * s))
        .text_size(px(13.0 * s))
        .line_height(px(15.0 * s))
        .whitespace_nowrap()
        .text_ellipsis()
        .child(name)
}

pub(crate) fn glyph(icon: IconName, size: f32, color: Hsla) -> Icon {
    Icon::new(icon).w(px(size)).h(px(size)).text_color(color)
}

/// Pivot header. With `key` set, the big title slides in from the right and fades in
/// (the Windows Phone page-enter feel); the key makes it replay once per navigation.
pub(crate) fn pivot_header(small: String, big: String, key: Option<usize>) -> AnyElement {
    let header = div()
        .pt(px(8.0))
        .pb(px(12.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(small),
        )
        .child(
            div()
                .text_size(px(54.0))
                .line_height(px(60.0))
                .font_weight(FontWeight::LIGHT)
                .child(big),
        );
    match key {
        Some(k) => header
            .with_animation(
                ("pivot", k),
                Animation::new(Duration::from_millis(460)).with_easing(ease_out_quint()),
                |el, d| el.relative().left(px(90.0 * (1.0 - d))).opacity(d),
            )
            .into_any_element(),
        None => header.into_any_element(),
    }
}

/// Wrap a tile's plate for a turnstile transition: the plate keeps its full size inside a
/// clip whose width collapses toward the left edge (like a card hinged on its left side),
/// fading as it goes. `delay` (ms) staggers tiles.
pub(crate) fn turnstile(
    plate: Div,
    w: f32,
    h: f32,
    id: usize,
    turn: Turn,
    delay: f32,
) -> AnyElement {
    let (key, leaving) = match turn {
        Turn::Idle => return plate.into_any_element(),
        Turn::Enter(k) => (k, false),
        Turn::Leave(k) => (k, true),
    };
    const DUR: f32 = 380.0;
    let total = DUR + delay;
    div()
        .relative()
        .h_full()
        .overflow_hidden()
        .child(plate.w(px(w)).h(px(h)))
        .with_animation(
            (
                if leaving { "tile-leave" } else { "tile-enter" },
                id * 100_000 + key % 100_000,
            ),
            Animation::new(Duration::from_millis(total as u64)),
            move |el, d| {
                let p = ((d * total - delay) / DUR).clamp(0.0, 1.0);
                let e = ease_out(p);
                let (s, a) = if leaving {
                    (1.0 - 0.8 * e, 1.0 - e)
                } else {
                    (0.25 + 0.75 * e, e)
                };
                el.w(px(w * s)).opacity(a)
            },
        )
        .into_any_element()
}
