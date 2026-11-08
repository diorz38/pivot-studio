//! Canonical themed table model (UI-agnostic). Parity invariant:
//! UI and all exporters render this model (Phases 2-6). AC3.4.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CellRole {
    #[default]
    Body,
    StubHead,
    RowHeader,
    ColHeader,
    Subtotal,
    GrandTotal,
    Footer,
    Title,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Cell {
    pub text: String,
    pub role: CellRole,
    /// Column span (>=1).
    pub colspan: usize,
    /// Row span (>=1).
    pub rowspan: usize,
    /// Indent level for hierarchical row headers.
    pub indent: usize,
}

impl Cell {
    pub fn new(text: impl Into<String>, role: CellRole) -> Self {
        Self { text: text.into(), role, colspan: 1, rowspan: 1, indent: 0 }
    }
    pub fn span(text: impl Into<String>, role: CellRole, colspan: usize, rowspan: usize) -> Self {
        Self { text: text.into(), role, colspan: colspan.max(1), rowspan: rowspan.max(1), indent: 0 }
    }
    pub fn indented(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FooterBandKind {
    Subtotal,
    GrandTotal,
    Average,
    Count,
    Notes,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FooterBand {
    pub kind: FooterBandKind,
    pub label: String,
    /// Pre-resolved cells aligned to the model's column count (or a single spanning label + values).
    pub cells: Vec<Cell>,
    /// User-pinned label/value text override behavior is tracked by the generator/spec; model holds resolved text.
    pub pinned: bool,
}

/// Canonical, fully-resolved table for rendering/export. No egui types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ThemedTableModel {
    pub title: String,
    pub title_pinned: bool,
    /// Number of row-header columns on the left (>=1 when stub-head shown).
    pub row_header_cols: usize,
    /// Header rows (column headers, may include stub-head in first row_header_cols).
    pub header_rows: Vec<Vec<Cell>>,
    /// Body rows: row headers + value cells.
    pub body_rows: Vec<Vec<Cell>>,
    /// Ordered footer bands (AC3.2).
    pub footer_bands: Vec<FooterBand>,
}

impl ThemedTableModel {
    pub fn total_columns(&self) -> usize {
        self.header_rows
            .first()
            .map(|r| r.iter().map(|c| c.colspan).sum())
            .or_else(|| self.body_rows.first().map(|r| r.iter().map(|c| c.colspan).sum()))
            .unwrap_or(0)
    }
}
