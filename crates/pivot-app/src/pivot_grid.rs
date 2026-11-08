//! Custom-painted pivot grid (AC2.5): sticky headers, column resize, selection+copy (TSV),
//! and colspan/rowspan rendering for `ThemedTableModel`.

use egui::{Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use pivot_core::model::{CellRole, ThemedTableModel};
use pivot_theme::Tokens;

const ROW_H: f32 = 28.0;
const HEADER_H: f32 = 30.0;
const PAD_X: f32 = 8.0;
const MIN_COL_W: f32 = 50.0;
const RESIZE_HIT: f32 = 5.0;

#[derive(Clone, Default)]
pub struct GridUiState {
    pub col_widths: Vec<f32>,
    pub model_key: u64,
    /// (visual_row, col) in the flattened table (header rows first, then body, then footer rows).
    pub sel_anchor: Option<(usize, usize)>,
    pub sel_focus: Option<(usize, usize)>,
    pub copy_status: Option<String>,
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
    for i in 0..model.row_header_cols.min(n) {
        w[i] = 140.0;
    }
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

pub fn show(ui: &mut Ui, model: &ThemedTableModel, t: &Tokens) {
    let grid_id = ui.id().with("pivot_grid_state");
    let mut state = GridUiState::load(ui.ctx(), grid_id);
    ensure_widths(&mut state, model);

    ui.horizontal(|ui| {
        if ui.small_button("Copy selection (TSV)").clicked() {
            copy_selection(ui.ctx(), model, &state);
            state.copy_status = Some("Selection copied as TSV.".to_string());
        }
        if ui.small_button("Copy all (TSV)").clicked() {
            copy_all(ui.ctx(), model);
            state.copy_status = Some("Table copied as TSV.".to_string());
        }
        if ui.small_button("Clear selection").clicked() {
            state.clear_selection();
        }
        if let Some(msg) = &state.copy_status {
            ui.label(egui::RichText::new(msg).color(t.text_muted).size(11.0));
        }
    });
    ui.add_space(4.0);

    let col_count = state.col_widths.len();
    let total_w: f32 = state.col_widths.iter().sum();
    let header_rows_n = model.header_rows.len();
    let header_h = header_rows_n as f32 * HEADER_H;
    let footer_rows_n = model.footer_bands.len();
    let body_h = model.body_rows.len() as f32 * ROW_H;
    let footer_h = footer_rows_n as f32 * ROW_H;
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

    // Header strip (horizontal synced by reading body offset next frame is imperfect; we read stored offset)
    let (header_rect, _) = ui.allocate_exact_size(Vec2::new(avail.x.max(200.0), header_h), Sense::hover());
    paint_header(ui, model, t, &state.col_widths, header_rect, x_offset);
    // Resize handles in header
    handle_resize(ui, &mut state.col_widths, header_rect, x_offset, col_count);

    let body_out = body_scroll.show(ui, |ui| {
        let (content_rect, response) = ui.allocate_exact_size(Vec2::new(total_w.max(100.0), content_h.max(ROW_H)), Sense::click_and_drag());
        let painter = ui.painter_at(content_rect.intersect(ui.clip_rect()));
        paint_body_and_footer(&painter, model, t, &state, content_rect, header_rows_n);
        handle_selection(ui.ctx(), &response, model, &mut state, content_rect, header_rows_n);
    });
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

fn paint_header(ui: &Ui, model: &ThemedTableModel, t: &Tokens, col_widths: &[f32], rect: Rect, x_offset: f32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, t.grid_header);
    // Build occupancy for rowspan/colspan in header
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
            for r in ri..ri + rs {
                for c in ci..ci + cs {
                    if r < rows && c < cols { occupied[r][c] = true; }
                }
            }
            let x0 = rect.min.x + col_x(col_widths, ci) - x_offset;
            let w: f32 = col_widths[ci..ci + cs].iter().sum();
            let y0 = rect.min.y + ri as f32 * HEADER_H;
            let h = rs as f32 * HEADER_H;
            let cell_rect = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(w, h));
            let vis = cell_rect.intersect(rect);
            if vis.is_positive() {
                let bg = role_bg(cell.role, t, true);
                painter.rect_filled(cell_rect, 0.0, bg);
                painter.rect_stroke(cell_rect, 0.0, Stroke::new(1.0_f32, t.grid_line), StrokeKind::Inside);
                let text_color = role_fg(cell.role, t);
                let galley = painter.layout_no_wrap(truncate(&cell.text, 60), FontId::proportional(13.0), text_color);
                let tx = (cell_rect.min.x + PAD_X).min(cell_rect.max.x - 4.0);
                let ty = cell_rect.center().y - galley.size().y * 0.5;
                painter.galley(Pos2::new(tx, ty), galley, text_color);
            }
            ci += cs;
        }
    }
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
}

