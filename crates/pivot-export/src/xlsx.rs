use crate::{ExportError, ExportStyle, Result};
use pivot_core::{Cell, CellRole, FooterBandKind, ThemedTableModel};
use pivot_theme::table::{RoleStyle, TableStyle};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet};
use std::path::Path;

pub fn write(path: &Path, model: &ThemedTableModel, style: ExportStyle) -> Result<()> {
    let ts = style.resolve();
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Pivot")
        .map_err(|e| ExportError::Xlsx(e.to_string()))?;

    let cols = model.total_columns().max(1) as u16;
    // Column widths
    for c in 0..cols {
        let w = if c < model.row_header_cols as u16 { 22.0 } else { 14.0 };
        ws.set_column_width(c, w).ok();
    }

    let title_fmt = make_format(
        &RoleStyle { fill: ts.body.fill, text: ts.title, strong: true, grid_line: ts.border },
        FormatAlign::Left,
        true,
    );
    let mut row: u32 = 0;
    // Title row
    if cols > 1 {
        ws.merge_range(row, 0, row, cols - 1, &model.title, &title_fmt)
            .map_err(|e| ExportError::Xlsx(e.to_string()))?;
    } else {
        ws.write_with_format(row, 0, &model.title, &title_fmt)
            .map_err(|e| ExportError::Xlsx(e.to_string()))?;
    }
    ws.set_row_height(row, 22.0).ok();
    row += 1;

    // Header rows
    for hrow in &model.header_rows {
        write_row(ws, row, hrow, &ts, false, 20.0)?;
        row += 1;
    }

    // Body rows
    for (ri, brow) in model.body_rows.iter().enumerate() {
        let zebra = ri % 2 == 1;
        write_row(ws, row, brow, &ts, zebra, 18.0)?;
        row += 1;
    }

    // Footer bands
    for band in &model.footer_bands {
        let _ = band_kind_name(band.kind);
        write_row(ws, row, &band.cells, &ts, false, 18.0)?;
        row += 1;
    }

    // Freeze header area (title + header rows) and row-header columns
    let freeze_rows = 1 + model.header_rows.len() as u32;
    ws.set_freeze_panes(freeze_rows, model.row_header_cols as u16)
        .ok();

    wb.save(path)
        .map_err(|e| ExportError::Xlsx(e.to_string()))?;
    Ok(())
}

fn write_row(ws: &mut Worksheet, row: u32, cells: &[Cell], ts: &TableStyle, zebra: bool, height: f64) -> Result<()> {
    ws.set_row_height(row, height).ok();
    let mut col: u16 = 0;
    for cell in cells {
        let rs = cell_role_style(cell.role, ts, zebra);
        let align = if is_value_role(cell.role) && looks_numeric(&cell.text) { FormatAlign::Right } else { FormatAlign::Left };
        let fmt = make_format(&rs, align, rs.strong);
        let fmt = if cell.indent > 0 {
            fmt.set_indent(cell.indent.min(8) as u8)
        } else {
            fmt
        };
        let cspan = (cell.colspan.max(1) - 1) as u16;
        let rspan = (cell.rowspan.max(1) - 1) as u32;
        if cspan > 0 || rspan > 0 {
            ws.merge_range(row, col, row + rspan, col + cspan, &cell.text, &fmt)
                .map_err(|e| ExportError::Xlsx(e.to_string()))?;
        } else if let Some(n) = parse_number(&cell.text) {
            ws.write_number_with_format(row, col, n, &fmt)
                .map_err(|e| ExportError::Xlsx(e.to_string()))?;
        } else {
            ws.write_string_with_format(row, col, &cell.text, &fmt)
                .map_err(|e| ExportError::Xlsx(e.to_string()))?;
        }
        col += cell.colspan.max(1) as u16;
    }
    Ok(())
}

fn make_format(rs: &RoleStyle, align: FormatAlign, bold: bool) -> Format {
    let mut f = Format::new()
        .set_background_color(color(rs.fill))
        .set_font_color(color(rs.text))
        .set_border(FormatBorder::Thin)
        .set_border_color(color(rs.grid_line))
        .set_align(align)
        .set_align(rust_xlsxwriter::FormatAlign::VerticalCenter);
    if bold {
        f = f.set_bold();
    }
    f
}

fn color(c: egui::Color32) -> Color {
    Color::RGB(((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32)
}

fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() || t.len() > 32 { return None; }
    // Avoid mis-parsing identifiers/labels that happen to be numeric-like in odd ways
    if t.chars().any(|c| c.is_alphabetic()) { return None; }
    t.replace(',', "").parse::<f64>().ok().filter(|n| n.is_finite())
}

fn looks_numeric(text: &str) -> bool {
    parse_number(text).is_some()
}

fn is_value_role(role: CellRole) -> bool {
    matches!(role, CellRole::Body | CellRole::Subtotal | CellRole::GrandTotal | CellRole::Footer)
}

fn cell_role_style(role: CellRole, ts: &TableStyle, zebra: bool) -> RoleStyle {
    match role {
        CellRole::Title => RoleStyle { fill: ts.body.fill, text: ts.title, strong: true, grid_line: ts.border },
        CellRole::StubHead => ts.stub_head,
        CellRole::ColHeader => ts.col_header,
        CellRole::RowHeader => ts.row_header,
        CellRole::Body => if ts.zebra && zebra { ts.body_zebra } else { ts.body },
        CellRole::Subtotal => ts.subtotal,
        CellRole::GrandTotal => ts.grand_total,
        CellRole::Footer => ts.footer,
    }
}

fn band_kind_name(kind: FooterBandKind) -> &'static str {
    match kind {
        FooterBandKind::Subtotal => "subtotal",
        FooterBandKind::GrandTotal => "grand_total",
        FooterBandKind::Average => "average",
        FooterBandKind::Count => "count",
        FooterBandKind::Notes => "notes",
        FooterBandKind::Custom => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::reference_model;
    use crate::ExportStyle;

    #[test]
    fn xlsx_writes_nonempty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.xlsx");
        write(&path, &reference_model(), ExportStyle::default()).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > 1000, "xlsx too small: {}", meta.len());
    }
}
