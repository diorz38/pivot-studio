//! Docked, resizable panel layout (DesignCraft docking feel, Phase 2).
//! Home page: open/import/connect. Worksheet: field wells, pivot grid (ThemedTableModel), data preview.

use crate::app::PivotApp;
use egui::{RichText, Sense, Vec2};
use pivot_core::{Aggregate, DataType, FilterOp, FooterBandSpec, ThemedTableModel, Value as CoreValue};
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
        .default_height(210.0)
        .min_height(90.0)
        .frame(egui::Frame::new().fill(t.panel_alt).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(8)))
        .show_inside(ui, |ui| {
            panel_header(ui, "Filters / Data Preview", &t);
            ui.add_space(2.0);
            if app.table.is_some() {
                filters_ui(app, ui, &t);
                ui.separator();
            }
            if let Some(table) = &app.table {
                let preview_rows = table.row_count().min(60);
                egui::ScrollArea::both().auto_shrink([false, false]).max_height(110.0).id_salt("data_preview_bottom").show(ui, |ui| {
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
                ui.label(RichText::new("Import data to see filters and a preview.").color(t.text_muted));
            }
        });

    // Left panel removed for now; Pivot Layout (right) provides field selection.

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
            ui.add_space(4.0);
            ui.label(RichText::new("Pick a data column, choose how to use it, then Add.").color(t.text_muted).size(11.0));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().id_salt("layout_scroll").auto_shrink([false, false]).show(ui, |ui| {
                app.sync_layout_pickers();
                let field_names: Vec<String> = app.table.as_ref().map(|tbl| tbl.columns.iter().map(|c| c.name.clone()).collect()).unwrap_or_default();
                if field_names.is_empty() {
                    ui.label(RichText::new("Load data to configure Rows, Columns, and Values.").color(t.text_disabled));
                } else {
                    layout_picker_ui(app, ui, &t, &field_names);
                }
                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new("Options").strong().color(t.text_strong));
                let mut row_sub = app.pivot_spec.subtotals.row_subtotals;
                if ui.checkbox(&mut row_sub, "Row subtotals").changed() { app.pivot_spec.subtotals.row_subtotals = row_sub; app.request_pivot_recompute(); }

                ui.add_space(6.0);
                ui.label(RichText::new("Table title (AC3.3)").color(t.text_strong));
                let auto = app.pivot_model.as_ref().map(|m| if m.title_pinned { "(pinned)".to_string() } else { m.title.clone() }).unwrap_or_else(|| "(recompute to preview)".to_string());
                let mut pinned = app.pivot_spec.title_override.is_some();
                if ui.checkbox(&mut pinned, "Pin title").changed() {
                    app.set_title_pinned(pinned);
                }
                if let Some(over) = &mut app.pivot_spec.title_override {
                    if ui.text_edit_singleline(over).changed() { app.request_pivot_recompute(); }
                    if ui.small_button("Reset title to auto").clicked() { app.set_title_pinned(false); }
                } else {
                    ui.label(RichText::new(format!("Auto: {auto}")).color(t.text_muted).size(11.0));
                }

                ui.add_space(6.0);
                ui.label(RichText::new("Stub head (col 1, AC3.1)").color(t.text_strong));
                let mut stub = app.pivot_spec.stub_head.clone().unwrap_or_default();
                let stub_resp = ui.text_edit_singleline(&mut stub);
                if stub_resp.changed() {
                    app.pivot_spec.stub_head = if stub.trim().is_empty() { None } else { Some(stub.clone()) };
                    app.request_pivot_recompute();
                }
                if !app.pivot_spec.stub_head.is_some() && !stub.is_empty() { /* already normalized */ }
                if app.pivot_spec.stub_head.is_some() && ui.small_button("Clear stub head").clicked() {
                    app.pivot_spec.stub_head = None;
                    app.request_pivot_recompute();
                }

                ui.add_space(8.0);
                ui.separator();
                ui.label(RichText::new("Footer bands (AC3.2)").strong().color(t.text_strong));
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("+ Grand Total").clicked() { app.add_footer_band(FooterBandSpec::grand_total()); }
                    if ui.small_button("+ Average").clicked() { app.add_footer_band(FooterBandSpec::average()); }
                    if ui.small_button("+ Count").clicked() { app.add_footer_band(FooterBandSpec::count()); }
                    if ui.small_button("+ Notes").clicked() { app.add_footer_band(FooterBandSpec::notes()); }
                });
                let bands = app.pivot_spec.effective_footer_bands();
                if bands.is_empty() {
                    ui.label(RichText::new("(no footer bands)").color(t.text_disabled));
                } else {
                    let mut remove: Option<usize> = None;
                    let mut mov: Option<(usize, usize)> = None;
                    let len = bands.len();
                    for (i, b) in bands.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{:?}: {}", b.kind, b.effective_label())).color(t.text).size(12.0));
                            if ui.small_button("x").clicked() { remove = Some(i); }
                            if i > 0 && ui.small_button("^").clicked() { mov = Some((i, i - 1)); }
                            if i + 1 < len && ui.small_button("v").clicked() { mov = Some((i, i + 1)); }
                        });
                    }
                    if let Some(i) = remove { app.remove_footer_band(i); }
                    else if let Some((a, b)) = mov { app.move_footer_band(a, b); }
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(t.canvas).stroke(egui::Stroke::NONE).inner_margin(egui::Margin::same(12)))
        .show_inside(ui, |ui| {
            pivot_grid(app, ui, &t);
        });
}

