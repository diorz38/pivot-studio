//! Custom-painted pivot grid (AC2.5): sticky headers, column resize, selection+copy (TSV),
//! and colspan/rowspan rendering for `ThemedTableModel`.

use egui::{FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use pivot_core::model::{CellRole, ThemedTableModel};
use pivot_core::spec::FooterBandSpec;
use pivot_theme::table::{CellBorderMode, TableStyle};
use pivot_theme::Tokens;
use crate::app::PivotApp;

const PAD_X: f32 = 10.0;

/// Numeric cell text (int/decimal, optional thousands separators or % sign).
fn is_numeric(s: &str) -> bool {
    let t = s.trim().trim_end_matches('%');
    !t.is_empty() && !t.contains(|c: char| !c.is_ascii_digit() && c != ',' && c != '.' && c != '-' && c != '+') && t.parse::<f64>().is_ok()
}

/// Font for a cell: numerics get tabular monospace (caller right-aligns).
fn cell_font(text: &str, size: f32) -> (egui::FontId, bool) {
    if is_numeric(text) { (egui::FontId::monospace(size - 0.5), true) } else { (egui::FontId::proportional(size), false) }
}
const MIN_COL_W: f32 = 50.0;
const RESIZE_HIT: f32 = 5.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridEditTarget {
    GrandTotal,
    RowSubtotal,
    ColumnTotal,
    ColumnSubtotal,
    FooterBand(usize),
}

#[derive(Clone, Debug)]
pub struct GridEditSession {
    pub target: GridEditTarget,
    pub text: String,
    pub original: String,
    pub initial_focus_done: bool,
}

#[derive(Clone, Default)]
pub struct GridUiState {
    pub col_widths: Vec<f32>,
    pub model_key: u64,
    /// (visual_row, col) in the flattened table (header rows first, then body, then footer rows).
    pub sel_anchor: Option<(usize, usize)>,
    pub sel_focus: Option<(usize, usize)>,
    pub copy_status: Option<String>,
    pub active_edit: Option<GridEditSession>,
}

impl GridUiState {
    pub fn load(ctx: &egui::Context, id: Id) -> Self {
        ctx.data(|d| d.get_temp::<Self>(id)).unwrap_or_default()
    }
    pub fn store(self, ctx: &egui::Context, id: Id) {
        ctx.data_mut(|d| d.insert_temp(id, self));
    }
    pub fn selection(&self) -> Option<(usize, usize, usize, usize)> {
        let (ar, ac) = self.sel_anchor?;
        let (fr, fc) = self.sel_focus?;
        Some((ar.min(fr), ac.min(fc), ar.max(fr), ac.max(fc)))
    }
    pub fn clear_selection(&mut self) {
        self.sel_anchor = None;
        self.sel_focus = None;
    }
}

fn model_key(model: &ThemedTableModel) -> u64 {
    // cheap identity: dimensions + title + first/last body text markers
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    model.total_columns().hash(&mut h);
    model.header_rows.len().hash(&mut h);
    model.body_rows.len().hash(&mut h);
    model.footer_bands.len().hash(&mut h);
    model.title.hash(&mut h);
    h.finish()
}

fn default_widths(model: &ThemedTableModel) -> Vec<f32> {
    let n = model.total_columns().max(1);
    let mut w = vec![110.0; n];
    let header_cols = model.row_header_cols.min(n);
    w[..header_cols].fill(140.0);
    w
}

fn ensure_widths(state: &mut GridUiState, model: &ThemedTableModel) {
    let n = model.total_columns().max(1);
    let key = model_key(model);
    if state.col_widths.len() != n || state.model_key != key {
        // preserve existing widths where possible
        let mut nw = default_widths(model);
        for (i, v) in state.col_widths.iter().enumerate() {
            if i < nw.len() { nw[i] = *v; }
        }
        state.col_widths = nw;
        state.model_key = key;
    }
}

pub fn target_for_cell(
    model: &ThemedTableModel,
    header_rows_n: usize,
    vr: usize,
    vc: usize,
) -> Option<GridEditTarget> {
    if vr < header_rows_n {
        if let Some(row) = model.header_rows.get(vr) {
            let mut ci = 0;
            for cell in row {
                let cs = cell.colspan.max(1);
                if vc >= ci && vc < ci + cs {
                    let lower = cell.text.to_lowercase();
                    if lower.contains("subtotal") {
                        return Some(GridEditTarget::ColumnSubtotal);
                    }
                    if cell.text.eq_ignore_ascii_case("total") || lower.contains("total") {
                        return Some(GridEditTarget::ColumnTotal);
                    }
                    break;
                }
                ci += cs;
            }
        }
        return None;
    }
    let body_len = model.body_rows.len();
    if vr < header_rows_n + body_len {
        let ri = vr - header_rows_n;
        if let Some(row) = model.body_rows.get(ri) {
            if row.iter().any(|c| matches!(c.role, CellRole::Subtotal)) {
                return Some(GridEditTarget::RowSubtotal);
            }
        }
        return None;
    }
    let fi = vr - (header_rows_n + body_len);
    if let Some(band) = model.footer_bands.get(fi) {
        if matches!(band.kind, pivot_core::model::FooterBandKind::GrandTotal)
            || band.cells.iter().any(|c| matches!(c.role, CellRole::GrandTotal))
        {
            return Some(GridEditTarget::GrandTotal);
        }
        return Some(GridEditTarget::FooterBand(fi));
    }
    None
}

fn start_edit(state: &mut GridUiState, target: GridEditTarget, app: &PivotApp) {
    if let Some(ref edit) = state.active_edit {
        if edit.target == target {
            return;
        }
    }
    let (current, auto_hint) = match target {
        GridEditTarget::GrandTotal => {
            let cur = app.active_spec().subtotals.grand_total_label.clone();
            let auto = FooterBandSpec::grand_total().effective_label_lang(app.active_spec().output_language);
            (cur, auto)
        }
        GridEditTarget::RowSubtotal => {
            let cur = app.active_spec().subtotals.row_subtotal_label.clone();
            let auto = match app.active_spec().output_language {
                pivot_core::OutputLanguage::En | pivot_core::OutputLanguage::IdEn => "{prefix} Subtotal".to_string(),
                pivot_core::OutputLanguage::Id => "Subtotal {prefix}".to_string(),
            };
            (cur, auto)
        }
        GridEditTarget::ColumnTotal => {
            let cur = app.active_spec().subtotals.column_total_label.clone();
            (cur, "Total".to_string())
        }
        GridEditTarget::ColumnSubtotal => {
            let cur = app.active_spec().subtotals.column_subtotal_label.clone();
            (cur, "{prefix} Subtotal".to_string())
        }
        GridEditTarget::FooterBand(idx) => {
            let bands = app.active_spec().effective_footer_bands();
            let cur = bands.get(idx).and_then(|b| b.label.clone());
            let auto = bands
                .get(idx)
                .map(|b| b.effective_label_lang(app.active_spec().output_language))
                .unwrap_or_else(|| "Footer".to_string());
            (cur, auto)
        }
    };
    let initial_text = current.unwrap_or(auto_hint.clone());
    state.active_edit = Some(GridEditSession {
        target,
        text: initial_text,
        original: auto_hint,
        initial_focus_done: false,
    });
}

pub fn show(
    app: &mut PivotApp,
    ui: &mut Ui,
    model: &ThemedTableModel,
    t: &Tokens,
) {
    let brightness = app.brightness;
    let color_theme = app.table_color_theme;
    let border_theme = app.table_border_theme;
    let style = TableStyle::resolve(color_theme, border_theme, brightness, t);
    let grid_id = ui.id().with("pivot_grid_state");
    let mut state = GridUiState::load(ui.ctx(), grid_id);
    ensure_widths(&mut state, model);

    let header_rows_n = model.header_rows.len();
    let sel_target = state.selection().and_then(|(r0, c0, _, _)| target_for_cell(model, header_rows_n, r0, c0));
    let mut trigger_edit = None;

    ui.horizontal(|ui| {
        if let Some(msg) = &state.copy_status {
            ui.label(egui::RichText::new(msg).color(t.text_muted).size(11.0));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Clear").on_hover_text("Clear cell selection").clicked() {
                state.clear_selection();
            }
            if ui.small_button("Copy all").on_hover_text("Copy whole table as TSV").clicked() {
                copy_all(ui.ctx(), model);
                state.copy_status = Some("Table copied as TSV.".to_string());
            }
            let has_sel = state.selection().is_some();
            if ui.add_enabled(has_sel, egui::Button::new("Copy").small()).on_hover_text("Copy selection as TSV").clicked() {
                copy_selection(ui.ctx(), model, &state);
                state.copy_status = Some("Selection copied as TSV.".to_string());
            }
            if let Some(ref target) = sel_target {
                let btn_text = match target {
                    GridEditTarget::GrandTotal => "✎ Edit Grand Total",
                    GridEditTarget::RowSubtotal => "✎ Edit Subtotal",
                    GridEditTarget::ColumnTotal => "✎ Edit Column Total",
                    GridEditTarget::ColumnSubtotal => "✎ Edit Col Subtotal",
                    GridEditTarget::FooterBand(_) => "✎ Edit Footer Band",
                };
                if ui.small_button(btn_text).on_hover_text("Edit this label directly").clicked() {
                    trigger_edit = Some(target.clone());
                }
            }
        });
    });
    if let Some(target) = trigger_edit {
        start_edit(&mut state, target, app);
    }

    let mut apply_edit: Option<(GridEditTarget, Option<String>)> = None;
    let mut cancel_edit = false;

    if let Some(ref mut edit) = state.active_edit {
        ui.add_space(2.0);
        egui::Frame::NONE
            .fill(t.badge_bg)
            .stroke(Stroke::new(1.0_f32, t.accent_cyan))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let target_name = match edit.target {
                        GridEditTarget::GrandTotal => "Grand Total label",
                        GridEditTarget::RowSubtotal => "Row Subtotal template",
                        GridEditTarget::ColumnTotal => "Column Total label",
                        GridEditTarget::ColumnSubtotal => "Column Subtotal template",
                        GridEditTarget::FooterBand(_) => "Footer Band label",
                    };
                    ui.label(egui::RichText::new(format!("✎ {target_name}:")).strong().color(t.accent_cyan));
                    let edit_id = grid_id.with("active_edit_input");
                    let text_resp = ui.add(
                        egui::TextEdit::singleline(&mut edit.text)
                            .hint_text(&edit.original)
                            .id(edit_id)
                            .desired_width(220.0),
                    );
                    if !edit.initial_focus_done {
                        text_resp.request_focus();
                        edit.initial_focus_done = true;
                    }
                    let enter_pressed = text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Apply").clicked() || enter_pressed {
                        let text = edit.text.trim().to_string();
                        apply_edit = Some((edit.target.clone(), if text.is_empty() { None } else { Some(text) }));
                    }
                    if ui.button("Reset to Auto").clicked() {
                        apply_edit = Some((edit.target.clone(), None));
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel_edit = true;
                    }
                });
            });
        ui.add_space(2.0);
    }

    if let Some((target, val)) = apply_edit {
        match target {
            GridEditTarget::GrandTotal => app.set_grand_total_label(val),
            GridEditTarget::RowSubtotal => app.set_row_subtotal_label(val),
            GridEditTarget::ColumnTotal => app.set_column_total_label(val),
            GridEditTarget::ColumnSubtotal => app.set_column_subtotal_label(val),
            GridEditTarget::FooterBand(idx) => app.set_footer_band_label(idx, val),
        }
        state.active_edit = None;
    }
    if cancel_edit {
        state.active_edit = None;
    }

    ui.add_space(4.0);

    let col_count = state.col_widths.len();
    let total_w: f32 = state.col_widths.iter().sum();
    let row_h = style.row_height;
    let header_h_unit = style.header_height;
    let header_h = header_rows_n as f32 * header_h_unit;
    let footer_rows_n = model.footer_bands.len();
    let body_h = model.body_rows.len() as f32 * row_h;
    let footer_h = footer_rows_n as f32 * row_h;
    let content_h = body_h + footer_h;

    let avail = ui.available_size();
    let outer_h = (avail.y - 2.0).max(120.0);
    let body_view_h = (outer_h - header_h).max(60.0);

    // Body scroll area (both directions)
    let body_id = ui.id().with("pivot_body_scroll");
    let body_scroll = egui::ScrollArea::both()
        .id_salt(body_id)
        .auto_shrink([false, false])
        .min_scrolled_height(body_view_h)
        .max_height(body_view_h);
    let mut x_offset: f32 = ui.ctx().data(|d| d.get_temp::<f32>(grid_id.with("xoff")).unwrap_or(0.0));

    // Header strip
    let (header_rect, header_resp) = ui.allocate_exact_size(Vec2::new(avail.x.max(200.0), header_h), Sense::click());
    if header_resp.double_clicked() {
        if let Some(pos) = header_resp.interact_pointer_pos() {
            let local_x = pos.x - header_rect.min.x + x_offset;
            let mut x = 0.0;
            let mut ci = state.col_widths.len().saturating_sub(1);
            for (i, w) in state.col_widths.iter().enumerate() {
                if local_x < x + *w { ci = i; break; }
                x += *w;
            }
            let local_y = (pos.y - header_rect.min.y).max(0.0);
            let ri = ((local_y / header_h_unit) as usize).min(header_rows_n.saturating_sub(1));
            if let Some(target) = target_for_cell(model, header_rows_n, ri, ci) {
                start_edit(&mut state, target, app);
            }
        }
    }
    paint_header(ui, model, &style, &state.col_widths, header_rect, x_offset, header_h_unit);
    // Resize handles in header
    handle_resize(ui, &mut state.col_widths, header_rect, x_offset, col_count);

    let mut double_click_target = None;
    let body_out = body_scroll.show(ui, |ui| {
        let (content_rect, response) = ui.allocate_exact_size(Vec2::new(total_w.max(100.0), content_h.max(row_h)), Sense::click_and_drag());
        let painter = ui.painter_at(content_rect.intersect(ui.clip_rect()));
        paint_body_and_footer(&painter, model, &style, &state, content_rect, header_rows_n, row_h);
        paint_selection_outline(&painter, &state, content_rect, header_rows_n, row_h, &style, model);
        double_click_target = handle_selection(ui.ctx(), &response, model, &mut state, content_rect, header_rows_n, row_h);
        response
    });
    if let Some(target) = double_click_target {
        start_edit(&mut state, target, app);
    }

    // Right-click menu on the grid body
    let mut context_edit_target = None;
    body_out.inner.context_menu(|ui| {
        let has_sel = state.selection().is_some();
        let sel_target = state.selection().and_then(|(r0, c0, _, _)| target_for_cell(model, header_rows_n, r0, c0));
        if let Some(ref target) = sel_target {
            let label = match target {
                GridEditTarget::GrandTotal => "✎ Edit Grand Total label...",
                GridEditTarget::RowSubtotal => "✎ Edit Subtotal template...",
                GridEditTarget::ColumnTotal => "✎ Edit Column Total label...",
                GridEditTarget::ColumnSubtotal => "✎ Edit Column Subtotal template...",
                GridEditTarget::FooterBand(_) => "✎ Edit Footer Band label...",
            };
            if ui.button(label).clicked() {
                context_edit_target = Some(target.clone());
                ui.close_menu();
            }
            ui.separator();
        }
        if ui.add_enabled(has_sel, egui::Button::new("Copy selection (TSV)")).clicked() {
            copy_selection(ui.ctx(), model, &state);
            state.copy_status = Some("Selection copied as TSV.".to_string());
            ui.close_menu();
        }
        if ui.button("Copy all (TSV)").clicked() {
            copy_all(ui.ctx(), model);
            state.copy_status = Some("Table copied as TSV.".to_string());
            ui.close_menu();
        }
        if ui.add_enabled(has_sel, egui::Button::new("Clear selection")).clicked() {
            state.clear_selection();
            ui.close_menu();
        }
    });
    if let Some(target) = context_edit_target {
        start_edit(&mut state, target, app);
    }
    // capture horizontal offset (egui 0.31: state.offset)
    x_offset = body_out.state.offset.x;
    ui.ctx().data_mut(|d| d.insert_temp(grid_id.with("xoff"), x_offset));

    // handle selection clicks via a separate interaction pass on the body response area
    // (done inside allocate above; reload to persist)
    // Keyboard copy
    let copy_requested = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::C));
    if copy_requested && state.selection().is_some() {
        copy_selection(ui.ctx(), model, &state);
        state.copy_status = Some("Selection copied as TSV.".to_string());
    }

    state.store(ui.ctx(), grid_id);
    ui.ctx().request_repaint(); // keep resize/offset in sync feeling responsive
}

