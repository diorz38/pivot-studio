//! Docked, resizable panel layout (DesignCraft docking feel, Phase 2).
//! Home page: open/import/connect. Worksheet: field wells, pivot grid (ThemedTableModel), data preview.

use crate::app::PivotApp;
use egui::{Id, RichText, Sense, Vec2};
use pivot_core::{Aggregate, DataType, FilterOp, FooterBandSpec, ThemedTableModel, Value as CoreValue};
use pivot_theme::Tokens;
use serde_json::Value;

#[derive(Clone, Debug)]
struct FieldDrag {
    name: String,
    dtype: DataType,
}

fn drag_id() -> Id {
    Id::new("pivot_field_drag")
}

fn begin_drag(ui: &egui::Ui, name: &str, dtype: DataType) {
    ui.ctx().memory_mut(|m| m.data.insert_temp(drag_id(), FieldDrag { name: name.to_string(), dtype }));
}

fn current_drag(ui: &egui::Ui) -> Option<FieldDrag> {
    ui.ctx().memory(|m| m.data.get_temp::<FieldDrag>(drag_id()))
}

fn clear_drag_if_released(ui: &egui::Ui) {
    if ui.input(|i| i.pointer.any_released()) {
        ui.ctx().memory_mut(|m| m.data.remove::<FieldDrag>(drag_id()));
    }
}

