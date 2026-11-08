//! Application state and egui frame/update (DesignCraft `app.rs`/`DesignApp` pattern, simplified).

use crate::{chrome, dock, menus};
use pivot_core::{DataTable, DataType};
use pivot_data::FileKind;
use pivot_theme::{apply, Brightness, Tokens};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;

pub enum LoadMsg {
    Progress(String),
    Done(Result<LoadedTable, String>),
}

pub struct LoadedTable {
    pub name: String,
    pub table: DataTable,
}

pub struct LoadState {
    pub rx: Receiver<LoadMsg>,
    pub progress: String,
    pub cancel: Arc<AtomicBool>,
}

pub struct PivotApp {
    pub brightness: Brightness,
    pub status: String,
    brightness_applied: Option<Brightness>,
    exit_requested: bool,
    pub on_home: bool,
    pub table: Option<DataTable>,
    pub source_name: Option<String>,
    pub loading: Option<LoadState>,
}

impl PivotApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let brightness = Brightness::default();
        let tokens = Tokens::for_brightness(brightness);
        apply(&cc.egui_ctx, &tokens);
        Self {
            brightness,
            status: "Home - open a saved project or import data to start.".to_string(),
            brightness_applied: Some(brightness),
            exit_requested: false,
            on_home: true,
            table: None,
            source_name: None,
            loading: None,
        }
    }

    fn ensure_theme(&mut self, ctx: &egui::Context) {
        if self.brightness_applied != Some(self.brightness) {
            let tokens = Tokens::for_brightness(self.brightness);
            apply(ctx, &tokens);
            self.brightness_applied = Some(self.brightness);
        }
    }

    pub fn row_count(&self) -> usize {
        self.table.as_ref().map(|t| t.row_count()).unwrap_or(0)
    }

    pub fn col_count(&self) -> usize {
        self.table.as_ref().map(|t| t.column_count()).unwrap_or(0)
    }

    pub fn poll_loading(&mut self, ctx: &egui::Context) {
        let mut finished: Option<Result<LoadedTable, String>> = None;
        if let Some(load) = &mut self.loading {
            while let Ok(msg) = load.rx.try_recv() {
                match msg {
                    LoadMsg::Progress(s) => {
                        load.progress = s.clone();
                        self.status = s;
                    }
                    LoadMsg::Done(res) => {
                        finished = Some(res);
                        break;
                    }
                }
            }
        }
        if let Some(res) = finished {
            self.loading = None;
            match res {
                Ok(loaded) => {
                    self.status = format!(
                        "Loaded '{}' ({} rows, {} cols).",
                        loaded.name,
                        loaded.table.row_count(),
                        loaded.table.column_count()
                    );
                    self.source_name = Some(loaded.name);
                    self.table = Some(loaded.table);
                    self.on_home = false;
                }
                Err(e) => {
                    self.status = format!("Load failed: {e}");
                }
            }
            ctx.request_repaint();
        }
    }

    fn start_load(&mut self, path: PathBuf, kind: FileKind) {
        if self.loading.is_some() {
            self.status = "A load is already in progress.".to_string();
            return;
        }
        let (tx, rx) = mpsc::channel::<LoadMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        self.status = format!("Loading {name}...");
        self.loading = Some(LoadState {
            rx,
            progress: self.status.clone(),
            cancel,
        });
        let ctx = None::<egui::Context>; // repaint driven by poll via request_repaint_after
        thread::spawn(move || {
            let _ = tx.send(LoadMsg::Progress(format!("Reading {name}...")));
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(LoadMsg::Done(Err("Cancelled.".to_string())));
                return;
            }
            let result = pivot_data::load_file(&path, kind);
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(LoadMsg::Done(Err("Cancelled.".to_string())));
                return;
            }
            match result {
                Ok(mut table) => {
                    if table.source.is_none() {
                        table.source = Some(path.to_string_lossy().into_owned());
                    }
                    let _ = tx.send(LoadMsg::Progress(format!(
                        "Loaded {} rows.",
                        table.row_count()
                    )));
                    let _ = tx.send(LoadMsg::Done(Ok(LoadedTable { name, table })));
                }
                Err(e) => {
                    let _ = tx.send(LoadMsg::Done(Err(e.to_string())));
                }
            }
            let _ = ctx; // silence
        });
    }

    fn pick_and_load(&mut self, kind: FileKind) {
        let (title, filters): (&str, Vec<(&str, &[&str])>) = match kind {
            FileKind::Csv => ("Import CSV/TSV", vec![("CSV/TSV", &["csv", "tsv", "txt"][..])]),
            FileKind::Tsv => ("Import TSV", vec![("TSV", &["tsv", "txt"][..])]),
            FileKind::Xlsx => ("Import Excel", vec![("Excel", &["xlsx", "xls", "xlsm"][..])]),
            FileKind::Dbf => ("Import DBF", vec![("DBF", &["dbf"][..])]),
        };
        let mut dialog = rfd::FileDialog::new().set_title(title);
        for (name, exts) in filters {
            dialog = dialog.add_filter(name, exts);
        }
        if let Some(path) = dialog.pick_file() {
            self.start_load(path, kind);
        } else {
            self.status = "Import cancelled.".to_string();
        }
    }

    pub fn cancel_load(&mut self) {
        if let Some(load) = &self.loading {
            load.cancel.store(true, Ordering::Relaxed);
            self.status = "Cancelling load...".to_string();
        }
    }

    pub fn reinterpret_column(&mut self, col_idx: usize, target: DataType) {
        if let Some(table) = &mut self.table {
            if let Some(col) = table.columns.get_mut(col_idx) {
                col.override_type = Some(target);
            }
            table.reinterpret_column(col_idx);
            self.status = format!(
                "Column type updated ({} rows).",
                table.row_count()
            );
        }
    }

    pub fn dispatch(&mut self, id: &str, params: &Value, ctx: &egui::Context) {
        match id {
            "app.exit" | "window.close" => {
                self.exit_requested = true;
            }
            "window.minimize" => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            "window.maximize" => {
                let on = params
                    .get("on")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| !ctx.input(|i| i.viewport().maximized.unwrap_or(false)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(on));
            }
            "nav.home" => {
                self.on_home = true;
                self.status = "Home - open a saved project or import data to start.".to_string();
            }
            "nav.worksheet" => {
                self.on_home = false;
                self.status = if let Some(name) = &self.source_name {
                    format!("Worksheet - {name} ({} rows, {} cols).", self.row_count(), self.col_count())
                } else {
                    "Worksheet - import data to populate fields (Phase 1).".to_string()
                };
            }
            "file.import" => {
                // Generic import defaults to CSV picker; home cards dispatch specific kinds.
                self.pick_and_load(FileKind::Csv);
            }
            "file.import.csv" => self.pick_and_load(FileKind::Csv),
            "file.import.xlsx" => self.pick_and_load(FileKind::Xlsx),
            "file.import.dbf" => self.pick_and_load(FileKind::Dbf),
            "file.open" => {
                self.on_home = false;
                self.status = "Open Workspace (.pivot) is a placeholder (Phase 7).".to_string();
            }
            "data.connect" => {
                self.on_home = false;
                self.status = "Database connectors are a placeholder (Phase 5).".to_string();
            }
            "data.cancel_load" => self.cancel_load(),
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
                self.status = "Pivot Studio (Phase 1) - data ingest foundation. See PIVOT_STUDIO_PLAN.md.".to_string();
            }
            _ => {
                self.status = format!("Command '{id}' is a placeholder (not implemented in Phase 1).");
            }
        }
    }

    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
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
        self.poll_loading(ctx);
        if self.loading.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.exit_requested = false;
        }

        let t = Tokens::get(ctx);
        egui::TopBottomPanel::top("app_bar")
            .frame(egui::Frame::new().fill(t.app_bar))
            .show(ctx, |ui| {
                chrome::app_bar(self, ui);
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