/// Accent outline around the selection range: reads as "selection" on top of any fill.
fn paint_selection_outline(painter: &egui::Painter, state: &GridUiState, rect: Rect, header_rows_n: usize, row_h: f32, style: &TableStyle, model: &ThemedTableModel) {
    let Some((r0, c0, r1, c1)) = state.selection() else { return };
    if r1 < header_rows_n { return; }
    let total = model.body_rows.len() + model.footer_bands.len();
    let r0b = (r0.max(header_rows_n) - header_rows_n).min(total.saturating_sub(1));
    let r1b = (r1 - header_rows_n).min(total.saturating_sub(1));
    if r0b > r1b { return; }
    let x0 = rect.min.x + col_x(&state.col_widths, c0.min(state.col_widths.len().saturating_sub(1)));
    let x1 = rect.min.x + col_x(&state.col_widths, (c1 + 1).min(state.col_widths.len()));
    let y0 = rect.min.y + r0b as f32 * row_h;
    let y1 = rect.min.y + (r1b + 1) as f32 * row_h;
    let sel_rect = Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1));
    painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.25_f32, style.tokens.accent_strong), StrokeKind::Inside);
}

fn paint_cell_border(painter: &egui::Painter, rect: Rect, rs: &pivot_theme::table::RoleStyle) {
    match rs.border_mode {
        CellBorderMode::All => {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, rs.grid_line), StrokeKind::Inside);
        }
        CellBorderMode::HorizontalOnly => {
            let s = Stroke::new(1.0_f32, rs.grid_line);
            painter.line_segment([rect.left_top(), rect.right_top()], s);
            painter.line_segment([rect.left_bottom(), rect.right_bottom()], s);
        }
        CellBorderMode::None => {}
    }
}
fn paint_header(ui: &Ui, model: &ThemedTableModel, style: &TableStyle, col_widths: &[f32], rect: Rect, x_offset: f32, header_h_unit: f32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, style.col_header.fill);
    let rows = model.header_rows.len();
    let cols = col_widths.len();
    let mut occupied = vec![vec![false; cols]; rows];
    for (ri, hrow) in model.header_rows.iter().enumerate() {
        let mut ci = 0usize;
        for cell in hrow {
            while ci < cols && occupied[ri][ci] { ci += 1; }
            if ci >= cols { break; }
            let cs = cell.colspan.max(1).min(cols.saturating_sub(ci));
            let rs = cell.rowspan.max(1).min(rows.saturating_sub(ri));
            for row_occ in occupied.iter_mut().take((ri + rs).min(rows)).skip(ri) {
                for occ in row_occ.iter_mut().take((ci + cs).min(cols)).skip(ci) {
                    *occ = true;
                }
            }
            let x0 = rect.min.x + col_x(col_widths, ci) - x_offset;
            let w: f32 = col_widths[ci..ci + cs].iter().sum();
            let y0 = rect.min.y + ri as f32 * header_h_unit;
            let h = rs as f32 * header_h_unit;
            let cell_rect = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(w, h));
            let vis = cell_rect.intersect(rect);
            if vis.is_positive() {
                let rs_style = role_style_for(style, cell.role, false);
                painter.rect_filled(cell_rect, 0.0, rs_style.fill);
                paint_cell_border(&painter, cell_rect, &rs_style);
                let text_color = rs_style.text;
                let galley = painter.layout_no_wrap(truncate(&cell.text, 60), FontId::proportional(13.0), text_color);
                let tx = (cell_rect.min.x + PAD_X).min(cell_rect.max.x - 4.0);
                let ty = cell_rect.center().y - galley.size().y * 0.5;
                painter.galley(Pos2::new(tx, ty), galley, text_color);
            }
            ci += cs;
        }
    }
    if style.outer_border {
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, style.border), StrokeKind::Inside);
    }
}

