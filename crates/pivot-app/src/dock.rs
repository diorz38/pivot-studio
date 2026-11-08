//! Docked, resizable panel layout (DesignCraft docking feel, Phase 1).
//! Home page: open saved project / import CSV, Excel, DBF / connect to a new DB.
//! Worksheet: docked pivot workspace with field list populated from loaded data.

use crate::app::PivotApp;
use egui::{RichText, Sense, Vec2};
use pivot_core::DataType;
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
                    ui.label(
                        RichText::new("Pivot Studio")
                            .strong()
                            .size(30.0)
                            .color(t.text_strong),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Open a saved project, import a file, or connect to a database to start building a pivot table.",
                        )
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
                                    let msg = app
                                        .loading
                                        .as_ref()
                                        .map(|l| l.progress.clone())
                                        .unwrap_or_else(|| "Loading...".to_string());
                                    ui.label(RichText::new(msg).color(t.text));
                                    if ui.button("Cancel").clicked() {
                                        app.cancel_load();
                                    }
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
                            action_card(ui, app, &t, "Open Worksheet", "View the loaded dataset and fields.", "nav.worksheet");
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
                            ui.label(
                                RichText::new("No recent projects yet. Project history arrives in a later phase.")
                                    .color(t.text_disabled),
                            );
                        });
                });
            });
        });
}

fn action_card(
    ui: &mut egui::Ui,
    app: &mut PivotApp,
    t: &Tokens,
    title: &str,
    detail: &str,
    command: &str,
) {
    let size = Vec2::new(250.0, 96.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_text(detail);
    let bg = if response.hovered() { t.hover } else { t.panel };
    let stroke = if response.hovered() {
        egui::Stroke::new(1.0_f32, t.accent)
    } else {
        egui::Stroke::new(1.0_f32, t.border)
    };
    ui.painter().rect(rect, 6.0, bg, stroke, egui::StrokeKind::Inside);
    let inner = rect.shrink(14.0);
    ui.painter().text(
        egui::pos2(inner.min.x, inner.min.y),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(15.0),
        t.text_strong,
    );
    ui.painter().text(
        egui::pos2(inner.min.x, inner.min.y + 26.0),
        egui::Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(12.0),
        t.text_muted,
    );
    if response.clicked() {
        let ctx = ui.ctx().clone();
        app.dispatch(command, &Value::Null, &ctx);
    }
}

fn worksheet(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());

    egui::TopBottomPanel::bottom("dock_bottom")
        .resizable(true)
        .default_height(160.0)
        .min_height(80.0)
        .frame(
            egui::Frame::new()
                .fill(t.panel_alt)
                .stroke(egui::Stroke::new(1.0_f32, t.border))
                .inner_margin(egui::Margin::same(8)),
        )
        .show_inside(ui, |ui| {
            panel_header(ui, "Filters / Settings", &t);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Placeholder: report filters, value formatting, and view settings (Phase 2+).")
                    .color(t.text_muted),
            );
        });

    egui::SidePanel::left("dock_left")
        .resizable(true)
        .default_width(280.0)
        .min_width(180.0)
        .frame(
            egui::Frame::new()
                .fill(t.panel)
                .stroke(egui::Stroke::new(1.0_f32, t.border))
                .inner_margin(egui::Margin::same(8)),
        )
        .show_inside(ui, |ui| {
            panel_header(ui, "Sources / Fields", &t);
            ui.add_space(4.0);
            if let Some(name) = &app.source_name {
                ui.label(RichText::new(format!("Source: {name}")).color(t.text).size(12.0));
            }
            ui.horizontal(|ui| {
                if ui.button("Import CSV").clicked() {
                    let ctx = ui.ctx().clone();
                    app.dispatch("file.import.csv", &Value::Null, &ctx);
                }
                if ui.button("Import Excel").clicked() {
                    let ctx = ui.ctx().clone();
                    app.dispatch("file.import.xlsx", &Value::Null, &ctx);
                }
                if ui.button("Import DBF").clicked() {
                    let ctx = ui.ctx().clone();
                    app.dispatch("file.import.dbf", &Value::Null, &ctx);
                }
            });
            if app.loading.is_some() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    let msg = app.loading.as_ref().map(|l| l.progress.clone()).unwrap_or_default();
                    ui.label(RichText::new(msg).color(t.text_muted).size(11.5));
                    if ui.small_button("Cancel").clicked() {
                        app.cancel_load();
                    }
                });
            }
            ui.add_space(8.0);
            ui.separator();
            if app.table.is_some() {
                let col_count = app.table.as_ref().map(|t| t.column_count()).unwrap_or(0);
                let cols: Vec<(usize, String, DataType)> = app
                    .table
                    .as_ref()
                    .map(|t| {
                        t.columns
                            .iter()
                            .enumerate()
                            .map(|(i, c)| (i, c.name.clone(), c.effective_type()))
                            .collect()
                    })
                    .unwrap_or_default();
                ui.label(RichText::new(format!("Fields ({col_count})")).strong().color(t.text_strong));
                ui.add_space(4.0);
                let mut change: Option<(usize, DataType)> = None;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, name, current) in &cols {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).color(t.text).size(12.0));
                            let mut effective = *current;
                            let before = effective;
                            egui::ComboBox::from_id_salt(("type", *i))
                                .selected_text(format!("{effective:?}"))
                                .width(95.0)
                                .show_ui(ui, |ui| {
                                    for dt in [
                                        DataType::String, DataType::Integer, DataType::Decimal,
                                        DataType::Boolean, DataType::Date, DataType::DateTime,
                                    ] {
                                        ui.selectable_value(&mut effective, dt, format!("{dt:?}"));
                                    }
                                });
                            if effective != before {
                                change = Some((*i, effective));
                            }
                        });
                    }
                });
                if let Some((i, dt)) = change {
                    app.reinterpret_column(i, dt);
                }
            } else {
                ui.label(RichText::new("No dataset loaded. Import a file to see fields.").color(t.text_disabled));
            }
        });

    egui::SidePanel::right("dock_right")
        .resizable(true)
        .default_width(220.0)
        .min_width(140.0)
        .frame(
            egui::Frame::new()
                .fill(t.panel)
                .stroke(egui::Stroke::new(1.0_f32, t.border))
                .inner_margin(egui::Margin::same(8)),
        )
        .show_inside(ui, |ui| {
            panel_header(ui, "Layout / Properties", &t);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Placeholder: pivot field wells, theme picker, stub-head text, footer bands (Phase 2-4).")
                    .color(t.text_muted),
            );
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(t.canvas)
                .stroke(egui::Stroke::NONE)
                .inner_margin(egui::Margin::same(12)),
        )
        .show_inside(ui, |ui| {
            grid_placeholder(app, ui, &t);
        });
}

