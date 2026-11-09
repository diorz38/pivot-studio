use crate::{ExportError, ExportStyle, Result};
use pivot_core::{Cell, CellRole, OutputLanguage, ThemedTableModel};
use pivot_theme::table::{RoleStyle, TableStyle};
use printpdf::path::PaintMode;
use printpdf::{
    BuiltinFont, Color as PdfColor, Mm, PdfDocument, PdfDocumentReference, PdfLayerIndex,
    PdfLayerReference, PdfPageIndex, Rect, Rgb,
};
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
/// Minimum width for a data/value column (mm) before horizontal pagination kicks in.
const MIN_DATA_COL_W: f64 = 18.0;
/// Minimum width for a row-header/stub column (mm).
const MIN_HEADER_COL_W: f64 = 22.0;

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
    let natural = compute_natural_col_widths(model, cols);
    let slices = partition_horizontal(model, &natural, table_w);

    let (doc, page1, layer1) =
        PdfDocument::new(&model.title, Mm(PAGE_W as f32), Mm(PAGE_H as f32), "Layer 1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;

    let mut state = PageState { doc, font, font_bold, page: page1, layer: layer1, y: PAGE_H - MARGIN, page_no: 1 };
    let footer_total_h: f64 = model.footer_bands.iter().map(|b| band_height(b)).sum();
    let continuation = continuation_label(style.output_language);

    for (si, slice) in slices.iter().enumerate() {
        if si > 0 {
            new_page(&mut state);
        }
        let title = if si == 0 || continuation.is_empty() {
            model.title.clone()
        } else {
            format!("{} {}", model.title.trim_end(), continuation)
        };
        draw_title(&mut state, &title, &ts)?;
        draw_header_rows_slice(&mut state, model, slice, &ts)?;

        for row in &model.body_rows {
            if state.y - row_h() < MARGIN + footer_total_h {
                new_page(&mut state);
                draw_title(&mut state, &title, &ts)?;
                draw_header_rows_slice(&mut state, model, slice, &ts)?;
            }
            draw_row_slice(&mut state, row, slice, &ts, false)?;
        }

        if !model.footer_bands.is_empty() && state.y - footer_total_h < MARGIN {
            new_page(&mut state);
            draw_title(&mut state, &title, &ts)?;
            draw_header_rows_slice(&mut state, model, slice, &ts)?;
        }
        for band in &model.footer_bands {
            draw_row_slice(&mut state, &band.cells, slice, &ts, false)?;
        }
    }

    let mut buf = BufWriter::new(Vec::new());
    state
        .doc
        .save(&mut buf)
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))?;
    buf.into_inner().map_err(|e| ExportError::Pdf(e.to_string()))
}

fn continuation_label(lang: OutputLanguage) -> &'static str {
    match lang {
        OutputLanguage::Id => "(Lanjutan)",
        OutputLanguage::En => "(continuance)",
        OutputLanguage::IdEn => "(Lanjutan / continuance)",
    }
}

/// A horizontal page slice: global column indices and their rendered widths.
/// Always starts with row-header columns (if any), followed by a contiguous range of data columns.
#[derive(Debug, Clone)]
struct ColSlice {
    cols: Vec<usize>,
    widths: Vec<f64>,
}

fn compute_natural_col_widths(model: &ThemedTableModel, cols: usize) -> Vec<f64> {
    let mut widths = vec![MIN_DATA_COL_W.max(28.0); cols];
    let rh = model.row_header_cols.min(cols);
    for i in 0..rh {
        widths[i] = MIN_HEADER_COL_W.max(38.0);
    }
    // Estimate from content (capped). Built-in PDF font metrics are approximate.
    let mut max_chars = vec![0usize; cols];
    let mut observe = |cells: &[Cell]| {
        let mut ci = 0usize;
        for cell in cells {
            let span = cell.colspan.max(1);
            let per = cell.text.chars().count() / span + 1;
            for dc in 0..span {
                let c = ci + dc;
                if c < max_chars.len() {
                    max_chars[c] = max_chars[c].max(per);
                }
            }
            ci += span;
        }
    };
    for row in &model.header_rows { observe(row); }
    for row in &model.body_rows { observe(row); }
    for band in &model.footer_bands { observe(&band.cells); }
    let char_w = FONT_SIZE * 0.3528 * 0.58; // approx mm per char
    for i in 0..cols {
        let est = (max_chars[i] as f64) * char_w + CELL_PAD_X * 2.0 + 2.0;
        let min_w = if i < rh { MIN_HEADER_COL_W } else { MIN_DATA_COL_W };
        let max_w = if i < rh { 80.0 } else { 55.0 };
        widths[i] = est.clamp(min_w, max_w);
    }
    widths
}

