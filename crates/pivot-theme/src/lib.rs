//! Theme tokens and brightness modes for Pivot Studio.
//!
//! Design pattern reference: DesignCraft `crates/ui-egui/src/theme.rs`
//! (`reference/designcraft/crates/ui-egui/src/theme.rs` after restructure).
//! Re-implemented here as original code following the same *pattern*:
//! - `Brightness` modes with stable ids (incl. High Contrast)
//! - a central `Tokens` resolved per brightness
//! - `apply(ctx, tokens)` installs egui visuals/text styles/spacing
//! - widgets source colors from tokens (no ad-hoc literals in widget code)
//!
//! Phase 4 adds role-based table themes/presets (`table` module) rendered from
//! resolved roles for screen/export parity.

pub mod table;

use egui::{Color32, Context, CornerRadius, FontId, Margin, Stroke, Style, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

/// Interface brightness / color mode. Stable `id()` values are persisted/serialized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Brightness {
    Dark,
    #[default]
    MediumDark,
    MediumLight,
    Light,
    /// Accessibility: high-contrast chrome, strong text, clear focus/selection.
    HighContrast,
}

impl Brightness {
    pub const ALL: [Brightness; 5] = [
        Brightness::Dark,
        Brightness::MediumDark,
        Brightness::MediumLight,
        Brightness::Light,
        Brightness::HighContrast,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Brightness::Dark => "Dark",
            Brightness::MediumDark => "Medium Dark",
            Brightness::MediumLight => "Medium Light",
            Brightness::Light => "Light",
            Brightness::HighContrast => "High Contrast",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Brightness::Dark => "dark",
            Brightness::MediumDark => "mediumDark",
            Brightness::MediumLight => "mediumLight",
            Brightness::Light => "light",
            Brightness::HighContrast => "highContrast",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|b| b.id().eq_ignore_ascii_case(s) || b.label().eq_ignore_ascii_case(s))
    }
}

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Central UI color/metric tokens. Widgets must read from these, not hard-code colors.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    pub app_bar: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub canvas: Color32,
    pub grid_header: Color32,
    pub grid_body: Color32,
    pub grid_line: Color32,
    pub text: Color32,
    pub text_strong: Color32,
    pub text_muted: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_strong: Color32,
    pub selection: Color32,
    pub hover: Color32,
    pub input_bg: Color32,
    pub input_border: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub status_bar: Color32,
    pub focus: Color32,
}