fn data_type_label(dt: DataType) -> &'static str {
    match dt {
        DataType::String => "Text (String)",
        DataType::Integer => "Integer",
        DataType::Decimal => "Decimal",
        DataType::Boolean => "Boolean",
        DataType::Date => "Date",
        DataType::DateTime => "DateTime",
    }
}

fn agg_label(agg: Aggregate) -> &'static str {
    match agg {
        Aggregate::Sum => "Sum",
        Aggregate::Count => "Count",
        Aggregate::CountDistinct => "Count distinct",
        Aggregate::Average => "Average",
        Aggregate::Min => "Min",
        Aggregate::Max => "Max",
    }
}

fn layout_picker_ui(app: &mut PivotApp, ui: &mut egui::Ui, t: &Tokens, field_names: &[String]) {
    let mut add_row: Option<(String, DataType)> = None;
    let mut add_col: Option<(String, DataType)> = None;
    let mut add_val: Option<(String, Aggregate)> = None;

    ui.group(|ui| {
        ui.label(RichText::new("Rows (group by)").strong().color(t.text_strong));
        ui.label(RichText::new("1) data column  2) interpret/type").color(t.text_muted).size(11.0));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("pick_row_field").selected_text(&app.pick_row_field).width(140.0).show_ui(ui, |ui| {
                for f in field_names {
                    if ui.selectable_value(&mut app.pick_row_field, f.clone(), f).changed() {
                        if let Some(dt) = app.field_type(&app.pick_row_field) { app.pick_row_as_type = dt; }
                    }
                }
            });
            egui::ComboBox::from_id_salt("pick_row_type").selected_text(data_type_label(app.pick_row_as_type)).width(120.0).show_ui(ui, |ui| {
                for dt in [DataType::String, DataType::Integer, DataType::Decimal, DataType::Boolean, DataType::Date, DataType::DateTime] {
                    ui.selectable_value(&mut app.pick_row_as_type, dt, data_type_label(dt));
                }
            });
            if ui.button("Add Row").clicked() { add_row = Some((app.pick_row_field.clone(), app.pick_row_as_type)); }
        });
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
        if row_len == 0 { ui.label(RichText::new("(no row grouping)").color(t.text_disabled)); }
        if let Some(i) = remove_row { app.remove_row_field(i); }
        else if let Some((a, b)) = move_row { app.move_row_field(a, b); }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.label(RichText::new("Columns (split, optional)").strong().color(t.text_strong));
        ui.label(RichText::new("1) data column  2) interpret/type").color(t.text_muted).size(11.0));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("pick_col_field").selected_text(&app.pick_col_field).width(140.0).show_ui(ui, |ui| {
                for f in field_names {
                    if ui.selectable_value(&mut app.pick_col_field, f.clone(), f).changed() {
                        if let Some(dt) = app.field_type(&app.pick_col_field) { app.pick_col_as_type = dt; }
                    }
                }
            });
            egui::ComboBox::from_id_salt("pick_col_type").selected_text(data_type_label(app.pick_col_as_type)).width(120.0).show_ui(ui, |ui| {
                for dt in [DataType::String, DataType::Integer, DataType::Decimal, DataType::Boolean, DataType::Date, DataType::DateTime] {
                    ui.selectable_value(&mut app.pick_col_as_type, dt, data_type_label(dt));
                }
            });
            if ui.button("Add Column").clicked() { add_col = Some((app.pick_col_field.clone(), app.pick_col_as_type)); }
        });
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
        if col_len == 0 { ui.label(RichText::new("(no column split)").color(t.text_disabled)); }
        if let Some(i) = remove_col { app.remove_col_field(i); }
        else if let Some((a, b)) = move_col { app.move_col_field(a, b); }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.label(RichText::new("Values (required)").strong().color(t.text_strong));
        ui.label(RichText::new("1) data column  2) calculation (Sum/Count/Avg...)").color(t.text_muted).size(11.0));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("pick_val_field").selected_text(&app.pick_val_field).width(140.0).show_ui(ui, |ui| {
                for f in field_names {
                    if ui.selectable_value(&mut app.pick_val_field, f.clone(), f).changed() {
                        if let Some(dt) = app.field_type(&app.pick_val_field) {
                            app.pick_val_agg = if matches!(dt, DataType::Integer | DataType::Decimal) { Aggregate::Sum } else { Aggregate::Count };
                        }
                    }
                }
            });
            egui::ComboBox::from_id_salt("pick_val_agg").selected_text(agg_label(app.pick_val_agg)).width(120.0).show_ui(ui, |ui| {
                for agg in [Aggregate::Sum, Aggregate::Count, Aggregate::CountDistinct, Aggregate::Average, Aggregate::Min, Aggregate::Max] {
                    ui.selectable_value(&mut app.pick_val_agg, agg, agg_label(agg));
                }
            });
            if ui.button("Add Value").clicked() { add_val = Some((app.pick_val_field.clone(), app.pick_val_agg)); }
        });
        ui.label(RichText::new("Tip: Sum/Avg/Min/Max make sense for numbers; Count works for any column.").color(t.text_muted).size(10.0));
        let mut remove_val: Option<usize> = None;
        for (i, v) in app.pivot_spec.values.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}({})", v.aggregate.label(), v.field.name)).color(t.text));
                if ui.small_button("x").on_hover_text("Remove value").clicked() { remove_val = Some(i); }
            });
        }
        if app.pivot_spec.values.is_empty() { ui.label(RichText::new("(add a Value to compute the pivot)").color(t.accent)); }
        if let Some(i) = remove_val { app.remove_value_field(i); }
    });

    if let Some((n, dt)) = add_row { app.reinterpret_named_column(&n, dt); app.add_row_field(n); }
    if let Some((n, dt)) = add_col { app.reinterpret_named_column(&n, dt); app.add_col_field(n); }
    if let Some((n, agg)) = add_val { app.add_value_field(n, agg); }
}

