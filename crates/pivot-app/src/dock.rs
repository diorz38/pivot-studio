//! Docked, resizable panel layout (DesignCraft docking feel, Phase 2).
//! Home page: open/import/connect. Worksheet: field wells, pivot grid (ThemedTableModel), data preview.

use crate::app::PivotApp;
use egui::{RichText, Sense, Vec2};
use pivot_core::{Aggregate, CellRole, DataType, ThemedTableModel};
use pivot_theme::Tokens;
use serde_json::Value;

pub fn show(app: &mut PivotApp, ui: &mut egui::Ui) {
    if app.on_home {
        home_page(app, ui);
    } else {
        worksheet(app, ui);
    }
}

fn home_page(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(28.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(860.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.label(RichText::new("Pivot Studio").strong().size(30.0).color(t.text_strong));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Open a saved project, import a file, or connect to a database to start building a pivot table.")
                            .size(14.0)
                            .color(t.text_muted),
                    );
                    ui.add_space(26.0);
                    if app.loading.is_some() {
                        egui::Frame::new()
                            .fill(t.panel)
                            .stroke(egui::Stroke::new(1.0_f32, t.accent))
                            .inner_margin(egui::Margin::same(10))
                            .corner_radius(6.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    let msg = app.loading.as_ref().map(|l| l.progress.clone()).unwrap_or_else(|| "Loading...".to_string());
                                    ui.label(RichText::new(msg).color(t.text));
                                    if ui.button("Cancel").clicked() { app.cancel_load(); }
                                });
                            });
                        ui.add_space(14.0);
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                        action_card(ui, app, &t, "Open Project", "Open a saved .pivot workspace.", "file.open");
                        action_card(ui, app, &t, "Import CSV", "Load a CSV/TSV file into a worksheet.", "file.import.csv");
                        action_card(ui, app, &t, "Import Excel", "Load an .xlsx workbook (first sheet).", "file.import.xlsx");
                        action_card(ui, app, &t, "Import DBF", "Load a dBASE (.dbf) file.", "file.import.dbf");
                        action_card(ui, app, &t, "Connect Database", "Create a new SQLite or MySQL connection (Phase 5).", "data.connect");
                        if app.table.is_some() {
                            action_card(ui, app, &t, "Open Worksheet", "View data and build a pivot.", "nav.worksheet");
                        }
                    });
                    ui.add_space(24.0);
                    egui::Frame::new()
                        .fill(t.panel)
                        .stroke(egui::Stroke::new(1.0_f32, t.border))
                        .inner_margin(egui::Margin::same(12))
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Recent projects").strong().color(t.text_strong));
                            ui.add_space(6.0);
                            ui.label(RichText::new("No recent projects yet. Project history arrives in a later phase.").color(t.text_disabled));
                        });
                });
            });
        });
}

