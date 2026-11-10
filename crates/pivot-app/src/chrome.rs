//! App bar, stage ribbon, activity bar, and status bar (DERIVA Studio chrome pattern).

use crate::app::{ActivityTab, PivotApp};
use crate::icons::*;
use egui::{Color32, CursorIcon, RichText, Sense, Stroke, Vec2};
use pivot_theme::{Brightness, Tokens};

pub fn app_bar(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::new()
        .fill(t.app_bar)
        .stroke(egui::Stroke::new(1.0_f32, t.border))
        .inner_margin(egui::Margin::symmetric(10, 4));
    frame.show(ui, |ui| {
        let height = 30.0;
        ui.set_min_height(height);
        ui.horizontal_centered(|ui| {
            // 1. Brand Badge: [P] PIVOT STUDIO
            let logo_frame = egui::Frame::NONE
                .fill(t.badge_bg)
                .stroke(Stroke::new(1.0_f32, t.border))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(7, 3));
            let logo_rect = logo_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(5.0, 0.0);
                    draw_svg(ui, "bytes://app-logo.svg", ICON_APP_LOGO, Vec2::new(13.0, 13.0), Color32::WHITE);
                    ui.colored_label(t.text_strong, RichText::new("PIVOT STUDIO").strong().size(12.0));
                    ui.colored_label(t.text_muted, RichText::new("v0.2.2").size(10.5));
                });
            });
            let logo_resp = ui.interact(logo_rect.response.rect, egui::Id::new("brand_home"), Sense::click()).on_hover_text("Home / Welcome");
            if logo_resp.clicked() {
                let ctx = ui.ctx().clone();
                app.dispatch("nav.home", &serde_json::Value::Null, &ctx);
            }
            ui.add_space(8.0);

            // 2. Presets Dropdown
            egui::ComboBox::from_id_salt("top_presets")
                .selected_text(RichText::new("Presets").size(11.5).color(t.text))
                .width(80.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(app.show_left_panel, "Default Studio").clicked() {
                        app.show_left_panel = true;
                    }
                    if ui.selectable_label(!app.show_left_panel, "Maximized Canvas").clicked() {
                        app.show_left_panel = false;
                    }
                });

            ui.add_space(4.0);

            // 3. Theme Selector Dropdown (e.g. Deriva Dark ˅)
            let mut current_brightness = app.brightness;
            egui::ComboBox::from_id_salt("top_theme")
                .selected_text(RichText::new(current_brightness.label()).size(11.5).color(t.text_strong))
                .width(115.0)
                .show_ui(ui, |ui| {
                    for b in Brightness::ALL {
                        let resp = ui.selectable_value(&mut current_brightness, b, b.label());
                        if resp.clicked() || resp.changed() {
                            app.set_brightness(b, ui.ctx());
                        }
                    }
                });

            ui.add_space(8.0);

            // 4. Engine Indicator Pill
            let engine_label = if app.db_source.is_some() { "SQLite Pushdown" } else { "In-Memory" };
            egui::Frame::NONE
                .fill(t.panel_alt)
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("Engine: {engine_label}")).size(10.5).color(t.text_muted));
                });

            // 5. Right controls & Window caption controls
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                caption_buttons(app, ui);

                // Window drag region
                let drag_w = 40.0;
                let (_, drag_resp) = ui.allocate_exact_size(egui::vec2(drag_w, height), Sense::click_and_drag());
                if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_resp.double_clicked() {
                    let on = !ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    let ctx = ui.ctx().clone();
                    app.dispatch("window.maximize", &serde_json::json!({"on": on}), &ctx);
                }

                ui.add_space(8.0);

                // Health / Error Status Badge (DERIVA style)
                let error_count = app.data_model_diagnostics.errors.len();
                let warning_count = app.data_model_diagnostics.warnings.len();
                if error_count > 0 {
                    let btn = ui.add(
                        egui::Button::new(RichText::new(format!("● {error_count} ERRORS")).size(11.0).strong().color(t.error_text))
                            .fill(t.error_bg)
                            .stroke(Stroke::new(1.0_f32, t.error_border))
                            .corner_radius(4.0)
                    ).on_hover_text("Click to toggle Compiler Diagnostics drawer");
                    if btn.clicked() {
                        app.diagnostics_open = !app.diagnostics_open;
                    }
                } else if warning_count > 0 {
                    let btn = ui.add(
                        egui::Button::new(RichText::new(format!("● {warning_count} WARNINGS")).size(11.0).strong().color(t.warning_text))
                            .fill(t.warning_bg)
                            .stroke(Stroke::new(1.0_f32, t.warning_border))
                            .corner_radius(4.0)
                    ).on_hover_text("Click to toggle Warnings drawer");
                    if btn.clicked() {
                        app.diagnostics_open = !app.diagnostics_open;
                    }
                } else {
                    let btn = ui.add(
                        egui::Button::new(RichText::new("● READY").size(11.0).strong().color(t.success_text))
                            .fill(t.success_bg)
                            .stroke(Stroke::new(1.0_f32, t.border))
                            .corner_radius(4.0)
                    ).on_hover_text("Data model and pivot specifications are clean");
                    if btn.clicked() {
                        app.diagnostics_open = !app.diagnostics_open;
                    }
                }

                ui.add_space(6.0);

                // Recompute Execution Button (DERIVA green ▶ RUN button style)
                let is_computing = app.pivot_computing.is_some();
                let run_btn = egui::Button::new(
                    RichText::new(if is_computing { "COMPUTING..." } else { "▶ RECOMPUTE" })
                        .size(11.5)
                        .strong()
                        .color(Color32::WHITE)
                )
                .fill(if is_computing { t.hover } else { Color32::from_rgb(16, 185, 129) })
                .corner_radius(4.0);

                if ui.add(run_btn).on_hover_text("Recompute active worksheet pivot table").clicked() {
                    app.request_pivot_recompute();
                }

                ui.add_space(4.0);

                // Quick Export Button (opens Export workspace tab)
                let export_btn = egui::Button::new(
                    RichText::new("⇡ EXPORT")
                        .size(11.5)
                        .strong()
                        .color(t.text_strong)
                )
                .fill(t.panel)
                .stroke(Stroke::new(1.0_f32, t.border))
                .corner_radius(4.0);

                if ui.add(export_btn).on_hover_text("Open Export Workspace panel (XLSX, HTML, PDF)").clicked() {
                    app.active_stage = 5;
                    app.on_home = false;
                    app.on_source_page = false;
                    app.activity_tab = ActivityTab::Export;
                    app.show_left_panel = true;
                }
            });
        });
    });
}