fn panel_header(ui: &mut egui::Ui, title: &str, t: &Tokens) {
    ui.label(RichText::new(title).strong().color(t.text_strong).size(13.0));
}

fn pivot_grid(app: &PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    if let Some(model) = &app.pivot_model {
        ui.label(RichText::new(&model.title).strong().color(t.text_strong).size(16.0));
        ui.label(RichText::new(format!("Theme: {}", app.table_theme.label())).color(t.text_muted).size(11.0));
        ui.add_space(6.0);
        render_model(model, ui, t, app.brightness, app.table_theme);
    } else if let Some(table) = &app.table {
        ui.label(RichText::new("Pivot Grid").strong().color(t.text_strong).size(16.0));
        let has_rows = !app.pivot_spec.rows.is_empty();
        let has_cols = !app.pivot_spec.columns.is_empty();
        let has_vals = !app.pivot_spec.values.is_empty();

        egui::Frame::new()
            .fill(t.panel)
            .stroke(egui::Stroke::new(1.0_f32, if has_vals { t.border } else { t.accent }))
            .inner_margin(egui::Margin::same(10))
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Build your pivot (Pivot Layout panel, right)").strong().color(t.text_strong));
                let rows = if has_rows { app.pivot_spec.rows.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ") } else { "(none - optional)".to_string() };
                let cols = if has_cols { app.pivot_spec.columns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ") } else { "(none - optional)".to_string() };
                let vals = if has_vals { app.pivot_spec.values.iter().map(|v| format!("{}({})", v.aggregate.label(), v.field.name)).collect::<Vec<_>>().join(", ") } else { "(missing - required)".to_string() };
                ui.label(RichText::new(format!("Rows: {rows}")).color(t.text));
                ui.label(RichText::new(format!("Columns: {cols}")).color(t.text));
                ui.label(RichText::new(format!("Values: {vals}")).color(if has_vals { t.text } else { t.accent }));
                ui.add_space(4.0);
                if !has_vals {
                    ui.label(RichText::new("To see a pivot result, add a Value. Example: pick a text/category field and click Count, or pick a number field and click Sum. Rows/Columns only group data; they donâ€™t produce a result without a Value.").color(t.text_muted));
                } else if app.pivot_computing.is_some() {
                    ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("Computing pivot...").color(t.text_muted)); });
                } else {
                    ui.label(RichText::new("If the result doesnâ€™t appear, click Recompute in the Pivot Layout panel.").color(t.text_muted));
                }
            });

        ui.add_space(8.0);
        ui.label(RichText::new("Data preview (raw rows)").color(t.text_muted).size(12.0));
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

