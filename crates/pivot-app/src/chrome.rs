//! App bar and status bar (DesignCraft chrome pattern, original code).

use crate::app::PivotApp;
use pivot_theme::Tokens;

pub fn app_bar(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.app_bar)
        .stroke(egui::Stroke::new(1.0_f32, t.border))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Pivot Studio")
                        .strong()
                        .color(t.text_strong)
                        .size(14.5),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!("Phase 0 shell Â· {}", app.brightness.label()))
                        .color(t.text_muted)
                        .size(12.0),
                );
            });
        });
}

pub fn status_bar(app: &PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.status_bar)
        .stroke(egui::Stroke::new(1.0_f32, t.divider))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&app.status).color(t.text).size(12.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Brightness: {} | Rows: 0 | Cols: 0",
                            app.brightness.label()
                        ))
                        .color(t.text_muted)
                        .size(12.0),
                    );
                });
            });
        });
}