/// Horizontal stage ribbon below the top bar (DERIVA compiler stage pipeline).
pub fn stage_ribbon(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.panel_alt)
        .stroke(egui::Stroke::new(1.0_f32, t.border))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                let stages = [
                    (1, "1. Data Sources", ICON_DATABASE, "bytes://database.svg"),
                    (2, "2. Relations & Model", ICON_GIT_MERGE, "bytes://git-merge.svg"),
                    (3, "3. Pivot Layout", ICON_TABLE, "bytes://table.svg"),
                    (4, "4. Table Styling", ICON_PALETTE, "bytes://palette.svg"),
                    (5, "5. Export & Output", ICON_DOWNLOAD, "bytes://download.svg"),
                ];

                for (idx, label, bytes, uri) in stages {
                    let is_active = app.active_stage == idx;
                    let (bg, border_color, text_color, icon_color) = if is_active {
                        (t.panel, t.accent, t.text_strong, t.accent_cyan)
                    } else {
                        (Color32::TRANSPARENT, Color32::TRANSPARENT, t.text_muted, t.text_disabled)
                    };

                    let resp = egui::Frame::NONE
                        .fill(bg)
                        .stroke(if is_active { Stroke::new(1.2_f32, border_color) } else { Stroke::NONE })
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(5.0, 0.0);
                                draw_svg(ui, uri, bytes, Vec2::new(13.0, 13.0), icon_color);
                                ui.label(RichText::new(label).size(12.0).color(text_color).strong());
                            });
                        });

                    let click_resp = ui.interact(resp.response.rect, egui::Id::new(("stage_btn", idx)), Sense::click());
                    if click_resp.hovered() && !is_active {
                        ui.painter().rect_stroke(resp.response.rect, 4.0, Stroke::new(1.0_f32, t.border), egui::StrokeKind::Inside);
                    }
                    if click_resp.clicked() {
                        app.active_stage = idx;
                        app.on_home = false;
                        match idx {
                            1 | 2 => {
                                app.on_source_page = true;
                                if idx == 1 { app.activity_tab = ActivityTab::Files; }
                                else { app.activity_tab = ActivityTab::Model; }
                            }
                            3 => {
                                app.on_source_page = false;
                                app.activity_tab = ActivityTab::Layout;
                                app.show_left_panel = true;
                            }
                            4 => {
                                app.on_source_page = false;
                                app.activity_tab = ActivityTab::Themes;
                                app.show_left_panel = true;
                            }
                            5 => {
                                app.on_source_page = false;
                                app.activity_tab = ActivityTab::Export;
                                app.show_left_panel = true;
                            }
                            _ => {}
                        }
                    }
                }
            });
        });
}

