use crate::{ExportError, ExportStyle, Result};
use pivot_core::{Cell, CellRole, ThemedTableModel};
use pivot_theme::table::{RoleStyle, TableStyle};
use printpdf::path::PaintMode;
use printpdf::{
    BuiltinFont, Color as PdfColor, Mm, PdfDocument, PdfDocumentReference, PdfLayerIndex,
    PdfLayerReference, PdfPageIndex, Rect, Rgb,
};
use std::collections::HashSet;
use std::io::BufWriter;
use std::path::Path;

const PAGE_W: f64 = 297.0; // A4 landscape mm
const PAGE_H: f64 = 210.0;
const MARGIN: f64 = 10.0;
const FONT_SIZE: f64 = 9.0;
const TITLE_SIZE: f64 = 14.0;
const LINE_H: f64 = 5.2;
const CELL_PAD_X: f64 = 1.6;
const CELL_PAD_Y: f64 = 1.2;

pub fn write(path: &Path, model: &ThemedTableModel, style: ExportStyle) -> Result<()> {
    let bytes = render_bytes(model, style)?;
    // AC6.5-safe write: temp sibling then rename (best-effort; fallback direct).
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("pdf")
    ));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        let bytes = render_bytes(model, style)?;
        std::fs::write(path, bytes).map_err(|e2| ExportError::Io(std::io::Error::new(e2.kind(), format!("{e}; fallback: {e2}"))))?;
    }
    Ok(())
}

