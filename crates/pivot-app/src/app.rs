//! Application state and egui frame/update (DesignCraft `app.rs`/`DesignApp` pattern, simplified).

use crate::{chrome, dock, menus};
use pivot_theme::{apply, Brightness, Tokens};
use serde_json::Value;

pub struct PivotApp {
    pub brightness: Brightness,
    pub status: String,
    brightness_applied: Option<Brightness>,
    exit_requested: bool,
}

impl PivotApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let brightness = Brightness::default();
        let tokens = Tokens::for_brightness(brightness);
        apply(&cc.egui_ctx, &tokens);
        Self {
            brightness,
            status: "Ready. Phase 0 shell - import data and pivot features arrive in later phases.".to_string(),
            brightness_applied: Some(brightness),
            exit_requested: false,
        }
    }

    fn ensure_theme(&mut self, ctx: &egui::Context) {
        if self.brightness_applied != Some(self.brightness) {
            let tokens = Tokens::for_brightness(self.brightness);
            apply(ctx, &tokens);
            self.brightness_applied = Some(self.brightness);
        }
    }

    pub fn dispatch(&mut self, id: &str, params: &Value, ctx: &egui::Context) {
        match id {
            "app.exit" => {
                self.exit_requested = true;
            }
            "view.brightness" => {
                if let Some(mode) = params.get("mode").and_then(|v| v.as_str()) {
                    if let Some(b) = Brightness::parse(mode) {
                        self.brightness = b;
                        self.status = format!("Brightness set to {}.", b.label());
                        let tokens = Tokens::for_brightness(b);
                        apply(ctx, &tokens);
                        self.brightness_applied = Some(b);
                    } else {
                        self.status = format!("Unknown brightness mode: {mode}.");
                    }
                }
            }
            "view.reset_layout" => {
                self.status = "Layout reset (placeholder; panel state reset in a future phase).".to_string();
            }
            "help.about" => {
                self.status = "Pivot Studio (Phase 0) - a polished pivot-table desktop app. See PIVOT_STUDIO_PLAN.md.".to_string();
            }
            _ => {
                self.status = format!("Command '{id}' is a placeholder (not implemented in Phase 0).");
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::menu::bar(ui, |ui| {
            for (title, items) in menus::menu_tree() {
                ui.menu_button(title, |ui| {
                    Self::render_menu_items(ui, &items, &ctx, self);
                });
            }
        });
    }

    fn render_menu_items(ui: &mut egui::Ui, items: &[menus::Item], ctx: &egui::Context, app: &mut PivotApp) {
        for item in items {
            match item {
                menus::Item::Sep => {
                    ui.separator();
                }
                menus::Item::Sub(label, children) => {
                    ui.menu_button(label, |ui| {
                        Self::render_menu_items(ui, children, ctx, app);
                    });
                }
                menus::Item::Cmd { label, id, params, shortcut } => {
                    let mut text = label.clone();
                    if let Some(sc) = shortcut {
                        text = format!("{label}  ({sc})");
                    }
                    let checked = if id == "view.brightness" {
                        params
                            .get("mode")
                            .and_then(|v| v.as_str())
                            .and_then(Brightness::parse)
                            .map(|b| b == app.brightness)
                    } else {
                        None
                    };
                    let response = if let Some(is_checked) = checked {
                        ui.selectable_label(is_checked, text)
                    } else {
                        ui.button(text)
                    };
                    if response.clicked() {
                        app.dispatch(id, params, ctx);
                        ui.close_menu();
                    }
                }
            }
        }
    }
}

impl eframe::App for PivotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_theme(ctx);
        if self.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.exit_requested = false;
        }

        let t = Tokens::get(ctx);
        egui::TopBottomPanel::top("app_bar")
            .frame(egui::Frame::new().fill(t.app_bar))
            .show(ctx, |ui| {
                chrome::app_bar(self, ui);
                egui::Frame::new()
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(1.0_f32, t.divider))
                    .inner_margin(egui::Margin::symmetric(6, 0))
                    .show(ui, |ui| {
                        self.menu_bar(ui);
                    });
            });

        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::new().fill(t.status_bar))
            .show(ctx, |ui| {
                chrome::status_bar(self, ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.canvas))
            .show(ctx, |ui| {
                dock::show(self, ui);
            });
    }
}