/// Narrow 44px vertical Activity Bar on the far left (replaces top menus with icon menu).
pub fn activity_bar(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.app_bar)
        .stroke(egui::Stroke::new(1.0_f32, t.border))
        .inner_margin(egui::Margin::symmetric(4, 8))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

                let tabs = [
                    (ActivityTab::Files, "Files & Project Sources", ICON_FILES, "bytes://files.svg"),
                    (ActivityTab::Model, "Data Model & Lookup Relations", ICON_GIT_MERGE, "bytes://git-merge.svg"),
                    (ActivityTab::Layout, "Pivot Fields & Layout Builder", ICON_TABLE, "bytes://table.svg"),
                    (ActivityTab::Themes, "Table Theme & Styling Roles", ICON_PALETTE, "bytes://palette.svg"),
                    (ActivityTab::Export, "Export Table (XLSX, PDF, HTML)", ICON_DOWNLOAD, "bytes://download.svg"),
                ];

                for (tab, tip, bytes, uri) in tabs {
                    let is_active = app.activity_tab == tab && app.show_left_panel;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(36.0, 36.0), Sense::click());
                    let resp = resp.on_hover_text(tip);

                    let bg = if is_active {
                        t.panel_alt
                    } else if resp.hovered() {
                        t.hover
                    } else {
                        Color32::TRANSPARENT
                    };

                    ui.painter().rect_filled(rect, 5.0, bg);

                    // Active left indicator line (VS Code / Deriva style)
                    if is_active {
                        let ind = egui::Rect::from_min_size(egui::pos2(rect.min.x, rect.min.y + 4.0), Vec2::new(2.5, rect.height() - 8.0));
                        ui.painter().rect_filled(ind, 1.0, t.accent_cyan);
                    }

                    let icon_color = if is_active {
                        t.text_strong
                    } else if resp.hovered() {
                        t.text
                    } else {
                        t.text_muted
                    };

                    let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::new(18.0, 18.0));
                    let img = egui::Image::from_bytes(uri, bytes)
                        .fit_to_exact_size(Vec2::new(18.0, 18.0))
                        .tint(icon_color);
                    img.paint_at(ui, icon_rect);

                    if resp.clicked() {
                        if app.activity_tab == tab && app.show_left_panel {
                            app.show_left_panel = false;
                        } else {
                            app.activity_tab = tab;
                            app.show_left_panel = true;
                            match tab {
                                ActivityTab::Files => {
                                    app.active_stage = 1;
                                }
                                ActivityTab::Model => {
                                    app.active_stage = 2;
                                }
                                ActivityTab::Layout => {
                                    app.active_stage = 3;
                                    app.on_source_page = false;
                                }
                                ActivityTab::Themes => {
                                    app.active_stage = 4;
                                    app.on_source_page = false;
                                }
                                ActivityTab::Export => {
                                    app.active_stage = 5;
                                    app.on_source_page = false;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Settings pinned at bottom
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    let is_active = app.activity_tab == ActivityTab::Settings && app.show_left_panel;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(36.0, 36.0), Sense::click());
                    let resp = resp.on_hover_text("Settings & Preferences");
                    let bg = if is_active { t.panel_alt } else if resp.hovered() { t.hover } else { Color32::TRANSPARENT };
                    ui.painter().rect_filled(rect, 5.0, bg);
                    let icon_color = if is_active { t.text_strong } else { t.text_muted };
                    let img = egui::Image::from_bytes("bytes://settings.svg", ICON_SETTINGS)
                        .fit_to_exact_size(Vec2::new(18.0, 18.0))
                        .tint(icon_color);
                    img.paint_at(ui, egui::Rect::from_center_size(rect.center(), Vec2::new(18.0, 18.0)));

                    if resp.clicked() {
                        if app.activity_tab == ActivityTab::Settings && app.show_left_panel {
                            app.show_left_panel = false;
                        } else {
                            app.activity_tab = ActivityTab::Settings;
                            app.show_left_panel = true;
                        }
                    }
                });
            });
        });
}

