//! Role-based table styling: independent Color Theme and Border Theme (AC4.1+).
//! Resolved roles drive grid rendering and exporters for parity.

use crate::{Brightness, Tokens};
use egui::Color32;
use serde::{Deserialize, Serialize};

/// Color palette preset (applied together with a [`BorderThemeId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TableColorThemeId {
    #[default]
    ClassicGrid,
    Ledger,
    Minimal,
    Executive,
    CompactDense,
    HighContrast,
}

impl TableColorThemeId {
    pub const ALL: [TableColorThemeId; 6] = [
        TableColorThemeId::ClassicGrid,
        TableColorThemeId::Ledger,
        TableColorThemeId::Minimal,
        TableColorThemeId::Executive,
        TableColorThemeId::CompactDense,
        TableColorThemeId::HighContrast,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TableColorThemeId::ClassicGrid => "classicGrid",
            TableColorThemeId::Ledger => "ledger",
            TableColorThemeId::Minimal => "minimal",
            TableColorThemeId::Executive => "executive",
            TableColorThemeId::CompactDense => "compactDense",
            TableColorThemeId::HighContrast => "highContrast",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TableColorThemeId::ClassicGrid => "Classic Grid",
            TableColorThemeId::Ledger => "Ledger / Zebra",
            TableColorThemeId::Minimal => "Minimal",
            TableColorThemeId::Executive => "Executive Accent",
            TableColorThemeId::CompactDense => "Compact Dense",
            TableColorThemeId::HighContrast => "High Contrast",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.id().eq_ignore_ascii_case(s) || t.label().eq_ignore_ascii_case(s))
    }
}

/// Border preset (applied together with a [`TableColorThemeId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BorderThemeId {
    /// Full cell grid borders.
    Full,
    /// Horizontal rules for header/footer bands; body values without grid borders.
    #[default]
    HorizontalRules,
    /// No cell borders.
    None,
}

impl BorderThemeId {
    pub const ALL: [BorderThemeId; 3] = [BorderThemeId::Full, BorderThemeId::HorizontalRules, BorderThemeId::None];

    pub fn id(self) -> &'static str {
        match self {
            BorderThemeId::Full => "full",
            BorderThemeId::HorizontalRules => "horizontalRules",
            BorderThemeId::None => "none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BorderThemeId::Full => "Full Border",
            BorderThemeId::HorizontalRules => "Horizontal Rules (header/footer)",
            BorderThemeId::None => "No Border",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.id().eq_ignore_ascii_case(s) || t.label().eq_ignore_ascii_case(s))
    }
}