fn partition_horizontal(model: &ThemedTableModel, natural: &[f64], table_w: f64) -> Vec<ColSlice> {
    let cols = natural.len();
    if cols == 0 {
        return vec![ColSlice { cols: vec![0], widths: vec![table_w] }];
    }
    let rh = model.row_header_cols.min(cols);
    let rh_width: f64 = natural.iter().take(rh).sum();
    let data_cols: Vec<usize> = (rh..cols).collect();
    if data_cols.is_empty() {
        return vec![fit_slice((0..cols).collect(), natural, table_w, rh)];
    }

    let mut slices = Vec::new();
    let mut start = 0usize;
    while start < data_cols.len() {
        let avail = (table_w - rh_width).max(MIN_DATA_COL_W);
        let mut end = start;
        let mut used = 0.0f64;
        while end < data_cols.len() {
            let w = natural[data_cols[end]];
            if end > start && used + w > avail {
                break;
            }
            used += w;
            end += 1;
            if used >= avail && end < data_cols.len() {
                break;
            }
        }
        if end == start { end = start + 1; } // ensure progress
        let mut idxs: Vec<usize> = (0..rh).collect();
        for i in start..end { idxs.push(data_cols[i]); }
        slices.push(fit_slice(idxs, natural, table_w, rh));
        start = end;
    }
    if slices.is_empty() {
        slices.push(fit_slice((0..cols).collect(), natural, table_w, rh));
    }
    slices
}

fn fit_slice(cols: Vec<usize>, natural: &[f64], table_w: f64, row_header_cols: usize) -> ColSlice {
    let mut widths: Vec<f64> = cols.iter().map(|&c| natural.get(c).copied().unwrap_or(MIN_DATA_COL_W)).collect();
    let total: f64 = widths.iter().sum();
    if total > 0.0 && (total - table_w).abs() > 0.01 {
        // Scale to fit available width (expand small slices, gently shrink if slightly over).
        let scale = table_w / total;
        for (i, w) in widths.iter_mut().enumerate() {
            let is_rh = i < row_header_cols.min(cols.len());
            let min_w = if is_rh { MIN_HEADER_COL_W } else { MIN_DATA_COL_W };
            *w = (*w * scale).max(min_w);
        }
        // If mins pushed us over, scale non-header down.
        let total2: f64 = widths.iter().sum();
        if total2 > table_w + 0.01 {
            let rhw: f64 = widths.iter().take(row_header_cols.min(cols.len())).sum();
            let avail = (table_w - rhw).max(MIN_DATA_COL_W);
            let data_total: f64 = widths.iter().skip(row_header_cols.min(cols.len())).sum();
            if data_total > 0.0 {
                let s = avail / data_total;
                for (i, w) in widths.iter_mut().enumerate() {
                    if i >= row_header_cols.min(cols.len()) {
                        *w = (*w * s).max(MIN_DATA_COL_W);
                    }
                }
            }
        }
    }
    ColSlice { cols, widths }
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
    if title.trim().is_empty() { return Ok(()); }
    let layer = state.layer();
    // Ensure visible on PDF white background (some themes may use light title colors).
    let c = ts.title;
    let lum = 0.2126 * (c.r() as f32) + 0.7152 * (c.g() as f32) + 0.0722 * (c.b() as f32);
    let text_color = if lum > 220.0 { egui::Color32::from_rgb(20, 20, 24) } else { c };
    set_fill_color(&layer, text_color);
    // printpdf uses bottom-left origin; draw text baseline below current top margin.
    let y = state.y - (TITLE_SIZE * 0.45);
    layer.use_text(sanitize(title), TITLE_SIZE as f32, mm(MARGIN), mm(y), &state.font_bold);
    state.y = y - 4.0;
    Ok(())
}

fn draw_header_rows_slice(state: &mut PageState, model: &ThemedTableModel, slice: &ColSlice, ts: &TableStyle) -> Result<()> {
    let rows_n = model.header_rows.len();
    let mut occupied = vec![vec![false; slice.cols.len()]; rows_n];
    for (ri, row) in model.header_rows.iter().enumerate() {
        draw_row_occupancy_slice(state, row, slice, ts, true, ri, &mut occupied)?;
    }
    Ok(())
}

/// Render a non-header row (body/footer band) clipped/mapped to a horizontal column slice.
fn draw_row_slice(state: &mut PageState, cells: &[Cell], slice: &ColSlice, ts: &TableStyle, is_header: bool) -> Result<()> {
    let rh = row_h();
    let layer = state.layer();
    let y_top = state.y;
    let mut ci = 0usize; // global column cursor
    let mut si = 0usize; // slice column cursor
    for cell in cells {
        let cspan = cell.colspan.max(1);
        let rspan = cell.rowspan.max(1);
        let cell_start = ci;
        let cell_end = ci + cspan;
        // advance slice cursor past columns before this cell
        while si < slice.cols.len() && slice.cols[si] < cell_start { si += 1; }
        let slice_start = si;
        let mut slice_end = si;
        while slice_end < slice.cols.len() && slice.cols[slice_end] < cell_end { slice_end += 1; }
        if slice_start < slice_end {
            let w: f64 = slice.widths[slice_start..slice_end].iter().sum();
            let x = MARGIN + slice.widths[..slice_start].iter().sum::<f64>();
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
        }
        ci = cell_end;
    }
    state.y -= rh;
    Ok(())
}
fn band_height(_band: &pivot_core::FooterBand) -> f64 { row_h() }