fn handle_resize(ui: &Ui, col_widths: &mut [f32], header_rect: Rect, x_offset: f32, col_count: usize) {
    let mut x = header_rect.min.x - x_offset;
    for (i, width) in col_widths.iter_mut().enumerate().take(col_count) {
        x += *width;
        let hit = Rect::from_center_size(
            Pos2::new(x, header_rect.center().y),
            Vec2::new(RESIZE_HIT * 2.0, header_rect.height()),
        );
        let id = ui.id().with(("col_resize", i));
        let resp = ui.interact(hit, id, Sense::drag());
        if resp.dragged() {
            let delta = resp.drag_delta().x;
            *width = (*width + delta).max(MIN_COL_W);
        }
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }
}

fn paint_body_and_footer(painter: &egui::Painter, model: &ThemedTableModel, style: &TableStyle, state: &GridUiState, rect: Rect, header_rows_n: usize, row_h: f32) {
    // Selection paints an opaque composite (selection over role fill) and re-derives
    // the text color from it, so selected cells stay readable in every theme.
    let sel_colors = |rs: &pivot_theme::table::RoleStyle| -> (egui::Color32, egui::Color32) {
        let fill = pivot_theme::color::over(style.selection, rs.fill);
        (fill, pivot_theme::color::text_on(fill, &style.tokens))
    };
    let sel = state.selection();
    // body
    for (ri, brow) in model.body_rows.iter().enumerate() {
        let y0 = rect.min.y + ri as f32 * row_h;
        let mut ci = 0usize;
        let zebra = ri % 2 == 1;
        for cell in brow {
            let cs = cell.colspan.max(1);
            let x0 = rect.min.x + col_x(&state.col_widths, ci);
            let w: f32 = state.col_widths.iter().skip(ci).take(cs).sum();
            let cell_rect = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(w, row_h));
            let vis_row = header_rows_n + ri;
            let selected = sel.map(|(r0, c0, r1, c1)| vis_row >= r0 && vis_row <= r1 && ci >= c0 && ci < c1 + 1).unwrap_or(false);
            let rs = role_style_for(style, cell.role, zebra);
            let (bg, text) = if selected { sel_colors(&rs) } else { (rs.fill, rs.text) };
            painter.rect_filled(cell_rect, 0.0, bg);
            paint_cell_border(painter, cell_rect, &rs);
            if matches!(cell.role, CellRole::GrandTotal) {
                // Strong rule above the grand total band separates it from body rows.
                painter.line_segment([cell_rect.left_top(), cell_rect.right_top()], Stroke::new(1.5_f32, style.border));
            }
            let indent = if matches!(cell.role, CellRole::RowHeader) { cell.indent as f32 * 12.0 } else { 0.0 };
            let (font, right) = cell_font(&cell.text, 13.0);
            let galley = painter.layout_no_wrap(truncate(&cell.text, 80), font, text);
            let tx = if right { (cell_rect.max.x - PAD_X) - galley.size().x } else { (cell_rect.min.x + PAD_X + indent).min(cell_rect.max.x - 4.0) };
            let ty = cell_rect.center().y - galley.size().y * 0.5;
            painter.galley(Pos2::new(tx, ty), galley, text);
            ci += cs;
        }
    }
    // footers
    for (fi, band) in model.footer_bands.iter().enumerate() {
        let y0 = rect.min.y + model.body_rows.len() as f32 * row_h + fi as f32 * row_h;
        let mut ci = 0usize;
        let total_w: f32 = state.col_widths.iter().sum();
        if band.cells.len() == 1 && band.cells[0].colspan <= 1 && state.col_widths.len() > 1 {
            let cell = &band.cells[0];
            let cell_rect = Rect::from_min_size(Pos2::new(rect.min.x, y0), Vec2::new(total_w, row_h));
            let vis_row = header_rows_n + model.body_rows.len() + fi;
            let selected = sel.map(|(r0, _, r1, _)| vis_row >= r0 && vis_row <= r1).unwrap_or(false);
            let rs = if band.kind == pivot_core::model::FooterBandKind::Notes { style.notes } else { role_style_for(style, cell.role, false) };
            let (bg, text) = if selected { sel_colors(&rs) } else { (rs.fill, rs.text) };
            painter.rect_filled(cell_rect, 0.0, bg);
            paint_cell_border(painter, cell_rect, &rs);
            // Notes band: quiet small text; other footer bands keep cell typography.
            let size = if band.kind == pivot_core::model::FooterBandKind::Notes { 11.0 } else { 13.0 };
            let (font, right) = cell_font(&cell.text, size);
            let galley = painter.layout_no_wrap(truncate(&cell.text, 160), font, text);
            let tx = if right { (cell_rect.max.x - PAD_X) - galley.size().x } else { cell_rect.min.x + PAD_X };
            painter.galley(Pos2::new(tx, cell_rect.center().y - galley.size().y * 0.5), galley, text);
        } else {
            for cell in &band.cells {
                let cs = cell.colspan.max(1);
                let x0 = rect.min.x + col_x(&state.col_widths, ci);
                let w: f32 = state.col_widths.iter().skip(ci).take(cs).sum::<f32>().max(*state.col_widths.get(ci).unwrap_or(&110.0));
                let cell_rect = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(w, row_h));
                let vis_row = header_rows_n + model.body_rows.len() + fi;
                let selected = sel.map(|(r0, c0, r1, c1)| vis_row >= r0 && vis_row <= r1 && ci >= c0 && ci < c1 + 1).unwrap_or(false);
                let rs = role_style_for(style, cell.role, false);
                let (bg, text) = if selected { sel_colors(&rs) } else { (rs.fill, rs.text) };
                painter.rect_filled(cell_rect, 0.0, bg);
                paint_cell_border(painter, cell_rect, &rs);
                if matches!(cell.role, CellRole::GrandTotal) {
                    painter.line_segment([cell_rect.left_top(), cell_rect.right_top()], Stroke::new(1.5_f32, style.border));
                }
                let (font, right) = cell_font(&cell.text, 13.0);
                let galley = painter.layout_no_wrap(truncate(&cell.text, 80), font, text);
                let tx = if right { (cell_rect.max.x - PAD_X) - galley.size().x } else { cell_rect.min.x + PAD_X };
                painter.galley(Pos2::new(tx, cell_rect.center().y - galley.size().y * 0.5), galley, text);
                ci += cs;
            }
        }
    }
}