fn handle_resize(ui: &Ui, col_widths: &mut [f32], header_rect: Rect, x_offset: f32, col_count: usize) {
    let mut x = header_rect.min.x - x_offset;
    for i in 0..col_count {
        x += col_widths[i];
        let hit = Rect::from_center_size(Pos2::new(x, header_rect.center().y), Vec2::new(RESIZE_HIT * 2.0, header_rect.height()));
        let id = ui.id().with(("col_resize", i));
        let resp = ui.interact(hit, id, Sense::drag());
        if resp.dragged() {
            let delta = resp.drag_delta().x;
            col_widths[i] = (col_widths[i] + delta).max(MIN_COL_W);
        }
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }
}

fn paint_body_and_footer(painter: &egui::Painter, model: &ThemedTableModel, t: &Tokens, state: &GridUiState, rect: Rect, header_rows_n: usize) {
    let sel = state.selection();
    // body
    for (ri, brow) in model.body_rows.iter().enumerate() {
        let y0 = rect.min.y + ri as f32 * ROW_H;
        let _row_rect = Rect::from_min_size(Pos2::new(rect.min.x, y0), Vec2::new(rect.width(), ROW_H));
        let mut ci = 0usize;
        for cell in brow {
            let cs = cell.colspan.max(1);
            let x0 = rect.min.x + col_x(&state.col_widths, ci);
            let w: f32 = state.col_widths.iter().skip(ci).take(cs).sum();
            let cell_rect = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(w, ROW_H));
            let vis_row = header_rows_n + ri;
            let selected = sel.map(|(r0, c0, r1, c1)| vis_row >= r0 && vis_row <= r1 && ci >= c0 && ci < c1 + 1).unwrap_or(false);
            let bg = if selected { t.selection.gamma_multiply(0.35) } else { role_bg(cell.role, t, false) };
            painter.rect_filled(cell_rect, 0.0, bg);
            painter.rect_stroke(cell_rect, 0.0, Stroke::new(1.0_f32, t.grid_line), StrokeKind::Inside);
            let indent = if matches!(cell.role, CellRole::RowHeader) { cell.indent as f32 * 12.0 } else { 0.0 };
            let text_color = role_fg(cell.role, t);
            let font = if matches!(cell.role, CellRole::RowHeader | CellRole::Subtotal | CellRole::GrandTotal) {
                FontId::proportional(13.0)
            } else {
                FontId::proportional(13.0)
            };
            let galley = painter.layout_no_wrap(truncate(&cell.text, 80), font, text_color);
            let tx = (cell_rect.min.x + PAD_X + indent).min(cell_rect.max.x - 4.0);
            let ty = cell_rect.center().y - galley.size().y * 0.5;
            painter.galley(Pos2::new(tx, ty), galley, text_color);
            ci += cs;
        }
    }
    // footers
    let mut fi = 0usize;
    for band in &model.footer_bands {
        let y0 = rect.min.y + model.body_rows.len() as f32 * ROW_H + fi as f32 * ROW_H;
        let mut ci = 0usize;
        let total_w: f32 = state.col_widths.iter().sum();
        if band.cells.len() == 1 && band.cells[0].colspan <= 1 && state.col_widths.len() > 1 {
            // Notes-style single spanning cell
            let cell = &band.cells[0];
            let cell_rect = Rect::from_min_size(Pos2::new(rect.min.x, y0), Vec2::new(total_w, ROW_H));
            let vis_row = header_rows_n + model.body_rows.len() + fi;
            let selected = sel.map(|(r0, _, r1, _)| vis_row >= r0 && vis_row <= r1).unwrap_or(false);
            let bg = if selected { t.selection.gamma_multiply(0.35) } else { role_bg(cell.role, t, false) };
            painter.rect_filled(cell_rect, 0.0, bg);
            painter.rect_stroke(cell_rect, 0.0, Stroke::new(1.0_f32, t.grid_line), StrokeKind::Inside);
            let galley = painter.layout_no_wrap(truncate(&cell.text, 160), FontId::proportional(12.0), t.text_muted);
            painter.galley(Pos2::new(cell_rect.min.x + PAD_X, cell_rect.center().y - galley.size().y * 0.5), galley, t.text_muted);
        } else {
            for cell in &band.cells {
                let cs = cell.colspan.max(1);
                let x0 = rect.min.x + col_x(&state.col_widths, ci);
                let w: f32 = state.col_widths.iter().skip(ci).take(cs).sum::<f32>().max(*state.col_widths.get(ci).unwrap_or(&110.0));
                let cell_rect = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(w, ROW_H));
                let vis_row = header_rows_n + model.body_rows.len() + fi;
                let selected = sel.map(|(r0, c0, r1, c1)| vis_row >= r0 && vis_row <= r1 && ci >= c0 && ci < c1 + 1).unwrap_or(false);
                let bg = if selected { t.selection.gamma_multiply(0.35) } else { role_bg(cell.role, t, false) };
                painter.rect_filled(cell_rect, 0.0, bg);
                painter.rect_stroke(cell_rect, 0.0, Stroke::new(1.0_f32, t.grid_line), StrokeKind::Inside);
                let galley = painter.layout_no_wrap(truncate(&cell.text, 80), FontId::proportional(13.0), role_fg(cell.role, t));
                painter.galley(Pos2::new(cell_rect.min.x + PAD_X, cell_rect.center().y - galley.size().y * 0.5), galley, role_fg(cell.role, t));
                ci += cs;
            }
        }
        fi += 1;
    }
}