fn render_model(model: &ThemedTableModel, ui: &mut egui::Ui, t: &Tokens, brightness: pivot_theme::Brightness, table_theme: pivot_theme::table::TableThemeId) {
    crate::pivot_grid::show(ui, model, t, brightness, table_theme);
}

fn op_label(op: FilterOp) -> &'static str {
    match op {
        FilterOp::Equals => "Equals",
        FilterOp::NotEquals => "Not equals",
        FilterOp::Include => "Include",
        FilterOp::Exclude => "Exclude",
    }
}

fn value_label(v: &CoreValue) -> String {
    match v {
        CoreValue::Null => "(empty)".to_string(),
        _ => {
            let s = v.as_display();
            if s.is_empty() { "(empty)".to_string() } else { s }
        }
    }
}

fn filters_ui(app: &mut PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    let fields: Vec<String> = app.table.as_ref().map(|tbl| tbl.columns.iter().map(|c| c.name.clone()).collect()).unwrap_or_default();
    if fields.is_empty() {
        ui.label(RichText::new("No fields.").color(t.text_disabled));
        return;
    }
    if app.filter_field.is_empty() || !fields.iter().any(|f| f == &app.filter_field) {
        app.filter_field = fields[0].clone();
        app.filter_value_idx = 0;
    }
    let values = app.distinct_values_for(&app.filter_field, 400);
    if app.filter_value_idx >= values.len() { app.filter_value_idx = 0; }

    let mut add: Option<(String, FilterOp, CoreValue)> = None;
    let mut clear_all = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Field").color(t.text_muted).size(11.0));
        egui::ComboBox::from_id_salt("filter_field").selected_text(&app.filter_field).width(140.0).show_ui(ui, |ui| {
            for f in &fields {
                if ui.selectable_value(&mut app.filter_field, f.clone(), f).changed() {
                    app.filter_value_idx = 0;
                }
            }
        });
        ui.label(RichText::new("Op").color(t.text_muted).size(11.0));
        egui::ComboBox::from_id_salt("filter_op").selected_text(op_label(app.filter_op)).width(110.0).show_ui(ui, |ui| {
            for op in [FilterOp::Equals, FilterOp::NotEquals, FilterOp::Include, FilterOp::Exclude] {
                ui.selectable_value(&mut app.filter_op, op, op_label(op));
            }
        });
        ui.label(RichText::new("Value").color(t.text_muted).size(11.0));
        let selected = values.get(app.filter_value_idx).map(value_label).unwrap_or_else(|| "(no values)".to_string());
        egui::ComboBox::from_id_salt("filter_value").selected_text(selected).width(170.0).show_ui(ui, |ui| {
            if values.is_empty() {
                ui.label(RichText::new("(no values)").color(t.text_disabled));
            }
            for (i, v) in values.iter().enumerate() {
                ui.selectable_value(&mut app.filter_value_idx, i, value_label(v));
            }
        });
        if ui.button("Add filter").clicked() {
            if let Some(v) = values.get(app.filter_value_idx).cloned() {
                add = Some((app.filter_field.clone(), app.filter_op, v));
            } else {
                app.status = "No value selected for filter.".to_string();
            }
        }
        if ui.button("Clear all").clicked() { clear_all = true; }
    });
    if let Some((f, op, v)) = add { app.add_filter(f, op, v); }
    if clear_all { app.clear_filters(); }

    if app.pivot_spec.filters.is_empty() {
        ui.label(RichText::new("No active filters. Dynamic title/Notes update when filters change (AC3.3).").color(t.text_muted).size(11.0));
    } else {
        let mut remove: Option<usize> = None;
        for (i, f) in app.pivot_spec.filters.iter().enumerate() {
            let vtxt = f.values.first().map(value_label).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {} {}", f.field.name, op_label(f.op), vtxt)).color(t.text).size(12.0));
                if ui.small_button("x").on_hover_text("Remove filter").clicked() { remove = Some(i); }
            });
        }
        if let Some(i) = remove { app.remove_filter(i); }
    }
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