fn drop_zone(ui: &mut egui::Ui, t: &Tokens, label: &str, hint: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let dragging = current_drag(ui).is_some();
    let fill = if dragging { t.hover } else { t.panel_alt };
    let stroke_color = if dragging { t.accent } else { t.border };
    let frame = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(if dragging { 1.5_f32 } else { 1.0_f32 }, stroke_color))
        .inner_margin(egui::Margin::same(6))
        .corner_radius(4.0);
    let resp = frame.show(ui, |ui| {
        ui.label(RichText::new(label).strong().color(t.text_strong).size(12.0));
        if dragging {
            ui.label(RichText::new(format!("Drop field here to add to {hint}")).color(t.accent).size(11.0));
        }
        body(ui);
    });
    resp.response.hovered() && ui.input(|i| i.pointer.any_released()) && dragging
}

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

            if app.table.is_some() {
                egui::Frame::new()
                    .fill(t.canvas)
                    .stroke(egui::Stroke::new(1.0_f32, t.border))
                    .inner_margin(egui::Margin::same(8))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("How to build a pivot").strong().color(t.text_strong).size(12.0));
                        ui.label(RichText::new("1) +Row = group rows (e.g. Region)\n2) +Col = split columns (optional, e.g. Year)\n3) Add a Value = the number to calculate (required)\n   • Sum/Avg/Min/Max for numbers\n   • Count for any field").color(t.text_muted).size(11.0));
                        if app.pivot_spec.values.is_empty() {
                            ui.label(RichText::new("Tip: add at least one Value (e.g. Count) to show the pivot result.").color(t.accent).size(11.0));
                        }
                    });
                ui.add_space(4.0);
            }

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
                                let drag_resp = ui.add(egui::Label::new(RichText::new(format!("☰ {name}")).strong().color(t.text_strong).size(12.0)).sense(Sense::click_and_drag()))
                                    .on_hover_text("Drag to Rows, Columns, or Values on the right. Tip: drag to Values for Count (or Sum for numbers).");
                                if drag_resp.drag_started() {
                                    begin_drag(ui, name, *current);
                                }
                                let mut effective = *current;
                                egui::ComboBox::from_id_salt(("type", i)).selected_text(format!("{effective:?}")).width(78.0).show_ui(ui, |ui| {
                                    for dt in [DataType::String, DataType::Integer, DataType::Decimal, DataType::Boolean, DataType::Date, DataType::DateTime] {
                                        ui.selectable_value(&mut effective, dt, format!("{dt:?}"));
                                    }
                                });
                                if effective != *current { type_change = Some((i, effective)); }
                            });
                            ui.label(RichText::new("Quick add (or drag ☰ to layout)").color(t.text_muted).size(10.0));
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button("+ Row").on_hover_text("Group rows by this field").clicked() { add_row = Some(name.clone()); }
                                if ui.small_button("+ Column").on_hover_text("Split columns by this field (optional)").clicked() { add_col = Some(name.clone()); }
                            });
                            ui.horizontal_wrapped(|ui| {
                                let numeric = matches!(current, DataType::Integer | DataType::Decimal);
                                if numeric {
                                    if ui.small_button("Sum").on_hover_text("Add Sum of this field as a pivot value").clicked() { add_val = Some((name.clone(), Aggregate::Sum)); }
                                    if ui.small_button("Avg").on_hover_text("Average").clicked() { add_val = Some((name.clone(), Aggregate::Average)); }
                                }
                                if ui.small_button("Count").on_hover_text("Count rows (works for any field)").clicked() { add_val = Some((name.clone(), Aggregate::Count)); }
                                if ui.small_button("Count distinct").on_hover_text("Count unique values").clicked() { add_val = Some((name.clone(), Aggregate::CountDistinct)); }
                                if numeric {
                                    if ui.small_button("Min").clicked() { add_val = Some((name.clone(), Aggregate::Min)); }
                                    if ui.small_button("Max").clicked() { add_val = Some((name.clone(), Aggregate::Max)); }
                                }
                            });
                        });
                    }
                    if let Some((i, dt)) = type_change { app.reinterpret_column(i, dt); }
                    if let Some(n) = add_row { app.add_row_field(n); }
                    if let Some(n) = add_col { app.add_col_field(n); }
                    if let Some((n, a)) = add_val { app.add_value_field(n, a); }
                });
            }
            if let Some(d) = current_drag(ui) {
                if let Some(pos) = ui.ctx().pointer_hover_pos() {
                    let text = format!("☰ {} -> drop on Rows/Columns/Values", d.name);
                    egui::Area::new(Id::new("pivot_field_drag_ghost")).fixed_pos(pos + egui::vec2(12.0, 12.0)).order(egui::Order::Tooltip).interactable(false).show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.accent)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                            ui.label(RichText::new(text).color(t.text_strong).size(12.0));
                        });
                    });
                    ui.ctx().request_repaint();
                }
            }
            clear_drag_if_released(ui);
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
            ui.add_space(4.0);
            ui.label(RichText::new("Drag fields (☰) from the left into Rows, Columns, or Values.").color(t.text_muted).size(11.0));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().id_salt("layout_scroll").auto_shrink([false, false]).show(ui, |ui| {
                let mut drop_row: Option<String> = None;
                let mut drop_col: Option<String> = None;
                let mut drop_val: Option<(String, DataType)> = None;

                if drop_zone(ui, &t, "Rows", "Rows (grouping)", |ui| {
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
                    if row_len == 0 { ui.label(RichText::new("(drop a field here)").color(t.text_disabled)); }
                    if let Some(i) = remove_row { app.remove_row_field(i); }
                    else if let Some((a, b)) = move_row { app.move_row_field(a, b); }
                }) { drop_row = current_drag(ui).map(|d| d.name); }

                ui.add_space(6.0);
                if drop_zone(ui, &t, "Columns", "Columns (optional split)", |ui| {
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
                    if col_len == 0 { ui.label(RichText::new("(drop a field here, optional)").color(t.text_disabled)); }
                    if let Some(i) = remove_col { app.remove_col_field(i); }
                    else if let Some((a, b)) = move_col { app.move_col_field(a, b); }
                }) { drop_col = current_drag(ui).map(|d| d.name); }

                ui.add_space(6.0);
                if drop_zone(ui, &t, "Values", "Values (required calculation)", |ui| {
                    let mut remove_val: Option<usize> = None;
                    for (i, v) in app.pivot_spec.values.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}: {}", v.aggregate.label(), v.field.name)).color(t.text));
                            if ui.small_button("x").on_hover_text("Remove value").clicked() { remove_val = Some(i); }
                        });
                    }
                    if app.pivot_spec.values.is_empty() { ui.label(RichText::new("(drop a field here — Sum for numbers, Count otherwise)").color(t.text_disabled)); }
                    if let Some(i) = remove_val { app.remove_value_field(i); }
                }) { drop_val = current_drag(ui).map(|d| (d.name, d.dtype)); }

                if let Some(n) = drop_row { app.add_row_field(n); }
                if let Some(n) = drop_col { app.add_col_field(n); }
                if let Some((n, dt)) = drop_val {
                    let agg = if matches!(dt, DataType::Integer | DataType::Decimal) { Aggregate::Sum } else { Aggregate::Count };
                    app.add_value_field(n, agg);
                }
                clear_drag_if_released(ui);
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
                ui.label(RichText::new("Build your pivot (left panel: Fields)").strong().color(t.text_strong));
                let rows = if has_rows { app.pivot_spec.rows.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ") } else { "(none - optional)".to_string() };
                let cols = if has_cols { app.pivot_spec.columns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ") } else { "(none - optional)".to_string() };
                let vals = if has_vals { app.pivot_spec.values.iter().map(|v| format!("{}({})", v.aggregate.label(), v.field.name)).collect::<Vec<_>>().join(", ") } else { "(missing - required)".to_string() };
                ui.label(RichText::new(format!("Rows: {rows}")).color(t.text));
                ui.label(RichText::new(format!("Columns: {cols}")).color(t.text));
                ui.label(RichText::new(format!("Values: {vals}")).color(if has_vals { t.text } else { t.accent }));
                ui.add_space(4.0);
                if !has_vals {
                    ui.label(RichText::new("To see a pivot result, add a Value. Example: pick a text/category field and click Count, or pick a number field and click Sum. Rows/Columns only group data; they don’t produce a result without a Value.").color(t.text_muted));
                } else if app.pivot_computing.is_some() {
                    ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("Computing pivot...").color(t.text_muted)); });
                } else {
                    ui.label(RichText::new("If the result doesn’t appear, click Recompute in the Pivot Layout panel.").color(t.text_muted));
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
