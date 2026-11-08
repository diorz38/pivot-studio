//! Docked, resizable panel layout (DesignCraft docking feel, simplified for Phase 0).
//! Regions: Sources/Fields (left), Filters/Settings (right), Pivot grid (center).

use crate::app::PivotApp;
use pivot_theme::Tokens;

pub fn show(app: &mut PivotApp, ui: &mut egui::Ui) {
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
                    body_cell(ui, "â€”", t, false);
                    body_cell(ui, "â€”", t, false);
                    ui.end_row();

                    body_cell(ui, "Row 2", t, true);
                    body_cell(ui, "â€”", t, false);
                    body_cell(ui, "â€”", t, false);
                    ui.end_row();
                });
        });

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(format!(
            "Ready. Use View â†’ Brightness to switch modes (current: {}).",
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