fn caption_buttons(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    let button_w = 42.0;
    let button_h = 24.0;
    let spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 0.0;
    let buttons = [
        ("window.close", "Close"),
        ("window.maximize", if maximized { "Restore" } else { "Maximize" }),
        ("window.minimize", "Minimize"),
    ];
    for (cmd, label) in buttons {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(button_w, button_h), Sense::hover());
        let response = ui.interact(rect, egui::Id::new(("caption", cmd)), Sense::click()).on_hover_cursor(CursorIcon::Default).on_hover_text(label);
        let hovered = response.hovered();
        let bg = if hovered { if cmd == "window.close" { egui::Color32::from_rgb(196, 43, 28) } else { t.hover } } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, 3.0, bg);
        let color = if hovered && cmd == "window.close" { Color32::WHITE } else { t.text };
        paint_caption_glyph(ui.painter(), rect.center(), cmd, maximized, color);
        if response.clicked() {
            let params = if cmd == "window.maximize" { serde_json::json!({"on": !maximized}) } else { serde_json::Value::Null };
            let ctx = ui.ctx().clone();
            app.dispatch(cmd, &params, &ctx);
        }
    }
    ui.spacing_mut().item_spacing.x = spacing;
}

fn paint_caption_glyph(painter: &egui::Painter, c: egui::Pos2, cmd: &str, maximized: bool, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    match cmd {
        "window.close" => {
            let s = 4.0;
            painter.line_segment([egui::pos2(c.x - s, c.y - s), egui::pos2(c.x + s, c.y + s)], stroke);
            painter.line_segment([egui::pos2(c.x - s, c.y + s), egui::pos2(c.x + s, c.y - s)], stroke);
        }
        "window.maximize" => {
            let s = 4.0;
            if maximized {
                let back = egui::Rect::from_min_size(egui::pos2(c.x - s, c.y - s - 1.0), egui::vec2(s * 2.0, s * 2.0));
                let front = egui::Rect::from_min_size(egui::pos2(c.x - s + 2.5, c.y - s + 2.0), egui::vec2(s * 2.0, s * 2.0));
                painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Middle);
                painter.line_segment([back.left_bottom(), back.left_top()], stroke);
                painter.line_segment([back.left_top(), back.right_top()], stroke);
                painter.line_segment([back.right_top(), front.right_top()], stroke);
            } else {
                let square = egui::Rect::from_center_size(c, egui::vec2(s * 2.2, s * 2.2));
                painter.rect_stroke(square, 1.0, stroke, egui::StrokeKind::Middle);
            }
        }
        _ => {
            painter.line_segment([egui::pos2(c.x - 4.5, c.y + 3.0), egui::pos2(c.x + 4.5, c.y + 3.0)], stroke);
        }
    }
}

pub fn status_bar(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.status_bar)
        .stroke(egui::Stroke::new(1.0_f32, t.divider))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let status = if let Some(load) = &app.loading {
                    load.progress.clone()
                } else if app.pivot_computing.is_some() {
                    "Computing pivot table...".to_string()
                } else if app.exporting.is_some() {
                    app.exporting.as_ref().map(|e| e.progress.clone()).unwrap_or_else(|| "Exporting...".to_string())
                } else {
                    app.status.clone()
                };
                ui.add(egui::Label::new(egui::RichText::new(status).color(t.text).size(11.5)).truncate());

                if app.loading.is_some() { ui.add_space(8.0); ui.spinner(); if ui.small_button("Cancel").clicked() { app.cancel_load(); } }
                if app.pivot_computing.is_some() { ui.add_space(8.0); ui.spinner(); if ui.small_button("Cancel pivot").clicked() { app.cancel_pivot(); } }
                if app.exporting.is_some() { ui.add_space(8.0); ui.spinner(); if ui.small_button("Cancel export").clicked() { app.cancel_export(); } }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total_rows = app.row_count();
                    let total_sheets = app.worksheets.len();
                    let src = if let Some(db) = &app.db_source {
                        match db {
                            crate::app::DbSource::Sqlite { table, .. } => format!("sqlite/{table}"),
                            crate::app::DbSource::MySql(p) => format!("mysql/{}/{}", p.database, p.table),
                        }
                    } else { "File".to_string() };

                    let err_count = app.data_model_diagnostics.errors.len();
                    if err_count > 0 {
                        ui.colored_label(t.error_text, RichText::new(format!("● {err_count} ERRORS")).size(11.0).strong());
                        ui.separator();
                    } else {
                        ui.colored_label(t.success_text, RichText::new("● 0 ERRORS").size(11.0).strong());
                        ui.separator();
                    }

                    let meta = format!("{src} · {total_sheets} sheets · {total_rows} rows · {}", app.brightness.label());
                    ui.label(egui::RichText::new(meta).color(t.text_muted).size(11.0));
                });
            });
        });
}