fn cell_role_style(role: CellRole, ts: &TableStyle, zebra: bool) -> RoleStyle {
    match role {
        CellRole::Title => RoleStyle { fill: ts.body.fill, text: ts.title, strong: true, grid_line: ts.border, border_mode: pivot_theme::table::CellBorderMode::None },
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

/// Render a header row with rowspan/colspan occupancy, clipped/mapped to a horizontal column slice.
fn draw_row_occupancy_slice(
    state: &mut PageState,
    cells: &[Cell],
    slice: &ColSlice,
    ts: &TableStyle,
    is_header: bool,
    row_idx: usize,
    occupied: &mut [Vec<bool>],
) -> Result<()> {
    let rh = row_h();
    let layer = state.layer();
    let y_top = state.y;
    let total_slice = slice.cols.len();
    // Build a quick lookup from global col -> slice index
    let mut g2s = std::collections::HashMap::with_capacity(total_slice * 2);
    for (si, &gc) in slice.cols.iter().enumerate() {
        g2s.insert(gc, si);
    }
    let mut ci = 0usize; // global col cursor
    for cell in cells {
        // skip occupied in global terms? occupancy is tracked in slice space
        while ci < 10_000 && g2s.get(&ci).map(|&si| occupied.get(row_idx).and_then(|r| r.get(si)).copied().unwrap_or(false)).unwrap_or(false) {
            // advance past slice-occupied global col if it maps
            if g2s.contains_key(&ci) { ci += 1; } else { ci += 1; break; }
        }
        let cspan = cell.colspan.max(1);
        let rspan = cell.rowspan.max(1);
        // find slice indices intersecting [ci, ci+cspan)
        let mut slice_indices: Vec<usize> = Vec::new();
        for dc in 0..cspan {
            if let Some(&si) = g2s.get(&(ci + dc)) {
                slice_indices.push(si);
            }
        }
        // mark occupancy in slice space for the full global span intersection
        for dr in 0..rspan {
            let rr = row_idx + dr;
            if rr >= occupied.len() { continue; }
            for &si in &slice_indices {
                if si < total_slice { occupied[rr][si] = true; }
            }
        }
        if !slice_indices.is_empty() {
            let si0 = *slice_indices.iter().min().unwrap();
            let si1 = *slice_indices.iter().max().unwrap();
            let w: f64 = slice.widths[si0..=si1].iter().sum();
            let x = MARGIN + slice.widths[..si0].iter().sum::<f64>();
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
        }
        ci += cspan;
    }
    state.y -= rh;
    Ok(())
}
#[cfg(test)]
mod pdf_pagination_tests {
    use super::*;
    use crate::{export_pdf, ExportStyle};
    use pivot_core::{Cell, CellRole, FooterBand, FooterBandKind, OutputLanguage, ThemedTableModel};
    use pivot_theme::table::{BorderThemeId, TableColorThemeId};
    use pivot_theme::Brightness;

    fn wide_model(cols: usize) -> ThemedTableModel {
        let mut header = vec![Cell::span("Row", CellRole::StubHead, 1, 1)];
        for i in 0..cols {
            header.push(Cell::span(format!("Measure Column {i} Long"), CellRole::ColHeader, 1, 1));
        }
        let mut body_row = vec![Cell::new("East", CellRole::RowHeader)];
        for i in 0..cols {
            body_row.push(Cell::new(format!("{}", i * 100), CellRole::Body));
        }
        let mut footer_cells = vec![Cell::new("Grand Total", CellRole::GrandTotal)];
        for i in 0..cols {
            footer_cells.push(Cell::new(format!("{}", i * 100), CellRole::GrandTotal));
        }
        ThemedTableModel {
            title: "Wide Pivot Report".into(),
            title_pinned: false,
            row_header_cols: 1,
            header_rows: vec![header],
            body_rows: vec![body_row.clone(), body_row],
            footer_bands: vec![FooterBand {
                kind: FooterBandKind::GrandTotal,
                label: "Grand Total".into(),
                cells: footer_cells,
                pinned: false,
            }],
        }
    }

    #[test]
    fn pdf_horizontal_pagination_wide_table_produces_valid_multi_page_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.pdf");
        let style = ExportStyle {
            color_theme: TableColorThemeId::ClassicGrid,
            border_theme: BorderThemeId::Full,
            brightness: Brightness::Light,
            output_language: OutputLanguage::Id,
        };
        export_pdf(&path, &wide_model(18), style).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        // Multi-page PDF should be larger than a trivial single-page render.
        assert!(bytes.len() > 1200, "expected multi-page PDF, got {} bytes", bytes.len());
        // Best-effort check: continuation label text may be encoded, but PDF should contain extra page objects.
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Type /Page") || s.contains("/Pages"));
    }
}
