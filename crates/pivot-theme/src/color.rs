//! Central contrast helpers: text colors are *derived* from fills, never hand-picked.
//! Single source of truth for readable text on any fill, including blends and
//! selection overlays, across all brightness modes and table color themes.

use crate::Tokens;
use egui::Color32;

/// WCAG relative luminance of an opaque color, 0..=1.
pub fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG contrast ratio between two opaque colors, 1..=21.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Alpha `fg` composited over opaque `bg` (result is opaque).
/// egui stores colors premultiplied, so this is plain source-over: `out = fg + bg*(1-a)`.
pub fn over(fg: Color32, bg: Color32) -> Color32 {
    if fg.a() == 255 {
        return fg;
    }
    let a = fg.a() as f32 / 255.0;
    let mix = |f: u8, b: u8| (f as f32 + b as f32 * (1.0 - a)).round() as u8;
    Color32::from_rgb(mix(fg.r(), bg.r()), mix(fg.g(), bg.g()), mix(fg.b(), bg.b()))
}

/// Readable text color for an opaque fill, always from the token ink pair.
/// `ink_on_light` is dark ink, `ink_on_dark` near-white; the better-contrast one wins.
pub fn text_on(fill: Color32, t: &Tokens) -> Color32 {
    if contrast_ratio(t.ink_on_light, fill) >= contrast_ratio(t.ink_on_dark, fill) {
        t.ink_on_light
    } else {
        t.ink_on_dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_extremes() {
        let white = Color32::WHITE;
        let black = Color32::BLACK;
        assert!((contrast_ratio(white, black) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 0.001);
    }

    #[test]
    fn text_on_picks_polarity() {
        for b in crate::Brightness::ALL {
            let t = Tokens::for_brightness(b);
            let on_black = text_on(Color32::BLACK, &t);
            let on_white = text_on(Color32::WHITE, &t);
            assert!(contrast_ratio(on_black, Color32::BLACK) >= 4.5, "ink unreadable on black ({b:?})");
            assert!(contrast_ratio(on_white, Color32::WHITE) >= 4.5, "ink unreadable on white ({b:?})");
        }
    }

    #[test]
    fn over_composites_alpha() {
        let half_red = Color32::from_rgba_unmultiplied(255, 0, 0, 128);
        let got = over(half_red, Color32::WHITE);
        assert_eq!(got, Color32::from_rgb(255, 127, 127));
        assert_eq!(over(Color32::from_rgb(1, 2, 3), Color32::WHITE), Color32::from_rgb(1, 2, 3));
    }

    #[test]
    fn selection_overlay_keeps_text_readable() {
        for b in crate::Brightness::ALL {
            let t = Tokens::for_brightness(b);
            for fill in [t.grid_body, t.panel_alt, t.panel, Color32::BLACK, Color32::WHITE] {
                let sel = over(t.selection, fill);
                assert!(
                    contrast_ratio(text_on(sel, &t), sel) >= 4.5,
                    "selection text unreadable on {fill:?} in {b:?}"
                );
            }
        }
    }
}
