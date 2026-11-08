//! Docked, resizable panel layout (DesignCraft docking feel, simplified for Phase 0).
//! Home page: open saved project / import CSV, Excel, DBF / connect to a new DB.
//! Worksheet: docked pivot workspace shown after data is ready (Phase 0 placeholder).

use crate::app::PivotApp;
use egui::{RichText, Sense, Vec2};
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

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                        action_card(
                            ui,
                            app,
                            &t,
                            "Open Project",
                            "Open a saved .pivot workspace.",
                            "file.open",
                        );
                        action_card(
                            ui,
                            app,
                            &t,
                            "Import CSV",
                            "Load a CSV/TSV file into a worksheet.",
                            "file.import",
                        );
                        action_card(
                            ui,
                            app,
                            &t,
                            "Import Excel",
                            "Load an .xlsx workbook.",
                            "file.import",
                        );
                        action_card(
                            ui,
                            app,
                            &t,
                            "Import DBF",
                            "Load a dBASE (.dbf) file.",
                            "file.import",
                        );
                        action_card(
                            ui,
                            app,
                            &t,
                            "Connect Database",
                            "Create a new SQLite or MySQL connection.",
                            "data.connect",
                        );
                        action_card(
                            ui,
                            app,
                            &t,
                            "Open Worksheet",
                            "View the current worksheet layout (Phase 0).",
                            "nav.worksheet",
                        );
                    });

                    ui.add_space(24.0);
                    egui::Frame::new()
                        .fill(t.panel)
                        .stroke(egui::Stroke::new(1.0_f32, t.border))
                        .inner_margin(egui::Margin::same(12))
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Recent projects")
                                    .strong()
                                    .color(t.text_strong),
                            );
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
    let bg = if response.hovered() {
        t.hover
    } else {
        t.panel
    };
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

    // Bottom strip for Filters/Settings (resizable) - shown via a bottom panel region.
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
                egui::RichText::new(
                    "Placeholder: report filters, value formatting, and view settings (Phase 2+).",
                )
                .color(t.text_muted),
            );
        });

    egui::SidePanel::left("dock_left")
        .resizable(true)
        .default_width(240.0)
        .min_width(160.0)
        .frame(
            egui::Frame::new()
                .fill(t.panel)
                .stroke(egui::Stroke::new(1.0_f32, t.border))
                .inner_margin(egui::Margin::same(8)),
        )
        .show_inside(ui, |ui| {
            panel_header(ui, "Sources / Fields", &t);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Placeholder: import CSV/XLSX/DBF/SQLite/MySQL and assign fields to Rows, Columns, Values, Filters.",
                )
                .color(t.text_muted),
            );
            ui.add_space(8.0);
            ui.separator();
            ui.label(egui::RichText::new("No dataset loaded.").color(t.text_disabled));
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
                egui::RichText::new(
                    "Placeholder: pivot field wells, theme picker, stub-head text, footer bands (Phase 2-4).",
                )
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
    ui.label(
        egui::RichText::new(title)
            .strong()
            .color(t.text_strong)
            .size(13.0),
    );
}

fn grid_placeholder(app: &PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    ui.label(
        egui::RichText::new("Pivot Grid")
            .strong()
            .color(t.text_strong)
            .size(16.0),
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Center workspace: the themed pivot table will render here (Phase 2+). \
             Export (XLSX/PDF/HTML) will render the same themed table model.",
        )
        .color(t.text_muted),
    );
    ui.add_space(16.0);

    // A small themed sample "table" using tokens (demonstrates token usage, no literals).
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

                    body_cell(ui, "Row 1", t, true);
                    body_cell(ui, "-", t, false);
                    body_cell(ui, "-", t, false);
                    ui.end_row();

                    body_cell(ui, "Row 2", t, true);
                    body_cell(ui, "-", t, false);
                    body_cell(ui, "-", t, false);
                    ui.end_row();
                });
        });

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(format!(
            "Ready. Use View -> Brightness to switch modes (current: {}).",
            app.brightness.label()
        ))
        .color(t.text),
    );
}

fn header_cell(ui: &mut egui::Ui, text: &str, t: &Tokens) {
    egui::Frame::new()
        .fill(t.grid_header)
        .stroke(egui::Stroke::new(1.0_f32, t.grid_line))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).strong().color(t.text_strong));
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
            ui.label(egui::RichText::new(text).color(color));
        });
}
