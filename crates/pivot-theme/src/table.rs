//! Role-based table themes and predefined presets (Phase 4, AC4.1-AC4.5).
//! Resolved roles drive grid rendering (and later exporters) for parity.

use crate::{Brightness, Tokens};
use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TableThemeId {
    #[default]
    ClassicGrid,
    Ledger,
    Minimal,
    Executive,
    CompactDense,
    HighContrast,
}

impl TableThemeId {
    pub const ALL: [TableThemeId; 6] = [
        TableThemeId::ClassicGrid,
        TableThemeId::Ledger,
        TableThemeId::Minimal,
        TableThemeId::Executive,
        TableThemeId::CompactDense,
        TableThemeId::HighContrast,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TableThemeId::ClassicGrid => "classicGrid",
            TableThemeId::Ledger => "ledger",
            TableThemeId::Minimal => "minimal",
            TableThemeId::Executive => "executive",
            TableThemeId::CompactDense => "compactDense",
            TableThemeId::HighContrast => "highContrast",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TableThemeId::ClassicGrid => "Classic Grid",
            TableThemeId::Ledger => "Ledger / Zebra",
            TableThemeId::Minimal => "Minimal",
            TableThemeId::Executive => "Executive Accent",
            TableThemeId::CompactDense => "Compact Dense",
            TableThemeId::HighContrast => "High Contrast",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.id().eq_ignore_ascii_case(s) || t.label().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleStyle {
    pub fill: Color32,
    pub text: Color32,
    pub strong: bool,
    pub grid_line: Color32,
}

/// Fully resolved table style roles for the current app brightness + table theme.
#[derive(Debug, Clone, Copy)]
pub struct TableStyle {
    pub id: TableThemeId,
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
    pub fn resolve(id: TableThemeId, b: Brightness, t: &Tokens) -> Self {
        if b == Brightness::HighContrast || id == TableThemeId::HighContrast {
            return Self::high_contrast(t);
        }
        let (header_fill, header_text, zebra_fill, total_fill, notes_text, zebra) = match (id, t.dark) {
            (TableThemeId::ClassicGrid, true) => (hex(0x2f3745), t.text_strong, t.grid_body, hex(0x3a4558), t.text_muted, false),
            (TableThemeId::ClassicGrid, false) => (hex(0xd8e3f3), hex(0x102a56), t.grid_body, hex(0xc7d7f0), t.text_muted, false),
            (TableThemeId::Ledger, true) => (hex(0x334155), t.text_strong, blend(t.grid_body, t.panel_alt, 0.55), hex(0x1f3b2d), t.text_muted, true),
            (TableThemeId::Ledger, false) => (hex(0xd9eadc), hex(0x14361f), blend(t.grid_body, hex(0xeaf4ed), 0.7), hex(0xcfe8d6), t.text_muted, true),
            (TableThemeId::Minimal, true) => (t.panel, t.text_strong, t.grid_body, t.panel_alt, t.text_muted, false),
            (TableThemeId::Minimal, false) => (t.panel, t.text_strong, t.grid_body, t.panel_alt, t.text_muted, false),
            (TableThemeId::Executive, true) => (hex(0x1e3a5f), hex(0xf8fbff), blend(t.grid_body, hex(0x16253d), 0.35), hex(0x2a4d7a), t.text_muted, false),
            (TableThemeId::Executive, false) => (hex(0x1f4e79), Color32::WHITE, blend(t.grid_body, hex(0xe8f0fa), 0.45), hex(0xd4e4f7), t.text_muted, false),
            (TableThemeId::CompactDense, true) => (hex(0x3b4454), t.text_strong, blend(t.grid_body, t.panel, 0.4), hex(0x454f61), t.text_muted, true),
            (TableThemeId::CompactDense, false) => (hex(0xd5dde8), hex(0x0f172a), blend(t.grid_body, hex(0xe7edf5), 0.55), hex(0xc8d3e2), t.text_muted, true),
            (TableThemeId::HighContrast, _) => unreachable!(),
        };
        let grid_line = if id == TableThemeId::Minimal { blend(t.grid_line, t.panel, 0.35) } else { t.grid_line };
        let row_h = if id == TableThemeId::CompactDense { 24.0 } else { 28.0 };
        let header_h = if id == TableThemeId::CompactDense { 26.0 } else { 30.0 };
        let role = |fill, text, strong| RoleStyle { fill, text, strong, grid_line };
        Self {
            id,
            title: t.text_strong,
            stub_head: role(header_fill, header_text, true),
            col_header: role(header_fill, header_text, true),
            row_header: role(blend(header_fill, t.panel_alt, if t.dark { 0.2 } else { 0.35 }), if id == TableThemeId::Executive && !t.dark { header_text } else { t.text_strong }, true),
            body: role(t.grid_body, t.text, false),
            body_zebra: role(zebra_fill, t.text, false),
            subtotal: role(blend(total_fill, header_fill, 0.45), t.text_strong, true),
            grand_total: role(total_fill, if id == TableThemeId::Executive && t.dark { Color32::WHITE } else { t.text_strong }, true),
            footer: role(blend(total_fill, t.panel_alt, 0.6), t.text_strong, true),
            notes: role(t.panel_alt, notes_text, false),
            selection: t.selection,
            border: t.border,
            zebra,
            row_height: row_h,
            header_height: header_h,
        }
    }

    fn high_contrast(t: &Tokens) -> Self {
        let role = |fill, text, strong| RoleStyle { fill, text, strong, grid_line: Color32::WHITE };
        Self {
            id: TableThemeId::HighContrast,
            title: Color32::WHITE,
            stub_head: role(Color32::BLACK, Color32::WHITE, true),
            col_header: role(Color32::BLACK, Color32::WHITE, true),
            row_header: role(Color32::BLACK, Color32::WHITE, true),
            body: role(Color32::BLACK, Color32::WHITE, false),
            body_zebra: role(hex(0x101010), Color32::WHITE, false),
            subtotal: role(hex(0x1a1a00), hex(0xffea00), true),
            grand_total: role(hex(0x222200), hex(0xffea00), true),
            footer: role(Color32::BLACK, Color32::WHITE, true),
            notes: role(Color32::BLACK, hex(0x00ffff), false),
            selection: t.selection,
            border: Color32::WHITE,
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
            Role::Title => RoleStyle { fill: self.body.fill, text: self.title, strong: true, grid_line: self.border },
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
        assert_eq!(TableThemeId::ALL.len(), 6);
        for t in TableThemeId::ALL {
            assert_eq!(TableThemeId::parse(t.id()), Some(t));
            assert_eq!(TableThemeId::parse(t.label()), Some(t));
        }
    }

    #[test]
    fn resolve_all_themes_and_brightnesses() {
        for b in Brightness::ALL {
            let tokens = Tokens::for_brightness(b);
            for id in TableThemeId::ALL {
                let s = TableStyle::resolve(id, b, &tokens);
                // basic legibility: text differs from fill for headers
                assert_ne!(s.col_header.text, s.col_header.fill);
                assert_ne!(s.grand_total.text, s.grand_total.fill);
                assert!(s.row_height > 0.0);
            }
        }
    }
}