fn handle_selection(
    ctx: &egui::Context,
    response: &egui::Response,
    model: &ThemedTableModel,
    state: &mut GridUiState,
    rect: Rect,
    header_rows_n: usize,
    row_h: f32,
) -> Option<GridEditTarget> {
    let mut edit_target = None;
    if response.double_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((vr, vc)) = pos_to_cell(pos, rect, &state.col_widths, header_rows_n, model, row_h) {
                edit_target = target_for_cell(model, header_rows_n, vr, vc);
            }
        }
    }
    if response.clicked() || response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((vr, vc)) = pos_to_cell(pos, rect, &state.col_widths, header_rows_n, model, row_h) {
                state.sel_anchor = Some((vr, vc));
                state.sel_focus = Some((vr, vc));
                state.copy_status = None;
            }
        }
    }
    if response.dragged() {
        if let Some(pos) = ctx.pointer_interact_pos() {
            if let Some((vr, vc)) = pos_to_cell(pos, rect, &state.col_widths, header_rows_n, model, row_h) {
                state.sel_focus = Some((vr, vc));
            }
        }
    }
    edit_target
}

fn pos_to_cell(pos: Pos2, rect: Rect, col_widths: &[f32], header_rows_n: usize, model: &ThemedTableModel, row_h: f32) -> Option<(usize, usize)> {
    if !rect.contains(pos) && pos.y < rect.min.y { return None; }
    let local_y = (pos.y - rect.min.y).max(0.0);
    let ri = (local_y / row_h).max(0.0) as usize;
    let total_rows = model.body_rows.len() + model.footer_bands.len();
    if ri >= total_rows { return None; }
    let local_x = (pos.x - rect.min.x).max(0.0);
    let mut x = 0.0;
    let mut ci = col_widths.len().saturating_sub(1);
    for (i, w) in col_widths.iter().enumerate() {
        if local_x < x + *w { ci = i; break; }
        x += *w;
    }
    Some((header_rows_n + ri, ci))
}