fn panel_header(ui: &mut egui::Ui, title: &str, t: &Tokens) {
    ui.label(RichText::new(title).strong().color(t.text_strong).size(13.0));
}

fn grid_placeholder(app: &PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    ui.label(RichText::new("Pivot Grid / Data Preview").strong().color(t.text_strong).size(16.0));
    ui.add_space(6.0);
    ui.label(
        RichText::new("Phase 1: loaded data preview (first rows). Pivot computation and themed table arrive in Phase 2+.")
            .color(t.text_muted),
    );
    ui.add_space(12.0);

    if let Some(table) = &app.table {
        let preview_rows = table.row_count().min(200);
        egui::Frame::new()
            .fill(t.grid_body)
            .stroke(egui::Stroke::new(1.0_f32, t.grid_line))
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("data_preview_grid")
                        .striped(false)
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            for col in &table.columns {
                                header_cell(ui, &col.name, t);
                            }
                            ui.end_row();
                            for r in 0..preview_rows {
                                for c in 0..table.column_count() {
                                    let text = table.get(r, c).map(|v| v.as_display()).unwrap_or_default();
                                    body_cell(ui, &text, t, false);
                                }
                                ui.end_row();
                            }
                        });
                });
            });
        if table.row_count() > preview_rows {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Showing {preview_rows} of {} rows.", table.row_count())).color(t.text_muted).size(11.5));
        }
    } else {
        egui::Frame::new()
            .fill(t.grid_body)
            .stroke(egui::Stroke::new(1.0_f32, t.grid_line))
            .inner_margin(egui::Margin::same(0))
            .show(ui, |ui| {
                egui::Grid::new("phase0_preview_grid")
                    .striped(false)
                    .min_col_width(120.0)
                    .show(ui, |ui| {
                        header_cell(ui, "Stub Head (col 1)", t);
                        header_cell(ui, "Column A", t);
                        header_cell(ui, "Column B", t);
                        ui.end_row();
                        body_cell(ui, "No data", t, true);
                        body_cell(ui, "-", t, false);
                        body_cell(ui, "-", t, false);
                        ui.end_row();
                    });
            });
    }
}

fn header_cell(ui: &mut egui::Ui, text: &str, t: &Tokens) {
    egui::Frame::new()
        .fill(t.grid_header)
        .stroke(egui::Stroke::new(1.0_f32, t.grid_line))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label(RichText::new(text).strong().color(t.text_strong));
        });
}

fn body_cell(ui: &mut egui::Ui, text: &str, t: &Tokens, row_header: bool) {
    let fill = if row_header { t.grid_header } else { t.grid_body };
    let color = if row_header { t.text_strong } else { t.text };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, t.grid_line))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color));
        });
}
