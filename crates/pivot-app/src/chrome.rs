//! App bar and status bar (DesignCraft chrome pattern, original code).
//! No generic OS title bar: Home icon leads into the main menu, with custom window controls.

use crate::app::PivotApp;
use egui::{Color32, CursorIcon, Sense, Stroke, Vec2};
use pivot_theme::Tokens;

pub fn app_bar(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::new()
        .fill(t.app_bar)
        .stroke(egui::Stroke::new(1.0_f32, t.border))
        .inner_margin(egui::Margin::symmetric(8, 4));
    frame.show(ui, |ui| {
        let height = 30.0;
        ui.set_min_height(height);
        ui.horizontal_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 24.0), Sense::click());
            let response = response.on_hover_text("Home");
            let bg = if app.on_home { t.selection } else if response.hovered() { t.hover } else { Color32::TRANSPARENT };
            ui.painter().rect_filled(rect, 3.0, bg);
            paint_home_icon(ui.painter(), rect.center(), t.text_strong);
            if response.clicked() { let ctx = ui.ctx().clone(); app.dispatch("nav.home", &serde_json::Value::Null, &ctx); }
            ui.add_space(6.0);

            ui.style_mut().visuals.override_text_color = Some(t.text_strong);
            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            ui.style_mut().visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, t.text_strong);
            ui.style_mut().visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, t.text_strong);
            ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            ui.style_mut().spacing.button_padding = egui::vec2(8.0, 3.0);
            app.menu_bar(ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                caption_buttons(app, ui);
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("v0.1.0 • {}", app.brightness.label())).color(t.text_muted).size(11.5));
                let drag_w = ui.available_width().max(40.0);
                let (drag_rect, drag_resp) = ui.allocate_exact_size(egui::vec2(drag_w, height), Sense::click_and_drag());
                let _ = drag_rect;
                if drag_resp.drag_started_by(egui::PointerButton::Primary) { ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag); }
                if drag_resp.double_clicked() {
                    let on = !ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    let ctx = ui.ctx().clone();
                    app.dispatch("window.maximize", &serde_json::json!({"on": on}), &ctx);
                }
            });
        });
    });
}

fn paint_home_icon(painter: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = 5.0_f32;
    let stroke = Stroke::new(1.4_f32, color);
    let roof = [egui::pos2(c.x - s, c.y), egui::pos2(c.x, c.y - s), egui::pos2(c.x + s, c.y)];
    painter.add(egui::Shape::line(roof.to_vec(), stroke));
    let body = egui::Rect::from_min_max(egui::pos2(c.x - s + 1.2, c.y), egui::pos2(c.x + s - 1.2, c.y + s));
    painter.rect_stroke(body, 0.0, stroke, egui::StrokeKind::Inside);
    let door = egui::Rect::from_min_max(egui::pos2(c.x - 1.4, c.y + 1.6), egui::pos2(c.x + 1.4, c.y + s));
    painter.rect_stroke(door, 0.0, Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
}

fn caption_buttons(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    let button_w = 42.0;
    let button_h = 24.0;
    let spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 0.0;
    let buttons = [
        ("window.close", "x", "Close"),
        ("window.maximize", if maximized { "R" } else { "M" }, if maximized { "Restore" } else { "Maximize" }),
        ("window.minimize", "-", "Minimize"),
    ];
    for (cmd, glyph, label) in buttons {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(button_w, button_h), Sense::hover());
        let response = ui.interact(rect, egui::Id::new(("caption", cmd)), Sense::click()).on_hover_cursor(CursorIcon::Default).on_hover_text(label);
        let hovered = response.hovered();
        let bg = if hovered { if cmd == "window.close" { egui::Color32::from_rgb(196, 43, 28) } else { t.hover } } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, 0.0, bg);
        let color = if hovered && cmd == "window.close" { Color32::WHITE } else { t.text };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(13.0), color);
        if response.clicked() {
            let params = if cmd == "window.maximize" { serde_json::json!({"on": !maximized}) } else { serde_json::Value::Null };
            let ctx = ui.ctx().clone();
            app.dispatch(cmd, &params, &ctx);
        }
    }
    ui.spacing_mut().item_spacing.x = spacing;
}

pub fn status_bar(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new().fill(t.status_bar).stroke(egui::Stroke::new(1.0_f32, t.divider)).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let status = if let Some(load) = &app.loading { load.progress.clone() }
                else if app.pivot_computing.is_some() { "Computing pivot...".to_string() }
                else if app.exporting.is_some() { app.exporting.as_ref().map(|e| e.progress.clone()).unwrap_or_else(|| "Exporting...".to_string()) }
                else { app.status.clone() };
            ui.label(egui::RichText::new(status).color(t.text).size(12.0));
            if app.loading.is_some() { ui.add_space(8.0); ui.spinner(); if ui.small_button("Cancel").clicked() { app.cancel_load(); } }
            if app.pivot_computing.is_some() { ui.add_space(8.0); ui.spinner(); if ui.small_button("Cancel pivot").clicked() { app.cancel_pivot(); } }
            if app.exporting.is_some() { ui.add_space(8.0); ui.spinner(); if ui.small_button("Cancel export").clicked() { app.cancel_export(); } }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let src = if let Some(db) = &app.db_source {
                    match db {
                        crate::app::DbSource::Sqlite { table, .. } => format!("DB:sqlite/{table}"),
                        crate::app::DbSource::MySql(p) => format!("DB:mysql(disabled)/{}/{}", p.database, p.table),
                    }
                } else { "File".to_string() };
                let sel = match &app.selected_columns { None => "all".to_string(), Some(s) => format!("{}/{}", s.len(), app.col_count()) };
                ui.label(egui::RichText::new(format!("{} | {} sheets | cols {} | {} | rows {}", src, app.worksheets.len(), sel, app.brightness.label(), app.row_count())).color(t.text_muted).size(12.0));
            });
        });
    });
}