fn col_x(col_widths: &[f32], idx: usize) -> f32 {
    col_widths.iter().take(idx).sum()
}

fn role_style_for(style: &TableStyle, role: CellRole, zebra: bool) -> pivot_theme::table::RoleStyle {
    match role {
        CellRole::StubHead => style.stub_head,
        CellRole::ColHeader => style.col_header,
        CellRole::RowHeader => style.row_header,
        CellRole::Body => if style.zebra && zebra { style.body_zebra } else { style.body },
        CellRole::Subtotal => style.subtotal,
        CellRole::GrandTotal => style.grand_total,
        CellRole::Footer | CellRole::Title => style.footer,
    }
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max { out.push('�'); break; }
        out.push(ch);
    }
    out
}

fn flattened_rows(model: &ThemedTableModel) -> Vec<Vec<String>> {
    let cols = model.total_columns().max(1);
    let mut out = Vec::new();
    // headers (best-effort expansion of spans)
    for hrow in &model.header_rows {
        let mut row = vec![String::new(); cols];
        let mut ci = 0usize;
        for cell in hrow {
            while ci < cols && !row[ci].is_empty() { ci += 1; }
            if ci < cols { row[ci] = cell.text.clone(); }
            ci += cell.colspan.max(1);
        }
        out.push(row);
    }
    for brow in &model.body_rows {
        let mut row = Vec::with_capacity(cols);
        for cell in brow { row.push(cell.text.clone()); }
        while row.len() < cols { row.push(String::new()); }
        out.push(row);
    }
    for band in &model.footer_bands {
        let mut row = vec![String::new(); cols];
        if band.cells.len() == 1 && cols > 1 {
            row[0] = band.cells[0].text.clone();
        } else {
            let mut ci = 0usize;
            for cell in &band.cells {
                if ci < cols { row[ci] = cell.text.clone(); }
                ci += cell.colspan.max(1);
            }
        }
        out.push(row);
    }
    out
}

fn to_tsv(rows: &[Vec<String>]) -> String {
    rows.iter().map(|r| r.iter().map(|c| c.replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n")
}

fn copy_all(ctx: &egui::Context, model: &ThemedTableModel) {
    let rows = flattened_rows(model);
    ctx.copy_text(to_tsv(&rows));
}

fn copy_selection(ctx: &egui::Context, model: &ThemedTableModel, state: &GridUiState) {
    let Some((r0, c0, r1, c1)) = state.selection() else { return; };
    let rows = flattened_rows(model);
    let mut out = Vec::new();
    for r in r0..=r1.min(rows.len().saturating_sub(1)) {
        if let Some(row) = rows.get(r) {
            let end = c1.min(row.len().saturating_sub(1));
            let line = row[c0..=end].to_vec();
            out.push(line);
        }
    }
    ctx.copy_text(to_tsv(&out));
}