fn action_card(ui: &mut egui::Ui, app: &mut PivotApp, t: &Tokens, title: &str, detail: &str, command: &str) {
    let size = Vec2::new(250.0, 96.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_text(detail);
    let bg = if response.hovered() { t.hover } else { t.panel };
    let stroke = if response.hovered() { egui::Stroke::new(1.0_f32, t.accent) } else { egui::Stroke::new(1.0_f32, t.border) };
    ui.painter().rect(rect, 6.0, bg, stroke, egui::StrokeKind::Inside);
    let inner = rect.shrink(14.0);
    ui.painter().text(egui::pos2(inner.min.x, inner.min.y), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(15.0), t.text_strong);
    ui.painter().text(egui::pos2(inner.min.x, inner.min.y + 26.0), egui::Align2::LEFT_TOP, detail, egui::FontId::proportional(12.0), t.text_muted);
    if response.clicked() {
        let ctx = ui.ctx().clone();
        app.dispatch(command, &Value::Null, &ctx);
    }
}

fn worksheet(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());

    egui::TopBottomPanel::bottom("dock_bottom")
        .resizable(true)
        .default_height(170.0)
        .min_height(80.0)
        .frame(egui::Frame::new().fill(t.panel_alt).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(8)))
        .show_inside(ui, |ui| {
            panel_header(ui, "Data Preview / Filters", &t);
            ui.add_space(4.0);
            if let Some(table) = &app.table {
                let preview_rows = table.row_count().min(80);
                egui::ScrollArea::both().auto_shrink([false, false]).max_height(120.0).show(ui, |ui| {
                    egui::Grid::new("data_preview_grid").striped(false).min_col_width(90.0).show(ui, |ui| {
                        for col in &table.columns { header_cell(ui, &col.name, &t); }
                        ui.end_row();
                        for r in 0..preview_rows {
                            for c in 0..table.column_count() {
                                let text = table.get(r, c).map(|v| v.as_display()).unwrap_or_default();
                                body_cell(ui, &text, &t, false);
                            }
                            ui.end_row();
                        }
                    });
                });
                if table.row_count() > preview_rows {
                    ui.label(RichText::new(format!("Showing {preview_rows} of {} rows.", table.row_count())).color(t.text_muted).size(11.0));
                }
            } else {
                ui.label(RichText::new("Import data to see a preview. Filters UI arrives in a later phase.").color(t.text_muted));
            }
        });

    egui::SidePanel::left("dock_left")
        .resizable(true)
        .default_width(300.0)
        .min_width(190.0)
        .frame(egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(8)))
        .show_inside(ui, |ui| {
            panel_header(ui, "Fields", &t);
            if let Some(name) = &app.source_name {
                ui.label(RichText::new(format!("Source: {name}")).color(t.text).size(12.0));
            }
            ui.horizontal(|ui| {
                if ui.small_button("CSV").clicked() { let ctx = ui.ctx().clone(); app.dispatch("file.import.csv", &Value::Null, &ctx); }
                if ui.small_button("Excel").clicked() { let ctx = ui.ctx().clone(); app.dispatch("file.import.xlsx", &Value::Null, &ctx); }
                if ui.small_button("DBF").clicked() { let ctx = ui.ctx().clone(); app.dispatch("file.import.dbf", &Value::Null, &ctx); }
            });
            if app.loading.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let msg = app.loading.as_ref().map(|l| l.progress.clone()).unwrap_or_default();
                    ui.label(RichText::new(msg).color(t.text_muted).size(11.0));
                    if ui.small_button("Cancel").clicked() { app.cancel_load(); }
                });
            }
            ui.separator();

            let fields: Vec<(String, DataType)> = app.table.as_ref().map(|tbl| tbl.columns.iter().map(|c| (c.name.clone(), c.effective_type())).collect()).unwrap_or_default();
            if fields.is_empty() {
                ui.label(RichText::new("No dataset loaded.").color(t.text_disabled));
            } else {
                egui::ScrollArea::vertical().id_salt("fields_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    let mut add_row: Option<String> = None;
                    let mut add_col: Option<String> = None;
                    let mut add_val: Option<(String, Aggregate)> = None;
                    let mut type_change: Option<(usize, DataType)> = None;
                    for (i, (name, current)) in fields.iter().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(name).strong().color(t.text_strong).size(12.0));
                                let mut effective = *current;
                                egui::ComboBox::from_id_salt(("type", i)).selected_text(format!("{effective:?}")).width(80.0).show_ui(ui, |ui| {
                                    for dt in [DataType::String, DataType::Integer, DataType::Decimal, DataType::Boolean, DataType::Date, DataType::DateTime] {
                                        ui.selectable_value(&mut effective, dt, format!("{dt:?}"));
                                    }
                                });
                                if effective != *current { type_change = Some((i, effective)); }
                            });
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button("+Row").clicked() { add_row = Some(name.clone()); }
                                if ui.small_button("+Col").clicked() { add_col = Some(name.clone()); }
                                if ui.small_button("Sum").clicked() { add_val = Some((name.clone(), Aggregate::Sum)); }
                                if ui.small_button("Count").clicked() { add_val = Some((name.clone(), Aggregate::Count)); }
                                if ui.small_button("Avg").clicked() { add_val = Some((name.clone(), Aggregate::Average)); }
                                if ui.small_button("Min").clicked() { add_val = Some((name.clone(), Aggregate::Min)); }
                                if ui.small_button("Max").clicked() { add_val = Some((name.clone(), Aggregate::Max)); }
                            });
                        });
                    }
                    if let Some((i, dt)) = type_change { app.reinterpret_column(i, dt); }
                    if let Some(n) = add_row { app.add_row_field(n); }
                    if let Some(n) = add_col { app.add_col_field(n); }
                    if let Some((n, a)) = add_val { app.add_value_field(n, a); }
                });
            }
        });

    egui::SidePanel::right("dock_right")
        .resizable(true)
        .default_width(260.0)
        .min_width(160.0)
        .frame(egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(8)))
        .show_inside(ui, |ui| {
            panel_header(ui, "Pivot Layout", &t);
            ui.horizontal(|ui| {
                if ui.button("Recompute").clicked() { app.request_pivot_recompute(); }
                if ui.button("Clear").clicked() { app.clear_layout(); }
                if app.pivot_computing.is_some() {
                    ui.spinner();
                    if ui.small_button("Cancel").clicked() { app.cancel_pivot(); }
                }
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().id_salt("layout_scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new("Rows").strong().color(t.text_strong));
                let mut remove_row: Option<usize> = None;
                let mut move_row: Option<(usize, usize)> = None;
                let row_len = app.pivot_spec.rows.len();
                for (i, f) in app.pivot_spec.rows.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&f.name).color(t.text));
                        if ui.small_button("x").on_hover_text("Remove from Rows").clicked() { remove_row = Some(i); }
                        if i > 0 && ui.small_button("^").clicked() { move_row = Some((i, i - 1)); }
                        if i + 1 < row_len && ui.small_button("v").clicked() { move_row = Some((i, i + 1)); }
                    });
                }
                if row_len == 0 { ui.label(RichText::new("(none)").color(t.text_disabled)); }
                if let Some(i) = remove_row { app.remove_row_field(i); }
                else if let Some((a, b)) = move_row { app.move_row_field(a, b); }
                ui.add_space(4.0);
                ui.label(RichText::new("Columns").strong().color(t.text_strong));
                let mut remove_col: Option<usize> = None;
                let mut move_col: Option<(usize, usize)> = None;
                let col_len = app.pivot_spec.columns.len();
                for (i, f) in app.pivot_spec.columns.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&f.name).color(t.text));
                        if ui.small_button("x").on_hover_text("Remove from Columns").clicked() { remove_col = Some(i); }
                        if i > 0 && ui.small_button("^").clicked() { move_col = Some((i, i - 1)); }
                        if i + 1 < col_len && ui.small_button("v").clicked() { move_col = Some((i, i + 1)); }
                    });
                }
                if col_len == 0 { ui.label(RichText::new("(none)").color(t.text_disabled)); }
                if let Some(i) = remove_col { app.remove_col_field(i); }
                else if let Some((a, b)) = move_col { app.move_col_field(a, b); }
                ui.add_space(4.0);
                ui.label(RichText::new("Values").strong().color(t.text_strong));
                let mut remove_val: Option<usize> = None;
                for (i, v) in app.pivot_spec.values.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}: {}", v.aggregate.label(), v.field.name)).color(t.text));
                        if ui.small_button("x").on_hover_text("Remove value").clicked() { remove_val = Some(i); }
                    });
                }
                if app.pivot_spec.values.is_empty() { ui.label(RichText::new("(add Sum/Count/Avg from Fields)").color(t.text_disabled)); }
                if let Some(i) = remove_val { app.remove_value_field(i); }
                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new("Options").strong().color(t.text_strong));
                let mut row_sub = app.pivot_spec.subtotals.row_subtotals;
                let mut grand = app.pivot_spec.subtotals.grand_total;
                if ui.checkbox(&mut row_sub, "Row subtotals").changed() { app.pivot_spec.subtotals.row_subtotals = row_sub; app.request_pivot_recompute(); }
                if ui.checkbox(&mut grand, "Grand total").changed() { app.pivot_spec.subtotals.grand_total = grand; app.request_pivot_recompute(); }
                ui.add_space(6.0);
                ui.label(RichText::new("Stub head (col 1)").color(t.text_strong));
                let mut stub = app.pivot_spec.stub_head.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut stub).changed() {
                    app.pivot_spec.stub_head = if stub.trim().is_empty() { None } else { Some(stub) };
                    app.request_pivot_recompute();
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(t.canvas).stroke(egui::Stroke::NONE).inner_margin(egui::Margin::same(12)))
        .show_inside(ui, |ui| {
            pivot_grid(app, ui, &t);
        });
}

