//! Exporters consuming the canonical [`pivot_core::ThemedTableModel`] (Phase 6).
//!
//! Parity invariant: UI and all exporters render the same resolved model.
//! Theme roles are resolved via [`pivot_theme::table::TableStyle`] for the
//! selected table theme and brightness.

pub mod html;
pub mod pdf;
pub mod xlsx;

use pivot_core::ThemedTableModel;
use pivot_theme::table::{TableStyle, TableThemeId};
use pivot_theme::{Brightness, Tokens};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("xlsx: {0}")]
    Xlsx(String),
    #[error("pdf: {0}")]
    Pdf(String),
    #[error("html: {0}")]
    Html(String),
    #[error("nothing to export: compute a pivot first")]
    Empty,
}

pub type Result<T> = std::result::Result<T, ExportError>;

#[derive(Debug, Clone, Copy)]
pub struct ExportStyle {
    pub table_theme: TableThemeId,
    pub brightness: Brightness,
}

impl Default for ExportStyle {
    fn default() -> Self {
        Self { table_theme: TableThemeId::default(), brightness: Brightness::MediumDark }
    }
}

impl ExportStyle {
    pub fn resolve(self) -> TableStyle {
        let tokens = Tokens::for_brightness(self.brightness);
        TableStyle::resolve(self.table_theme, self.brightness, &tokens)
    }
}

pub fn export_html(path: impl AsRef<Path>, model: &ThemedTableModel, style: ExportStyle) -> Result<()> {
    let html = html::render(model, style);
    std::fs::write(path, html)?;
    Ok(())
}

pub fn export_xlsx(path: impl AsRef<Path>, model: &ThemedTableModel, style: ExportStyle) -> Result<()> {
    xlsx::write(path.as_ref(), model, style)
}

pub fn export_pdf(path: impl AsRef<Path>, model: &ThemedTableModel, style: ExportStyle) -> Result<()> {
    pdf::write(path.as_ref(), model, style)
}

pub fn export(path: impl AsRef<Path>, model: &ThemedTableModel, style: ExportStyle) -> Result<()> {
    let path = path.as_ref();
    match path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => export_html(path, model, style),
        "xlsx" => export_xlsx(path, model, style),
        "pdf" => export_pdf(path, model, style),
        _ => Err(ExportError::Html("unsupported export extension (use .html, .xlsx, .pdf)".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pivot_core::{Cell, CellRole, FooterBand, FooterBandKind, ThemedTableModel};

    pub fn reference_model() -> ThemedTableModel {
        ThemedTableModel {
            title: "Sales by Region".into(),
            title_pinned: false,
            row_header_cols: 1,
            header_rows: vec![vec![
                Cell::span("Region / Metric", CellRole::StubHead, 1, 1),
                Cell::span("Sales", CellRole::ColHeader, 1, 1),
                Cell::span("Qty", CellRole::ColHeader, 1, 1),
            ]],
            body_rows: vec![
                vec![Cell::new("East", CellRole::RowHeader), Cell::new("120", CellRole::Body), Cell::new("3", CellRole::Body)],
                vec![Cell::new("West", CellRole::RowHeader), Cell::new("80", CellRole::Body), Cell::new("2", CellRole::Body)],
            ],
            footer_bands: vec![
                FooterBand { kind: FooterBandKind::GrandTotal, label: "Grand Total".into(), cells: vec![Cell::new("Grand Total", CellRole::GrandTotal), Cell::new("200", CellRole::GrandTotal), Cell::new("5", CellRole::GrandTotal)], pinned: false },
                FooterBand { kind: FooterBandKind::Notes, label: "Notes".into(), cells: vec![Cell::span("Notes: Source: demo | Rows: 2 | Filters: none", CellRole::Footer, 3, 1)], pinned: false },
            ],
        }
    }

    #[test]
    fn html_has_structure_and_roles() {
        let html = html::render(&reference_model(), ExportStyle::default());
        assert!(html.contains("<table"));
        assert!(html.contains("<caption>Sales by Region</caption>"));
        assert!(html.contains("Region / Metric"));
        assert!(html.contains("Grand Total"));
        assert!(html.contains("Filters: none"));
        assert!(html.contains("data-role=\"stub_head\""));
        assert!(html.contains("data-role=\"grand_total\""));
    }

    #[test]
    fn pdf_writes_valid_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.pdf");
        crate::pdf::write(&path, &reference_model(), ExportStyle::default()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "not a PDF: {:?}", &bytes[..8.min(bytes.len())]);
        assert!(bytes.len() > 500, "pdf too small: {}", bytes.len());
    }

    #[test]
    fn export_dispatch_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        for (name, magic) in [("a.html", "<!doctype".as_bytes()), ("b.xlsx", "PK".as_bytes()), ("c.pdf", "%PDF".as_bytes())] {
            let path = dir.path().join(name);
            crate::export(&path, &reference_model(), ExportStyle::default()).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.starts_with(magic), "{name} bad magic: {:?}", &bytes[..4.min(bytes.len())]);
        }
    }
}