/// Backward-compatible combined theme parser for legacy `table_theme_id` values.
/// Returns `(color, border)`. Unknown values fall back to defaults.
pub fn parse_legacy_theme(s: &str) -> (TableColorThemeId, BorderThemeId) {
    if let Some(b) = BorderThemeId::parse(s) {
        return (TableColorThemeId::default(), b);
    }
    if s.eq_ignore_ascii_case("horizontalRules") || s.eq_ignore_ascii_case("Horizontal Rules (header/footer)") {
        return (TableColorThemeId::ClassicGrid, BorderThemeId::HorizontalRules);
    }
    if let Some(c) = TableColorThemeId::parse(s) {
        return (c, BorderThemeId::Full);
    }
    (TableColorThemeId::default(), BorderThemeId::default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellBorderMode {
    #[default]
    All,
    HorizontalOnly,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleStyle {
    pub fill: Color32,
    pub text: Color32,
    pub strong: bool,
    pub grid_line: Color32,
    pub border_mode: CellBorderMode,
}

/// Fully resolved table style roles for the current app brightness + table theme.
#[derive(Debug, Clone, Copy)]
pub struct TableStyle {
    /// Tokens of the brightness this style was resolved for (ink pair for derived text).
    pub tokens: Tokens,
    pub color: TableColorThemeId,
    pub border_theme: BorderThemeId,
    pub title: Color32,
    pub stub_head: RoleStyle,
    pub col_header: RoleStyle,
    pub row_header: RoleStyle,
    pub body: RoleStyle,
    pub body_zebra: RoleStyle,
    pub subtotal: RoleStyle,
    pub grand_total: RoleStyle,
    pub footer: RoleStyle,
    pub notes: RoleStyle,
    pub selection: Color32,
    pub border: Color32,
    pub outer_border: bool,
    pub zebra: bool,
    pub row_height: f32,
    pub header_height: f32,
}

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

impl TableStyle {
    pub fn resolve(color: TableColorThemeId, border_theme: BorderThemeId, b: Brightness, t: &Tokens) -> Self {
        if b == Brightness::HighContrast || color == TableColorThemeId::HighContrast {
            return Self::high_contrast(border_theme, t);
        }
        // Fills only — text colors are always derived via [`crate::color::text_on`]
        // so every role stays readable on every brightness × color-theme blend.
        let (header_fill, zebra_fill, total_fill, zebra) = match (color, t.dark) {
            (TableColorThemeId::ClassicGrid, true) => (hex(0x2f3745), blend(t.grid_body, t.panel_alt, 0.35), hex(0x3a4558), true),
            (TableColorThemeId::ClassicGrid, false) => (hex(0xd8e3f3), blend(t.grid_body, t.panel_alt, 0.3), hex(0xc7d7f0), true),
            (TableColorThemeId::Ledger, true) => (hex(0x334155), blend(t.grid_body, t.panel_alt, 0.55), hex(0x1f3b2d), true),
            (TableColorThemeId::Ledger, false) => (hex(0xd9eadc), blend(t.grid_body, hex(0xeaf4ed), 0.7), hex(0xcfe8d6), true),
            (TableColorThemeId::Minimal, true) => (t.panel, t.grid_body, t.panel_alt, false),
            (TableColorThemeId::Minimal, false) => (t.panel, t.grid_body, t.panel_alt, false),
            (TableColorThemeId::Executive, true) => (hex(0x1e3a5f), blend(t.grid_body, hex(0x16253d), 0.35), hex(0x2a4d7a), false),
            (TableColorThemeId::Executive, false) => (hex(0x1f4e79), blend(t.grid_body, hex(0xe8f0fa), 0.45), hex(0xd4e4f7), false),
            (TableColorThemeId::CompactDense, true) => (hex(0x3b4454), blend(t.grid_body, t.panel, 0.4), hex(0x454f61), true),
            (TableColorThemeId::CompactDense, false) => (hex(0xd5dde8), blend(t.grid_body, hex(0xe7edf5), 0.55), hex(0xc8d3e2), true),
            (TableColorThemeId::HighContrast, _) => unreachable!(),
        };
        let grid_line = if color == TableColorThemeId::Minimal { blend(t.grid_line, t.panel, 0.35) } else { t.grid_line };
        let row_h = if color == TableColorThemeId::CompactDense { 24.0 } else { 28.0 };
        let header_h = if color == TableColorThemeId::CompactDense { 26.0 } else { 30.0 };
        let (border_mode, body_border, outer_border) = match border_theme {
            BorderThemeId::Full => (CellBorderMode::All, CellBorderMode::All, true),
            BorderThemeId::HorizontalRules => (CellBorderMode::HorizontalOnly, CellBorderMode::None, false),
            BorderThemeId::None => (CellBorderMode::None, CellBorderMode::None, false),
        };
        let role = |fill, strong, bm| RoleStyle { text: crate::color::text_on(fill, t), fill, strong, grid_line, border_mode: bm };
        Self {
            tokens: *t,
            color,
            border_theme,
            title: crate::color::text_on(t.grid_body, t),
            stub_head: role(header_fill, true, border_mode),
            col_header: role(header_fill, true, border_mode),
            // Light themes blend row headers further toward the light panel so the
            // derived dark ink clears WCAG AA (mid-tone fills fail both inks).
            row_header: role(blend(header_fill, t.panel_alt, if t.dark { 0.2 } else { 0.5 }), true, border_mode),
            body: role(t.grid_body, false, body_border),
            body_zebra: role(zebra_fill, false, body_border),
            subtotal: role(blend(total_fill, header_fill, 0.45), true, border_mode),
            grand_total: role(total_fill, true, border_mode),
            footer: role(blend(total_fill, t.panel_alt, 0.6), true, border_mode),
            notes: role(t.panel_alt, false, border_mode),
            selection: t.selection,
            border: t.border,
            outer_border,
            zebra,
            row_height: row_h,
            header_height: header_h,
        }
    }

    fn high_contrast(border_theme: BorderThemeId, t: &Tokens) -> Self {
        let (border_mode, body_border, outer_border) = match border_theme {
            BorderThemeId::Full => (CellBorderMode::All, CellBorderMode::All, true),
            BorderThemeId::HorizontalRules => (CellBorderMode::HorizontalOnly, CellBorderMode::None, false),
            BorderThemeId::None => (CellBorderMode::None, CellBorderMode::None, false),
        };
        let role = |fill, text, strong, bm| RoleStyle {
            fill,
            text,
            strong,
            grid_line: Color32::WHITE,
            border_mode: bm,
        };
        Self {
            tokens: *t,
            color: TableColorThemeId::HighContrast,
            border_theme,
            title: Color32::WHITE,
            stub_head: role(Color32::BLACK, Color32::WHITE, true, border_mode),
            col_header: role(Color32::BLACK, Color32::WHITE, true, border_mode),
            row_header: role(Color32::BLACK, Color32::WHITE, true, border_mode),
            body: role(Color32::BLACK, Color32::WHITE, false, body_border),
            body_zebra: role(hex(0x101010), Color32::WHITE, false, body_border),
            subtotal: role(hex(0x1a1a00), hex(0xffea00), true, border_mode),
            grand_total: role(hex(0x222200), hex(0xffea00), true, border_mode),
            footer: role(Color32::BLACK, Color32::WHITE, true, border_mode),
            notes: role(Color32::BLACK, hex(0x00ffff), false, border_mode),
            selection: t.selection,
            border: Color32::WHITE,
            outer_border,
            zebra: true,
            row_height: 28.0,
            header_height: 30.0,
        }
    }

    pub fn role_for(&self, role: pivot_role::Role, zebra_row: bool) -> RoleStyle {
        use pivot_role::Role;
        match role {
            Role::StubHead => self.stub_head,
            Role::ColHeader => self.col_header,
            Role::RowHeader => self.row_header,
            Role::Body => if self.zebra && zebra_row { self.body_zebra } else { self.body },
            Role::Subtotal => self.subtotal,
            Role::GrandTotal => self.grand_total,
            Role::Footer => self.footer,
            Role::Notes => self.notes,
            Role::Title => RoleStyle {
                fill: self.body.fill,
                text: self.title,
                strong: true,
                grid_line: self.border,
                border_mode: CellBorderMode::None,
            },
        }
    }
}

/// UI-agnostic mapping target for model roles (kept tiny to avoid depending on pivot-core here).
pub mod pivot_role {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Role {
        Title,
        StubHead,
        ColHeader,
        RowHeader,
        Body,
        Subtotal,
        GrandTotal,
        Footer,
        Notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Brightness, Tokens};

    #[test]
    fn theme_ids_stable() {
        assert_eq!(TableColorThemeId::ALL.len(), 6);
        for t in TableColorThemeId::ALL {
            assert_eq!(TableColorThemeId::parse(t.id()), Some(t));
            assert_eq!(TableColorThemeId::parse(t.label()), Some(t));
        }
        assert_eq!(BorderThemeId::ALL.len(), 3);
        for t in BorderThemeId::ALL {
            assert_eq!(BorderThemeId::parse(t.id()), Some(t));
        }
        // legacy compat
        let (c, b) = parse_legacy_theme("horizontalRules");
        assert_eq!(c, TableColorThemeId::ClassicGrid);
        assert_eq!(b, BorderThemeId::HorizontalRules);
        let (c, b) = parse_legacy_theme("ledger");
        assert_eq!(c, TableColorThemeId::Ledger);
        assert_eq!(b, BorderThemeId::Full);
    }

    #[test]
    fn resolve_all_themes_and_brightnesses() {
        use crate::color::{contrast_ratio, over, text_on};
        for b in Brightness::ALL {
            let tokens = Tokens::for_brightness(b);
            for color in TableColorThemeId::ALL {
                for border in BorderThemeId::ALL {
                    let s = TableStyle::resolve(color, border, b, &tokens);
                    assert!(s.row_height > 0.0);
                    assert_eq!(s.color, if b == Brightness::HighContrast { TableColorThemeId::HighContrast } else { color });
                    assert_eq!(s.border_theme, border);
                    // Every role's text must be readable on its fill (WCAG AA),
                    // and on the selection overlay painted over the fill.
                    let roles = [
                        (s.stub_head, "stub_head"),
                        (s.col_header, "col_header"),
                        (s.row_header, "row_header"),
                        (s.body, "body"),
                        (s.body_zebra, "body_zebra"),
                        (s.subtotal, "subtotal"),
                        (s.grand_total, "grand_total"),
                        (s.footer, "footer"),
                        (s.notes, "notes"),
                    ];
                    for (r, name) in roles {
                        assert!(
                            contrast_ratio(r.text, r.fill) >= 4.5,
                            "{name} unreadable: {} in {color:?}/{b:?}",
                            contrast_ratio(r.text, r.fill)
                        );
                        let sel_fill = over(s.selection, r.fill);
                        let sel_text = text_on(sel_fill, &tokens);
                        assert!(
                            contrast_ratio(sel_text, sel_fill) >= 4.5,
                            "{name} selection text unreadable in {color:?}/{b:?}"
                        );
                    }
                    // Title is painted on the body fill.
                    assert!(contrast_ratio(s.title, s.body.fill) >= 4.5, "title unreadable in {color:?}/{b:?}");
                }
            }
        }
    }
}