fn panel_header(ui: &mut egui::Ui, title: &str, t: &Tokens) {
    ui.label(RichText::new(title).strong().color(t.text_strong).size(13.0));
}

fn pivot_grid(app: &PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    if let Some(model) = &app.pivot_model {
        ui.label(RichText::new(&model.title).strong().color(t.text_strong).size(16.0));
        ui.add_space(8.0);
        render_model(model, ui, t);
    } else if let Some(table) = &app.table {
        ui.label(RichText::new("Pivot Grid").strong().color(t.text_strong).size(16.0));
        ui.label(RichText::new("Add fields to Rows/Columns/Values on the left, then Recompute. Showing data preview (first 120 rows).").color(t.text_muted));
        ui.add_space(8.0);
        let preview_rows = table.row_count().min(120);
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("data_preview_center").striped(false).min_col_width(100.0).show(ui, |ui| {
                for col in &table.columns { header_cell(ui, &col.name, t); }
                ui.end_row();
                for r in 0..preview_rows {
                    for c in 0..table.column_count() {
                        body_cell(ui, &table.get(r, c).map(|v| v.as_display()).unwrap_or_default(), t, false);
                    }
                    ui.end_row();
                }
            });
        });
    } else {
        ui.label(RichText::new("Pivot Grid").strong().color(t.text_strong).size(16.0));
        ui.label(RichText::new("Import data from Home to begin.").color(t.text_muted));
    }
}