impl Tokens {
    pub fn for_brightness(b: Brightness) -> Self {
        match b {
            Brightness::Dark => Self {
                dark: true,
                app_bar: hex(0x1f2329),
                panel: hex(0x262b33),
                panel_alt: hex(0x2d3340),
                canvas: hex(0x1a1e24),
                grid_header: hex(0x2f3745),
                grid_body: hex(0x232833),
                grid_line: hex(0x3a4354),
                text: hex(0xd7dde7),
                text_strong: hex(0xf5f7fb),
                text_muted: hex(0x9aa6b8),
                text_disabled: hex(0x667084),
                accent: hex(0x3b82f6),
                accent_strong: hex(0x2563eb),
                selection: Color32::from_rgba_unmultiplied(59, 130, 246, 90),
                hover: hex(0x343c4b),
                input_bg: hex(0x1c212b),
                input_border: hex(0x465166),
                border: hex(0x3a4354),
                divider: hex(0x323a48),
                status_bar: hex(0x1b2028),
                focus: hex(0x60a5fa),
            },
            Brightness::MediumDark => Self {
                dark: true,
                app_bar: hex(0x2b313b),
                panel: hex(0x333a46),
                panel_alt: hex(0x3a4250),
                canvas: hex(0x242a33),
                grid_header: hex(0x3d4656),
                grid_body: hex(0x2f3642),
                grid_line: hex(0x4a5568),
                text: hex(0xeaf0f8),
                text_strong: hex(0xffffff),
                // Brighter muted/inactive text so menu labels remain readable on app_bar/panels.
                text_muted: hex(0xc8d2e0),
                text_disabled: hex(0x9dabc0),
                accent: hex(0x4a90e2),
                accent_strong: hex(0x2f7dd6),
                selection: Color32::from_rgba_unmultiplied(74, 144, 226, 95),
                hover: hex(0x424c5c),
                input_bg: hex(0x28303b),
                input_border: hex(0x5a677c),
                border: hex(0x4a5568),
                divider: hex(0x404a59),
                status_bar: hex(0x262c36),
                focus: hex(0x7ab0f5),
            },
            Brightness::MediumLight => Self {
                dark: false,
                app_bar: hex(0xd9dde3),
                panel: hex(0xe6e9ee),
                panel_alt: hex(0xdfe3ea),
                canvas: hex(0xcfd5de),
                grid_header: hex(0xd3d9e3),
                grid_body: hex(0xf2f4f7),
                grid_line: hex(0xb8c1ce),
                text: hex(0x1f2937),
                text_strong: hex(0x111827),
                text_muted: hex(0x4b5563),
                text_disabled: hex(0x9aa3af),
                accent: hex(0x2563eb),
                accent_strong: hex(0x1d4ed8),
                selection: Color32::from_rgba_unmultiplied(37, 99, 235, 70),
                hover: hex(0xd5dbe4),
                input_bg: hex(0xf7f8fa),
                input_border: hex(0xaeb8c7),
                border: hex(0xb8c1ce),
                divider: hex(0xc7ced9),
                status_bar: hex(0xd4d9e1),
                focus: hex(0x2563eb),
            },
            Brightness::Light => Self {
                dark: false,
                app_bar: hex(0xf3f4f6),
                panel: hex(0xf9fafb),
                panel_alt: hex(0xf1f5f9),
                canvas: hex(0xe5e7eb),
                grid_header: hex(0xe8eef7),
                grid_body: hex(0xffffff),
                grid_line: hex(0xd1d5db),
                text: hex(0x1f2937),
                text_strong: hex(0x0f172a),
                text_muted: hex(0x6b7280),
                text_disabled: hex(0xa3aab6),
                accent: hex(0x2563eb),
                accent_strong: hex(0x1d4ed8),
                selection: Color32::from_rgba_unmultiplied(37, 99, 235, 60),
                hover: hex(0xe8edf5),
                input_bg: hex(0xffffff),
                input_border: hex(0xc4ccd8),
                border: hex(0xd1d5db),
                divider: hex(0xe2e6ec),
                status_bar: hex(0xeef0f3),
                focus: hex(0x2563eb),
            },
            Brightness::HighContrast => Self {
                dark: true,
                app_bar: hex(0x000000),
                panel: hex(0x000000),
                panel_alt: hex(0x101010),
                canvas: hex(0x141414),
                grid_header: hex(0x000000),
                grid_body: hex(0x000000),
                grid_line: hex(0xffffff),
                text: hex(0xffffff),
                text_strong: hex(0xffffff),
                text_muted: hex(0xf0f0f0),
                text_disabled: hex(0xb0b0b0),
                accent: hex(0xffd400),
                accent_strong: hex(0xffea00),
                selection: Color32::from_rgba_unmultiplied(255, 212, 0, 110),
                hover: hex(0x2a2a00),
                input_bg: hex(0x000000),
                input_border: hex(0xffffff),
                border: hex(0xffffff),
                divider: hex(0xffffff),
                status_bar: hex(0x000000),
                focus: hex(0x00ffff),
            },
        }
    }
}

/// Apply tokens to an egui context (visuals, spacing, text styles).
/// Follows the DesignCraft `theme::apply` pattern: one place to style the app.
pub fn apply(ctx: &Context, t: &Tokens) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, *t));
    let mut style = Style::default();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = Margin::same(8);
    style.spacing.menu_margin = Margin::same(6);

    style.text_styles = [
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.5)),
        (TextStyle::Heading, FontId::proportional(18.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.override_text_color = Some(t.text);
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = t.input_bg;
    v.faint_bg_color = t.panel_alt;
    v.selection.bg_fill = t.selection;
    v.selection.stroke = Stroke::new(1.0_f32, t.accent_strong);
    v.hyperlink_color = t.accent;
    v.widgets.noninteractive.bg_fill = t.panel;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, t.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, t.border);
    v.widgets.inactive.bg_fill = t.panel_alt;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, t.text);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, t.input_border);
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, t.text_strong);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, t.accent);
    v.widgets.active.bg_fill = t.accent;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, t.text_strong);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, t.accent_strong);
    v.widgets.open.bg_fill = t.panel_alt;
    v.widgets.open.fg_stroke = Stroke::new(1.0_f32, t.text);
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, t.border);
    let r = CornerRadius::same(4);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    style.visuals = v;
    ctx.set_style(style);
}

impl Tokens {
    /// Retrieve tokens stored by [`apply`] for use during painting.
    pub fn get(ctx: &Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::NULL))
            .unwrap_or_else(|| Tokens::for_brightness(Brightness::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_has_stable_ids_and_roundtrip() {
        assert_eq!(Brightness::ALL.len(), 5);
        for b in Brightness::ALL {
            assert_eq!(Brightness::parse(b.id()), Some(b));
            assert_eq!(Brightness::parse(b.label()), Some(b));
        }
        assert_eq!(Brightness::parse("highContrast"), Some(Brightness::HighContrast));
        assert_eq!(Brightness::parse("nope"), None);
    }

    #[test]
    fn tokens_resolve_for_all_modes() {
        for b in Brightness::ALL {
            let t = Tokens::for_brightness(b);
            // High contrast must use strong (black/white) text on chrome.
            if b == Brightness::HighContrast {
                assert_eq!(t.text, Color32::WHITE);
                assert_eq!(t.panel, Color32::BLACK);
                assert_ne!(t.focus, t.panel);
            }
        }
    }
}