fn handle_selection(ctx: &egui::Context, response: &egui::Response, model: &ThemedTableModel, state: &mut GridUiState, rect: Rect, header_rows_n: usize) {
    if response.clicked() || response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((vr, vc)) = pos_to_cell(pos, rect, &state.col_widths, header_rows_n, model) {
                state.sel_anchor = Some((vr, vc));
                state.sel_focus = Some((vr, vc));
                state.copy_status = None;
            }
        }
    }
    if response.dragged() {
        if let Some(pos) = ctx.pointer_interact_pos() {
            if let Some((vr, vc)) = pos_to_cell(pos, rect, &state.col_widths, header_rows_n, model) {
                state.sel_focus = Some((vr, vc));
            }
        }
    }
}

fn pos_to_cell(pos: Pos2, rect: Rect, col_widths: &[f32], header_rows_n: usize, model: &ThemedTableModel) -> Option<(usize, usize)> {
    if !rect.contains(pos) && pos.y < rect.min.y { return None; }
    let local_y = (pos.y - rect.min.y).max(0.0);
    let ri = (local_y / ROW_H) as usize;
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

fn role_bg(role: CellRole, t: &Tokens, header: bool) -> Color32 {
    match role {
        CellRole::StubHead | CellRole::ColHeader => t.grid_header,
        CellRole::RowHeader => t.panel_alt,
        CellRole::Subtotal => t.grid_header,
        CellRole::GrandTotal | CellRole::Footer => t.grid_header,
        CellRole::Body => if header { t.grid_header } else { t.grid_body },
        CellRole::Title => t.panel,
    }
}

fn role_fg(role: CellRole, t: &Tokens) -> Color32 {
    match role {
        CellRole::Body => t.text,
        CellRole::Footer => t.text_muted,
        _ => t.text_strong,
    }
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max { out.push('…'); break; }
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
    rows.iter().map(|r| r.iter().map(|c| c.replace('\t', " ").replace('\n', " ")).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n")
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
            let mut line = Vec::new();
            for c in c0..=c1.min(row.len().saturating_sub(1)) {
                line.push(row[c].clone());
            }
            out.push(line);
        }
    }
    ctx.copy_text(to_tsv(&out));
}