fn render_bytes(model: &ThemedTableModel, style: ExportStyle) -> Result<Vec<u8>> {
    let ts = style.resolve();
    let cols = model.total_columns().max(1);
    let table_w = PAGE_W - MARGIN * 2.0;
    let col_w = compute_col_widths(model, cols, table_w);

    let (doc, page1, layer1) =
        PdfDocument::new(&model.title, Mm(PAGE_W as f32), Mm(PAGE_H as f32), "Layer 1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;

    let mut state = PageState { doc, font, font_bold, page: page1, layer: layer1, y: PAGE_H - MARGIN, page_no: 1 };

    draw_title(&mut state, &model.title, &ts)?;
    let footer_total_h: f64 = model.footer_bands.iter().map(|b| band_height(b)).sum();

    draw_header_rows(&mut state, model, &col_w, &ts)?;

    for row in &model.body_rows {
        if state.y - row_h() < MARGIN + footer_total_h {
            new_page(&mut state);
            draw_header_rows(&mut state, model, &col_w, &ts)?;
        }
        draw_model_row(&mut state, row, &col_w, &ts, false)?;
    }

    if !model.footer_bands.is_empty() && state.y - footer_total_h < MARGIN {
        new_page(&mut state);
        draw_header_rows(&mut state, model, &col_w, &ts)?;
    }
    for band in &model.footer_bands {
        draw_model_row(&mut state, &band.cells, &col_w, &ts, false)?;
    }

    let mut buf = BufWriter::new(Vec::new());
    state
        .doc
        .save(&mut buf)
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;
    buf.into_inner().map_err(|e| ExportError::Pdf(e.to_string()))
}

struct PageState {
    doc: PdfDocumentReference,
    font: printpdf::IndirectFontRef,
    font_bold: printpdf::IndirectFontRef,
    page: PdfPageIndex,
    layer: PdfLayerIndex,
    y: f64,
    page_no: usize,
}
impl PageState {
    fn layer(&self) -> PdfLayerReference {
        self.doc.get_page(self.page).get_layer(self.layer)
    }
}
fn mm(v: f64) -> Mm { Mm(v as f32) }
fn row_h() -> f64 { LINE_H + CELL_PAD_Y * 2.0 }

fn new_page(state: &mut PageState) {
    let (page, layer) = state.doc.add_page(Mm(PAGE_W as f32), Mm(PAGE_H as f32), format!("Layer {}", state.page_no + 1));
    state.page = page;
    state.layer = layer;
    state.y = PAGE_H - MARGIN;
    state.page_no += 1;
}

fn draw_title(state: &mut PageState, title: &str, ts: &TableStyle) -> Result<()> {
    let layer = state.layer();
    set_fill_color(&layer, ts.title);
    layer.use_text(sanitize(title), TITLE_SIZE as f32, mm(MARGIN), mm(state.y), &state.font_bold);
    state.y -= TITLE_SIZE * 0.5 + 4.0;
    Ok(())
}

fn draw_header_rows(state: &mut PageState, model: &ThemedTableModel, col_w: &[f64], ts: &TableStyle) -> Result<()> {
    // Render each header row honoring colspan; rowspan is rare in current model but
    // we respect it by painting at full row height (header rows are repeated per page).
    for row in &model.header_rows {
        draw_model_row(state, row, col_w, ts, true)?;
    }
    Ok(())
}

fn draw_model_row(state: &mut PageState, cells: &[Cell], col_w: &[f64], ts: &TableStyle, is_header: bool) -> Result<()> {
    let rh = row_h();
    let layer = state.layer();
    let mut x = MARGIN;
    let mut ci = 0usize;
    let y_top = state.y;
    for cell in cells {
        let cspan = cell.colspan.max(1);
        let rspan = cell.rowspan.max(1);
        let w: f64 = col_w.iter().skip(ci).take(cspan).sum();
        let h = rh * rspan as f64;
        let rs = cell_role_style(cell.role, ts, false);
        set_fill_color(&layer, rs.fill);
        layer.add_rect(Rect::new(mm(x), mm(y_top - h), mm(x + w), mm(y_top)).with_mode(PaintMode::Fill));
        set_stroke_color(&layer, rs.grid_line);
        layer.add_rect(Rect::new(mm(x), mm(y_top - h), mm(x + w), mm(y_top)).with_mode(PaintMode::Stroke));
        set_fill_color(&layer, rs.text);
        let font = if rs.strong || is_header { &state.font_bold } else { &state.font };
        let tx = x + CELL_PAD_X + (cell.indent as f64) * 2.5;
        let text = truncate_to_width(&cell.text, w - CELL_PAD_X * 2.0);
        layer.use_text(sanitize(&text), FONT_SIZE as f32, mm(tx), mm(y_top - rh + CELL_PAD_Y + 0.8), font);
        x += w;
        ci += cspan;
    }
    // Only advance by one visual row; rowspan is contained within the repeated header band.
    state.y -= rh;
    let _ = HashSet::<usize>::new();
    Ok(())
}

fn compute_col_widths(model: &ThemedTableModel, cols: usize, table_w: f64) -> Vec<f64> {
    let mut widths = vec![table_w / cols as f64; cols];
    if model.row_header_cols > 0 && model.row_header_cols < cols {
        let header_total = (table_w * 0.28).min(table_w * 0.45);
        let each_h = header_total / model.row_header_cols as f64;
        let rest = (table_w - header_total) / (cols - model.row_header_cols) as f64;
        for i in 0..cols {
            widths[i] = if i < model.row_header_cols { each_h } else { rest };
        }
    }
    widths
}
fn band_height(_band: &pivot_core::FooterBand) -> f64 { row_h() }

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

fn set_fill_color(layer: &PdfLayerReference, c: egui::Color32) {
    layer.set_fill_color(PdfColor::Rgb(Rgb::new(c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, None)));
}
fn set_stroke_color(layer: &PdfLayerReference, c: egui::Color32) {
    layer.set_outline_color(PdfColor::Rgb(Rgb::new(c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, None)));
    layer.set_outline_thickness(0.2);
}
fn sanitize(s: &str) -> String { s.chars().map(|c| if c.is_ascii() { c } else { '?' }).collect() }
fn truncate_to_width(s: &str, width_mm: f64) -> String {
    let max_chars = ((width_mm / (FONT_SIZE * 0.3528 * 0.55)).floor() as usize).max(1);
    let t = sanitize(s);
    if t.chars().count() <= max_chars { t } else { t.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "~" }
}