fn render_model(model: &ThemedTableModel, ui: &mut egui::Ui, t: &Tokens) {
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        // Render header rows
        egui::Grid::new("pivot_header_grid").striped(false).min_col_width(90.0).show(ui, |ui| {
            for hrow in &model.header_rows {
                for cell in hrow {
                    // colspan/rowspan not natively supported by egui::Grid; render text (Phase 4 can use custom painter)
                    let mut text = cell.text.clone();
                    if cell.colspan > 1 { text = format!("{}  ", text); }
                    header_cell(ui, &text, t);
                }
                ui.end_row();
            }
        });
        ui.add_space(2.0);
        egui::Grid::new("pivot_body_grid").striped(false).min_col_width(90.0).show(ui, |ui| {
            for brow in &model.body_rows {
                for cell in brow {
                    let is_row_header = matches!(cell.role, CellRole::RowHeader | CellRole::StubHead);
                    let is_subtotal = matches!(cell.role, CellRole::Subtotal);
                    let bg = if is_subtotal { t.grid_header } else if is_row_header { t.panel_alt } else { t.grid_body };
                    let text_color = if is_row_header || is_subtotal { t.text_strong } else { t.text };
                    let mut label = cell.text.clone();
                    if cell.indent > 0 && !label.is_empty() && !is_subtotal {
                        label = format!("{}{}", "  ".repeat(cell.indent.min(4)), label);
                    }
                    egui::Frame::new().fill(bg).stroke(egui::Stroke::new(1.0_f32, t.grid_line)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                        let rt = if is_row_header || is_subtotal { RichText::new(label).strong().color(text_color) } else { RichText::new(label).color(text_color) };
                        ui.label(rt);
                    });
                }
                ui.end_row();
            }
        });
        if !model.footer_bands.is_empty() {
            ui.add_space(2.0);
            egui::Grid::new("pivot_footer_grid").striped(false).min_col_width(90.0).show(ui, |ui| {
                for band in &model.footer_bands {
                    for cell in &band.cells {
                        egui::Frame::new().fill(t.grid_header).stroke(egui::Stroke::new(1.0_f32, t.grid_line)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                            ui.label(RichText::new(&cell.text).strong().color(t.text_strong));
                        });
                    }
                    // if notes band spans conceptually as one cell with many logical cols, pad handled by renderer in future
                    ui.end_row();
                }
            });
        }
    });
}

fn header_cell(ui: &mut egui::Ui, text: &str, t: &Tokens) {
    egui::Frame::new().fill(t.grid_header).stroke(egui::Stroke::new(1.0_f32, t.grid_line)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.label(RichText::new(text).strong().color(t.text_strong));
    });
}

fn body_cell(ui: &mut egui::Ui, text: &str, t: &Tokens, row_header: bool) {
    let fill = if row_header { t.grid_header } else { t.grid_body };
    let color = if row_header { t.text_strong } else { t.text };
    egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0_f32, t.grid_line)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.label(RichText::new(text).color(color));
    });
}
