//! Docked, resizable panel layout (DesignCraft docking feel).
//! Home, Data Model / Source Data, and multi-worksheet pivot editor.

use crate::app::PivotApp;
use egui::{RichText, Sense, Vec2};
use pivot_core::{Aggregate, DataType, FilterOp, FooterBandSpec, ThemedTableModel, Value as CoreValue};
use pivot_theme::Tokens;
use serde_json::Value;

pub fn show(app: &mut PivotApp, ui: &mut egui::Ui) {
    if app.on_home {
        home_page(app, ui);
    } else if app.on_source_page {
        source_page(app, ui);
    } else {
        worksheet(app, ui);
    }
}

fn home_page(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(28.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(900.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.label(RichText::new("Pivot Studio").strong().size(30.0).color(t.text_strong));
                ui.add_space(4.0);
                ui.label(RichText::new("Open a saved project, import a file, or connect to a database. After loading data, review/select source columns, then build pivots across multiple worksheets.").size(14.0).color(t.text_muted));
                ui.add_space(26.0);
                if app.loading.is_some() {
                    egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.accent)).inner_margin(egui::Margin::same(10)).corner_radius(6.0).show(ui, |ui| {
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
                    action_card(ui, app, &t, "Connect SQLite", "Connect to a SQLite database (pushdown).", "data.connect");
                    if app.table.is_some() {
                        action_card(ui, app, &t, "Source Data", "Select which columns are processed in pivots.", "nav.source");
                        action_card(ui, app, &t, "Open Worksheet", "Build pivots in worksheets.", "nav.worksheet");
                    }
                });
                ui.add_space(24.0);
                egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(12)).corner_radius(6.0).show(ui, |ui| {
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
    if response.clicked() { let ctx = ui.ctx().clone(); app.dispatch(command, &Value::Null, &ctx); }
}

fn source_page(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new("Data Model / Source Data").strong().size(20.0).color(t.text_strong));
        ui.add_space(12.0);
        if let Some(name) = &app.source_name { ui.label(RichText::new(name).color(t.text_muted)); }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Open Worksheet ->").clicked() { let ctx = ui.ctx().clone(); app.dispatch("nav.worksheet", &Value::Null, &ctx); }
            if ui.button("Home").clicked() { let ctx = ui.ctx().clone(); app.dispatch("nav.home", &Value::Null, &ctx); }
        });
    });
    ui.label(RichText::new("1) Add datasets  2) pick the main table  3) relate a code/id column to a label column in another dataset  4) Resolve. Lookup relations never change the main table's row count or sums.").color(t.text_muted).size(12.0));
    ui.add_space(8.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Data model / multi-source section
        egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(10)).corner_radius(6.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Datasets").strong().size(15.0).color(t.text_strong));
                let mut add_file_path: Option<std::path::PathBuf> = None;
                let mut add_sqlite_path: Option<std::path::PathBuf> = None;
                if ui.button("Add file dataset...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().set_title("Add dataset (CSV/XLSX/DBF)").add_filter("Data", &["csv","tsv","txt","xlsx","xls","xlsm","dbf"]).pick_file() {
                        if app.add_dataset_sheet.is_empty() { app.add_dataset_sheet = "Sheet1".to_string(); }
                        add_file_path = Some(path);
                    }
                }
                if ui.button("Add SQLite dataset...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().set_title("Select SQLite database").add_filter("SQLite", &["db","sqlite","sqlite3"]).pick_file() {
                        add_sqlite_path = Some(path);
                    }
                }
                // XLSX sheet picker (shown when the picked file is XLSX and has multiple sheets)
                if let Some(path) = &add_file_path {
                    let is_xlsx = path.extension().and_then(|e| e.to_str()).map(|e| matches!(e.to_ascii_lowercase().as_str(), "xlsx"|"xls"|"xlsm")).unwrap_or(false);
                    if is_xlsx {
                        match pivot_data::list_sheets(path) {
                            Ok(sheets) if sheets.len() > 1 => {
                                if app.add_dataset_sheet.is_empty() || !sheets.iter().any(|s| s == &app.add_dataset_sheet) { app.add_dataset_sheet = sheets[0].clone(); }
                                let mut chosen = app.add_dataset_sheet.clone();
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("XLSX sheet:").color(t.text_muted).size(11.0));
                                    egui::ComboBox::from_id_salt("add_dataset_sheet").selected_text(chosen.clone()).show_ui(ui, |ui| {
                                        for s in &sheets { ui.selectable_value(&mut chosen, s.clone(), s); }
                                    });
                                });
                                app.add_dataset_sheet = chosen;
                            }
                            Ok(sheets) if sheets.len() == 1 => { app.add_dataset_sheet = sheets[0].clone(); }
                            _ => {}
                        }
                    }
                }
                // SQLite table picker (shown when tables available)
                let sqlite_tables: Vec<String>;
                if let Some(path) = &add_sqlite_path {
                    sqlite_tables = pivot_data::list_sqlite_tables(path).unwrap_or_default();
                    if sqlite_tables.is_empty() { app.status = "SQLite database has no tables.".to_string(); }
                    else {
                        if app.add_dataset_table.is_empty() || !sqlite_tables.iter().any(|t| t == &app.add_dataset_table) { app.add_dataset_table = sqlite_tables[0].clone(); }
                        let mut chosen = app.add_dataset_table.clone();
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("SQLite table:").color(t.text_muted).size(11.0));
                            egui::ComboBox::from_id_salt("add_dataset_table").selected_text(chosen.clone()).show_ui(ui, |ui| {
                                for tname in &sqlite_tables { ui.selectable_value(&mut chosen, tname.clone(), tname); }
                            });
                        });
                        app.add_dataset_table = chosen;
                    }
                }
                if let Some(path) = add_file_path {
                    let is_xlsx = path.extension().and_then(|e| e.to_str()).map(|e| matches!(e.to_ascii_lowercase().as_str(), "xlsx"|"xls"|"xlsm")).unwrap_or(false);
                    if is_xlsx {
                        let sheet = app.add_dataset_sheet.clone();
                        app.add_dataset_from_file_with_sheet(path, if sheet.trim().is_empty() { None } else { Some(sheet) });
                    } else {
                        app.add_dataset_from_file(path);
                    }
                }
                if let (Some(path), table) = (add_sqlite_path, app.add_dataset_table.clone()) {
                    if !table.trim().is_empty() {
                        let label = table.clone();
                        app.add_dataset_sqlite(path, table);
                        app.status = format!("Added SQLite dataset using table '{label}'. Resolve Data Model to use it in pivots.");
                    }
                }
                let resolve_enabled = !app.data_model_resolving;
                if ui.add_enabled(resolve_enabled, egui::Button::new(if app.data_model_resolving { "Resolving..." } else { "Resolve Data Model" })).clicked() {
                    app.request_resolve_model();
                }
            });
            ui.add_space(4.0);
            if app.data_model.datasets.is_empty() {
                ui.label(RichText::new("No datasets yet. Add a file (CSV/XLSX/DBF) or a SQLite table above. A single imported file works without a model - this page is for combining several.").color(t.text_muted));
            } else {
                egui::Grid::new("datasets_grid").striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Main").strong().color(t.text_strong));
                    ui.label(RichText::new("Dataset").strong().color(t.text_strong));
                    ui.label(RichText::new("Source").strong().color(t.text_strong));
                    ui.label(RichText::new("").strong().color(t.text_strong));
                    ui.end_row();
                    let mut remove: Option<usize> = None;
                    let mut set_base: Option<String> = None;
                    for (i, ds) in app.data_model.datasets.iter().enumerate() {
                        let full_src = match &ds.source {
                            pivot_data::WorkspaceSource::File { path, file_kind, sheet } => {
                                let k = file_kind.clone().unwrap_or_else(|| "file".into());
                                let sh = sheet.as_deref().filter(|s| !s.trim().is_empty()).map(|s| format!("#{s}")).unwrap_or_default();
                                format!("{k}{sh}:{path}")
                            }
                            pivot_data::WorkspaceSource::Sqlite { path, table } => format!("sqlite:{path}:{table}"),
                            pivot_data::WorkspaceSource::MySql { profile } => format!("mysql:{}.{}", profile.database, profile.table),
                            pivot_data::WorkspaceSource::None => "(none)".to_string(),
                        };
                        // Short label (file name [+sheet] / SQLite table); full path on hover.
                        let short_src = match &ds.source {
                            pivot_data::WorkspaceSource::Sqlite { table, .. } => format!("SQLite table: {table}"),
                            _ => {
                                let (kind, path) = full_src.split_once(':').unwrap_or(("", full_src.as_str()));
                                let sh = kind.split('#').nth(1).map(|s| format!("  [{s}]")).unwrap_or_default();
                                let name = std::path::Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or(path);
                                format!("{name}{sh}")
                            }
                        };
                        let is_base = app.data_model.base_dataset_id.as_deref() == Some(ds.id.as_str());
                        let col_count = app.dataset_schema_cache.get(&ds.id).map(|c| c.len());
                        let alias = if let Some(n) = col_count { format!("{}  ({n} cols)", ds.alias) } else { ds.alias.clone() };
                        if ui.radio(is_base, "").on_hover_text("Use as main (processed) table").clicked() && !is_base { set_base = Some(ds.id.clone()); }
                        ui.label(RichText::new(alias).color(if is_base { t.text_strong } else { t.text }).size(13.0));
                        ui.label(RichText::new(short_src).color(t.text_muted).size(11.5)).on_hover_text(full_src.clone());
                        ui.horizontal(|ui| { if ui.small_button("Remove").clicked() { remove = Some(i); } });
                        ui.end_row();
                    }
                    if let Some(id) = set_base {
                        app.data_model.base_dataset_id = Some(id);
                        app.data_model.relations.clear();
                        app.clear_relation_draft();
                        for ws in &mut app.worksheets { ws.model = None; }
                    }
                    if let Some(i) = remove {
                        let removed = app.data_model.datasets.remove(i);
                        app.dataset_schema_cache.remove(&removed.id);
                        app.data_model.relations.retain(|r| r.left_dataset_id != removed.id && r.right_dataset_id != removed.id);
                        if app.data_model.base_dataset_id.as_deref() == Some(removed.id.as_str()) {
                            app.data_model.base_dataset_id = app.data_model.datasets.first().map(|d| d.id.clone());
                        }
                        if app.rel_base_dataset == removed.id || app.rel_lookup_dataset == removed.id { app.clear_relation_draft(); }
                        for ws in &mut app.worksheets { ws.model = None; }
                        app.table = None;
                    }
                });
                // Keep column schemas warm so the relation pickers never need a "load columns" step.
                if app.preview_rx.is_none() {
                    if let Some(missing) = app.data_model.datasets.iter().find(|d| !app.dataset_schema_cache.contains_key(&d.id)).map(|d| d.id.clone()) {
                        app.load_dataset_schema(&missing);
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Reading columns...").color(t.text_muted).size(11.5));
                        if ui.small_button("Cancel").clicked() { app.cancel_preview(); }
                    });
                }
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(RichText::new("Related table (add columns from another dataset)").strong().size(15.0).color(t.text_strong));
                ui.label(RichText::new("Main table keeps every row and every sum: the key column is matched to the related table's key, and the checked columns are copied in as extra columns. Unmatched keys stay empty.").color(t.text_muted).size(11.5));
                if !app.data_model.relations.is_empty() {
                    let mut remove_id: Option<String> = None;
                    egui::Grid::new("relations_list").striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                        for rel in &app.data_model.relations {
                            let base_alias = app.data_model.datasets.iter().find(|d| d.id == rel.left_dataset_id).map(|d| d.alias.as_str()).unwrap_or(rel.left_dataset_id.as_str());
                            let lookup_alias = app.data_model.datasets.iter().find(|d| d.id == rel.right_dataset_id).map(|d| d.alias.as_str()).unwrap_or(rel.right_dataset_id.as_str());
                            let label = rel.label_columns.as_ref().map(|v| v.join(", ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "(all)".to_string());
                            ui.label(RichText::new(format!("{} . {}  ->  {}", base_alias, rel.left_column, lookup_alias)).color(t.text).size(12.5));
                            ui.label(RichText::new(format!("{}.{} = {}.{}   add: {}.{}", base_alias, rel.left_column, lookup_alias, rel.right_column, lookup_alias, label)).color(t.text_muted).size(11.5));
                            if ui.small_button("Remove").clicked() { remove_id = Some(rel.id.clone()); }
                            ui.end_row();
                        }
                    });
                    if let Some(id) = remove_id { app.remove_relation(&id); }
                }

                ui.add_space(8.0);
                let base_id = app.data_model.base_dataset_id.clone()
                    .filter(|id| app.data_model.datasets.iter().any(|d| &d.id == id))
                    .or_else(|| app.data_model.datasets.first().map(|d| d.id.clone()));
                let related: Vec<(String, String)> = match &base_id {
                    Some(base) => app.data_model.datasets.iter().filter(|d| &d.id != base).map(|d| (d.id.clone(), d.alias.clone())).collect(),
                    None => Vec::new(),
                };
                if related.is_empty() {
                    ui.label(RichText::new("Add a second dataset above, then pick a related table here.").color(t.text_muted).size(12.0));
                } else {
                    let base_id = base_id.expect("base id present when related is non-empty");
                    if app.rel_base_dataset != base_id || !related.iter().any(|(id, _)| id == &app.rel_lookup_dataset) {
                        app.rel_base_dataset = base_id.clone();
                        app.rel_lookup_dataset = related[0].0.clone();
                        app.rel_base_column.clear();
                        app.rel_lookup_column.clear();
                        app.rel_label_columns.clear();
                    }
                    app.rel_edit_id = None;
                    let base_cols = app.dataset_columns(&base_id).to_vec();
                    let related_cols = app.dataset_columns(&app.rel_lookup_dataset).to_vec();
                    let label_cols: Vec<String> = related_cols.iter().filter(|c| **c != app.rel_lookup_column).cloned().collect();
                    app.rel_label_columns.retain(|c| label_cols.contains(c));
                    let related_alias = related.iter().find(|(id, _)| id == &app.rel_lookup_dataset).map(|(_, a)| a.clone()).unwrap_or_default();
                    egui::Frame::new().fill(t.panel_alt).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(10)).corner_radius(6.0).show(ui, |ui| {
                        egui::Grid::new("relation_simple").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                            ui.label(RichText::new("Related table").color(t.text_strong));
                            egui::ComboBox::from_id_salt("rs_lookup_ds").width(260.0).selected_text(related_alias.clone()).show_ui(ui, |ui| {
                                for (id, alias) in &related {
                                    if ui.selectable_value(&mut app.rel_lookup_dataset, id.clone(), alias).changed() {
                                        app.rel_lookup_column.clear();
                                        app.rel_label_columns.clear();
                                    }
                                }
                            });
                            ui.end_row();
                            ui.label(RichText::new("Key in main table").color(t.text_strong));
                            if base_cols.is_empty() {
                                ui.label(RichText::new("loading columns...").color(t.text_muted));
                            } else {
                                egui::ComboBox::from_id_salt("rs_base_col").width(260.0).selected_text(if app.rel_base_column.is_empty() { "(pick column)".to_string() } else { app.rel_base_column.clone() }).show_ui(ui, |ui| {
                                    for c in &base_cols { ui.selectable_value(&mut app.rel_base_column, c.clone(), c.as_str()); }
                                });
                            }
                            ui.end_row();
                            ui.label(RichText::new("Same key in related").color(t.text_strong));
                            if related_cols.is_empty() {
                                ui.label(RichText::new("loading columns...").color(t.text_muted));
                            } else {
                                egui::ComboBox::from_id_salt("rs_lookup_col").width(260.0).selected_text(if app.rel_lookup_column.is_empty() { "(pick column)".to_string() } else { app.rel_lookup_column.clone() }).show_ui(ui, |ui| {
                                    for c in &related_cols {
                                        if ui.selectable_value(&mut app.rel_lookup_column, c.clone(), c.as_str()).changed() { app.rel_label_columns.clear(); }
                                    }
                                });
                            }
                            ui.end_row();
                            ui.label(RichText::new("Columns to add to main").color(t.text_strong));
                            if label_cols.is_empty() {
                                ui.label(RichText::new("(pick the related key first)").color(t.text_muted));
                            } else {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("Checked columns are appended to the main table (related key excluded).").color(t.text_muted).size(11.0));
                                    egui::ScrollArea::vertical().id_salt("rs_label_scroll").max_height(150.0).show(ui, |ui| {
                                        for c in &label_cols {
                                            let mut checked = app.rel_label_columns.contains(c);
                                            if ui.checkbox(&mut checked, c.as_str()).changed() {
                                                if checked { app.rel_label_columns.insert(c.clone()); } else { app.rel_label_columns.remove(c); }
                                            }
                                        }
                                    });
                                });
                            }
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        let ready = !app.rel_base_column.is_empty() && app.rel_base_column != app.rel_lookup_column && !app.rel_lookup_column.is_empty() && !app.rel_label_columns.is_empty();
                        ui.horizontal(|ui| {
                            if ui.add_enabled(ready, egui::Button::new("Add relation and apply")).clicked() {
                                app.rel_name.clear();
                                app.save_relation_from_draft();
                                app.request_resolve_model();
                            }
                        });
                        if let Some(msg) = &app.rel_message { ui.add_space(4.0); ui.label(RichText::new(msg).color(t.accent).size(11.5)); }
                    });
                }
            }
            if !app.data_model_diagnostics.errors.is_empty() || !app.data_model_diagnostics.warnings.is_empty() {
                ui.add_space(6.0);
                for e in &app.data_model_diagnostics.errors { ui.label(RichText::new(format!("Error: {e}")).color(egui::Color32::from_rgb(220, 90, 90)).size(11.5)); }
                for w in &app.data_model_diagnostics.warnings { ui.label(RichText::new(format!("Warning: {w}")).color(egui::Color32::from_rgb(220, 160, 60)).size(11.0)); }
            }
        });

        ui.add_space(10.0);
        // Legacy/single-source column selection (still useful for one resolved table)
        let Some(table) = app.table.clone() else {
            egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(12)).corner_radius(6.0).show(ui, |ui| {
                ui.label(RichText::new("No resolved table loaded.").strong().color(t.text_strong));
                ui.label(RichText::new("For a single source: import/load data from Home. For multi-source: add datasets, define relations, then Resolve Data Model.").color(t.text_muted));
            });
            return;
        };
        if app.uses_multi_source_model() || table.columns.iter().any(|c| c.name.contains('.')) {
            ui.label(RichText::new(format!("Resolved model: {} rows x {} columns. Fields are available in Worksheet layout (qualified as alias.column).", table.row_count(), table.columns.len())).color(t.text));
            return;
        }

        let total = table.columns.len();
        let selected = match &app.selected_columns { None => total, Some(s) => s.len() };
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Columns: {selected}/{total} selected  |  Rows: {}", table.row_count())).color(t.text));
            if ui.small_button("Select all").clicked() { app.select_all_columns(true); }
            if ui.small_button("Select none").clicked() { app.select_all_columns(false); }
        });
        ui.add_space(8.0);
        egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(10)).corner_radius(6.0).show(ui, |ui| {
            ui.label(RichText::new("Source columns").strong().color(t.text_strong));
            ui.add_space(6.0);
            egui::Grid::new("source_cols_grid").striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("Use").strong().color(t.text_muted));
                ui.label(RichText::new("Column").strong().color(t.text_muted));
                ui.label(RichText::new("Type").strong().color(t.text_muted));
                ui.label(RichText::new("Status").strong().color(t.text_muted));
                ui.end_row();
                let mut toggles: Vec<(String, bool)> = Vec::new();
                for col in &table.columns {
                    let mut checked = app.is_column_selected(&col.name);
                    let before = checked;
                    ui.checkbox(&mut checked, "");
                    let label = if let Some(o) = &col.original_name { format!("{} ({o})", col.name) } else { col.name.clone() };
                    ui.label(RichText::new(label).color(if checked { t.text_strong } else { t.text_disabled }));
                    ui.label(RichText::new(data_type_label(col.effective_type())).color(t.text_muted).size(11.5));
                    let stale_in: Vec<String> = app.worksheets.iter().filter(|w| w.spec.rows.iter().any(|f| f.name == col.name) || w.spec.columns.iter().any(|f| f.name == col.name) || w.spec.values.iter().any(|v| v.field.name == col.name) || w.spec.filters.iter().any(|f| f.field.name == col.name)).map(|w| w.name.clone()).collect();
                    let status = if stale_in.is_empty() { String::new() } else { format!("used by: {}", stale_in.join(", ")) };
                    ui.label(RichText::new(status).color(if !checked && !stale_in.is_empty() { egui::Color32::from_rgb(220, 140, 40) } else { t.text_muted }).size(11.0));
                    ui.end_row();
                    if checked != before { toggles.push((col.name.clone(), checked)); }
                }
                for (name, checked) in toggles { app.set_column_selected(&name, checked); }
            });
        });
    });
}


fn worksheet(app: &mut PivotApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());

    // Worksheet tabs
    egui::TopBottomPanel::top("worksheet_tabs").frame(egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::symmetric(8, 4))).show_inside(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let mut select: Option<usize> = None;
            let mut close: Option<usize> = None;
            let active = app.active_worksheet;
            for i in 0..app.worksheets.len() {
                let selected = i == active;
                let id = app.worksheets[i].id.clone();
                if selected {
                    let mut edit_name = app.worksheets[i].name.clone();
                    let edit = ui.add(egui::TextEdit::singleline(&mut edit_name)
                        .hint_text("Sheet name")
                        .desired_width(130.0)
                        .id(egui::Id::new(("sheet_name_edit", id.as_str()))));
                    if edit.changed() { app.rename_worksheet(i, edit_name); }
                } else {
                    let name = app.worksheets[i].name.clone();
                    ui.push_id(("sheet_tab", id.as_str()), |ui| {
                        if ui.selectable_label(false, &name).clicked() { select = Some(i); }
                        if app.worksheets.len() > 1
                            && ui.small_button("x").on_hover_text("Close worksheet").clicked() { close = Some(i); }
                    });
                    ui.add_space(4.0);
                    continue;
                }
                if app.worksheets.len() > 1 {
                    ui.push_id(("sheet_tab_close", id.as_str()), |ui| {
                        if ui.small_button("x").on_hover_text("Close worksheet").clicked() { close = Some(i); }
                    });
                }
                ui.add_space(4.0);
            }
            if ui.button("+ Add Worksheet").clicked() { app.add_worksheet(); }
            if let Some(i) = select { app.select_worksheet(i); }
            if let Some(i) = close { app.close_worksheet(i); }
        });
    });

    egui::TopBottomPanel::bottom("dock_bottom").resizable(true).default_height(170.0).min_height(80.0)
        .frame(egui::Frame::new().fill(t.panel_alt).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(8)))
        .show_inside(ui, |ui| {
            panel_header(ui, "Data Source / Filters", &t);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Data source:").color(t.text_muted));
                // Show full real source text as the editable default; editing changes only
                // the persisted Notes label (source_label), not the real data source.
                let real_source = app
                    .table
                    .as_ref()
                    .and_then(|tbl| tbl.source.clone())
                    .or_else(|| app.source_name.clone())
                    .unwrap_or_default();
                let mut src = app
                    .active_spec()
                    .source_label
                    .clone()
                    .unwrap_or_else(|| real_source.clone());
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut src)
                        .hint_text("Sumber data label (editable, does not change real data source)")
                        .desired_width(420.0),
                );
                if resp.changed() {
                    let trimmed = src.trim();
                    // Preserve spaces as typed (e.g. "Data Mira ...") to avoid fighting the cursor;
                    // treat blank/real-source-equivalent as no custom label override.
                    app.active_spec_mut().source_label = if trimmed.is_empty()
                        || (!real_source.is_empty() && trimmed == real_source.trim())
                    {
                        None
                    } else {
                        Some(src.clone())
                    };
                    if app.table.is_some() || app.db_source.is_some() {
                        app.request_pivot_recompute();
                    }
                }
                if ui.small_button("Manage columns...").clicked() { let ctx = ui.ctx().clone(); app.dispatch("nav.source", &Value::Null, &ctx); }
                let stale = app.stale_spec_refs(&app.active_spec().clone());
                if !stale.is_empty() { ui.label(RichText::new(format!("Stale refs: {}", stale.join(", "))).color(egui::Color32::from_rgb(220, 120, 60)).size(11.0)); }
            });
            ui.label(RichText::new("Edit to change only the Notes/'Sumber data' label in pivot output (real data source is unchanged).").color(t.text_muted).size(10.5));
            ui.add_space(4.0);
            if app.table.is_some() || app.db_source.is_some() { filters_ui(app, ui, &t); }
            else { ui.label(RichText::new("Import data (CSV/XLSX/DBF) or connect SQLite to configure filters.").color(t.text_muted)); }
        });

    egui::SidePanel::right("dock_right").resizable(true).default_width(280.0).min_width(160.0)
        .frame(egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, t.border)).inner_margin(egui::Margin::same(8)))
        .show_inside(ui, |ui| {
            panel_header(ui, &format!("Pivot Layout - {}", app.active_sheet().name), &t);
            ui.horizontal(|ui| {
                if ui.button("Recompute").clicked() { app.request_pivot_recompute(); }
                if ui.button("Clear").clicked() { app.clear_layout(); }
                if app.pivot_computing.is_some() { ui.spinner(); if ui.small_button("Cancel").clicked() { app.cancel_pivot(); } }
            });
            ui.add_space(4.0);
            let stale = app.stale_spec_refs(&app.active_spec().clone());
            if !stale.is_empty() {
                egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(220, 120, 40, 30)).stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(220, 120, 40))).inner_margin(egui::Margin::same(6)).corner_radius(4.0).show(ui, |ui| {
                    ui.label(RichText::new(format!("Unavailable columns referenced: {}. Re-enable in Source Data or remove from layout.", stale.join(", "))).color(egui::Color32::from_rgb(220, 140, 60)).size(11.0));
                });
                ui.add_space(4.0);
            }
            ui.label(RichText::new("Pick a selected data column, choose how to use it, then Add.").color(t.text_muted).size(11.0));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().id_salt("layout_scroll").auto_shrink([false, false]).show(ui, |ui| {
                app.sync_layout_pickers();
                let field_names: Vec<String> = app.available_columns().into_iter().map(|(n, _)| n).collect();
                if field_names.is_empty() {
                    ui.label(RichText::new("Load data and select columns in Source Data to configure pivots.").color(t.text_disabled));
                } else {
                    layout_picker_ui(app, ui, &t, &field_names);
                }
                ui.add_space(8.0);
                ui.separator();
                options_and_footer_ui(app, ui, &t);
            });
        });

    egui::CentralPanel::default().frame(egui::Frame::new().fill(t.canvas).stroke(egui::Stroke::NONE).inner_margin(egui::Margin::same(12))).show_inside(ui, |ui| {
        pivot_grid(app, ui, &t);
    });
}

fn options_and_footer_ui(app: &mut PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    ui.label(RichText::new("Options").strong().color(t.text_strong));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Output language").color(t.text_muted));
        let mut lang = app.active_spec().output_language;
        egui::ComboBox::from_id_salt(("output_language", app.active_worksheet)).selected_text(lang.label()).width(160.0).show_ui(ui, |ui| {
            for l in pivot_core::OutputLanguage::ALL { ui.selectable_value(&mut lang, l, l.label()); }
        });
        if lang != app.active_spec().output_language { app.active_spec_mut().output_language = lang; app.request_pivot_recompute(); }
    });
    let mut row_sub = app.active_spec().subtotals.row_subtotals;
    let row_sub_label = if app.active_spec().rows.len() > 1 {
        "Row subtotals (shown when 2+ row fields)".to_string()
    } else {
        "Row subtotals".to_string()
    };
    if ui
        .checkbox(&mut row_sub, format!("{row_sub_label}##subtotals_{}", app.active_worksheet))
        .on_hover_text("Show/hide subtotal rows for grouped row fields (requires 2 or more row fields).")
        .changed()
    {
        app.active_spec_mut().subtotals.row_subtotals = row_sub;
        app.request_pivot_recompute();
    }
    let mut col_tot = app.active_spec().subtotals.column_totals;
    let col_tot_enabled = !app.active_spec().columns.is_empty();
    ui.add_enabled_ui(col_tot_enabled, |ui| {
        if ui
            .checkbox(&mut col_tot, format!("Column totals##col_totals_{}", app.active_worksheet))
            .on_hover_text("Add a Total column across pivot columns (when column fields are configured).")
            .changed()
        {
            app.active_spec_mut().subtotals.column_totals = col_tot;
            app.request_pivot_recompute();
        }
    });

    ui.add_space(6.0);
    ui.label(RichText::new("Table title").color(t.text_strong));
    let auto = app
        .active_model()
        .map(|m| {
            if m.title_pinned {
                "(pinned)".to_string()
            } else {
                m.title.clone()
            }
        })
        .unwrap_or_else(|| "(recompute to preview)".to_string());
    let mut pinned = app.active_spec().title_override.is_some();
    if ui
        .checkbox(&mut pinned, format!("Pin title##pin_title_{}", app.active_worksheet))
        .changed()
    {
        app.set_title_pinned(pinned);
    }
    if let Some(mut over) = app.active_spec().title_override.clone() {
        // normalize legacy/base display: show without duplicated worksheet prefix
        let prefix = app.active_sheet().table_title_prefix();
        let shown = over.strip_prefix(&format!("{prefix} ")).unwrap_or(&over).to_string();
        over = shown;
        if ui
            .add(egui::TextEdit::singleline(&mut over).id(egui::Id::new(("title_override", app.active_worksheet))).hint_text("Table title (without Table NN prefix)"))
            .changed()
        {
            let v = over.trim().to_string();
            app.active_spec_mut().title_override = if v.is_empty() { None } else { Some(v) };
            app.request_pivot_recompute();
        }
        if ui.small_button("Reset title to auto").clicked() {
            app.set_title_pinned(false);
        }
    } else {
        ui.label(RichText::new(format!("Auto: {auto}")).color(t.text_muted).size(11.0));
    }

    ui.add_space(6.0);
    ui.label(RichText::new("Stub head (col 1)").color(t.text_strong));
    let auto_stub = app.active_model().and_then(|m| m.header_rows.first()).and_then(|r| r.first()).map(|c| c.text.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "(auto from row field)".to_string());
    let mut stub = app.active_spec().stub_head.clone().unwrap_or_default();
    let stub_resp = ui.add(egui::TextEdit::singleline(&mut stub).hint_text(&auto_stub).id(egui::Id::new(("stub_head", app.active_worksheet)))).on_hover_text("Leave empty to use the row column name by default");
    if stub_resp.changed() { app.active_spec_mut().stub_head = if stub.trim().is_empty() { None } else { Some(stub.clone()) }; app.request_pivot_recompute(); }
    if app.active_spec().stub_head.is_some() && ui.small_button("Clear stub head (use default)").clicked() { app.active_spec_mut().stub_head = None; app.request_pivot_recompute(); }
    if app.active_spec().stub_head.is_none() { ui.label(RichText::new(format!("Default: {auto_stub}")).color(t.text_muted).size(10.5)); }

    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Footer bands").strong().color(t.text_strong));
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("+ Grand Total").clicked() { app.add_footer_band(FooterBandSpec::grand_total()); }
        if ui.small_button("+ Average").clicked() { app.add_footer_band(FooterBandSpec::average()); }
        if ui.small_button("+ Count").clicked() { app.add_footer_band(FooterBandSpec::count()); }
        if ui.small_button("+ Notes").clicked() { app.add_footer_band(FooterBandSpec::notes()); }
    });
    let bands = app.active_spec().effective_footer_bands();
    if bands.is_empty() { ui.label(RichText::new("(no footer bands)").color(t.text_disabled)); }
    else {
        let mut remove: Option<usize> = None;
        let mut mov: Option<(usize, usize)> = None;
        let mut decimals_change: Option<(usize, Option<u8>)> = None;
        let len = bands.len();
        for (i, b) in bands.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:?}: {}", b.kind, b.effective_label())).color(t.text).size(12.0));
                if matches!(b.kind, pivot_core::FooterBandKind::Average) {
                    let current = b.decimals.map(|d| d.to_string()).unwrap_or_else(|| "auto".to_string());
                    egui::ComboBox::from_id_salt(("avg_decimals", app.active_worksheet, i)).width(68.0).selected_text(format!("dec: {current}")).show_ui(ui, |ui| {
                        for opt in [None, Some(0u8), Some(1), Some(2), Some(3), Some(4), Some(6)] {
                            let label = opt.map(|d| d.to_string()).unwrap_or_else(|| "auto".to_string());
                            if ui.selectable_label(b.decimals == opt, label).clicked() && b.decimals != opt { decimals_change = Some((i, opt)); }
                        }
                    });
                }
                if ui.small_button("x").clicked() { remove = Some(i); }
                if i > 0 && ui.small_button("^").clicked() { mov = Some((i, i - 1)); }
                if i + 1 < len && ui.small_button("v").clicked() { mov = Some((i, i + 1)); }
            });
        }
        if let Some((i, dec)) = decimals_change { app.set_footer_band_decimals(i, dec); }
        else if let Some(i) = remove { app.remove_footer_band(i); }
        else if let Some((a, b)) = mov { app.move_footer_band(a, b); }
    }
}

fn data_type_label(dt: DataType) -> &'static str {
    match dt { DataType::String => "Text (String)", DataType::Integer => "Integer", DataType::Decimal => "Decimal", DataType::Boolean => "Boolean", DataType::Date => "Date", DataType::DateTime => "DateTime" }
}
fn agg_label(agg: Aggregate) -> &'static str {
    match agg { Aggregate::Sum => "Sum", Aggregate::Count => "Count", Aggregate::CountDistinct => "Count distinct", Aggregate::Average => "Average", Aggregate::Min => "Min", Aggregate::Max => "Max" }
}

fn layout_picker_ui(app: &mut PivotApp, ui: &mut egui::Ui, t: &Tokens, field_names: &[String]) {
    let mut add_row: Option<(String, DataType)> = None;
    let mut add_col: Option<(String, DataType)> = None;
    let mut add_val: Option<(String, Aggregate)> = None;
    let sid = app.active_worksheet;

    ui.group(|ui| {
        ui.label(RichText::new("Rows (group by)").strong().color(t.text_strong));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt(("pick_row_field", sid)).selected_text(&app.pick_row_field).width(140.0).show_ui(ui, |ui| { for f in field_names { if ui.selectable_value(&mut app.pick_row_field, f.clone(), f).changed() { if let Some(dt) = app.field_type(&app.pick_row_field) { app.pick_row_as_type = dt; } } } });
            egui::ComboBox::from_id_salt(("pick_row_type", sid)).selected_text(data_type_label(app.pick_row_as_type)).width(120.0).show_ui(ui, |ui| { for dt in [DataType::String, DataType::Integer, DataType::Decimal, DataType::Boolean, DataType::Date, DataType::DateTime] { ui.selectable_value(&mut app.pick_row_as_type, dt, data_type_label(dt)); } });
            if ui.button("Add Row").clicked() { add_row = Some((app.pick_row_field.clone(), app.pick_row_as_type)); }
        });
        let mut remove_row: Option<usize> = None;
        let mut move_row: Option<(usize, usize)> = None;
        let row_len = app.active_spec().rows.len();
        for (i, f) in app.active_spec().rows.clone().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&f.name).color(t.text));
                if ui.small_button("x").clicked() { remove_row = Some(i); }
                if i > 0 && ui.small_button("^").clicked() { move_row = Some((i, i - 1)); }
                if i + 1 < row_len && ui.small_button("v").clicked() { move_row = Some((i, i + 1)); }
            });
        }
        if row_len == 0 { ui.label(RichText::new("(no row grouping)").color(t.text_disabled)); }
        if let Some(i) = remove_row { app.remove_row_field(i); } else if let Some((a, b)) = move_row { app.move_row_field(a, b); }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.label(RichText::new("Columns (split, optional)").strong().color(t.text_strong));
        ui.label(RichText::new("Enable 'Subtotal' on a column level to add subtotal columns for that grouping (useful with 2+ column levels).").color(t.text_muted).size(11.0));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt(("pick_col_field", sid)).selected_text(&app.pick_col_field).width(140.0).show_ui(ui, |ui| { for f in field_names { if ui.selectable_value(&mut app.pick_col_field, f.clone(), f).changed() { if let Some(dt) = app.field_type(&app.pick_col_field) { app.pick_col_as_type = dt; } } } });
            egui::ComboBox::from_id_salt(("pick_col_type", sid)).selected_text(data_type_label(app.pick_col_as_type)).width(120.0).show_ui(ui, |ui| { for dt in [DataType::String, DataType::Integer, DataType::Decimal, DataType::Boolean, DataType::Date, DataType::DateTime] { ui.selectable_value(&mut app.pick_col_as_type, dt, data_type_label(dt)); } });
            if ui.button("Add Column").clicked() { add_col = Some((app.pick_col_field.clone(), app.pick_col_as_type)); }
        });
        let mut remove_col: Option<usize> = None;
        let mut move_col: Option<(usize, usize)> = None;
        let mut toggle_col_sub: Option<(usize, bool)> = None;
        let col_len = app.active_spec().columns.len();
        let col_subs = app.active_spec().effective_column_subtotals();
        for (i, f) in app.active_spec().columns.clone().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&f.name).color(t.text));
                let mut sub = *col_subs.get(i).unwrap_or(&false);
                if ui.checkbox(&mut sub, format!("Subtotal##colsub_{sid}_{i}")).on_hover_text("Add subtotal columns for this column grouping level.").changed() {
                    toggle_col_sub = Some((i, sub));
                }
                if ui.small_button("x").clicked() { remove_col = Some(i); }
                if i > 0 && ui.small_button("^").clicked() { move_col = Some((i, i - 1)); }
                if i + 1 < col_len && ui.small_button("v").clicked() { move_col = Some((i, i + 1)); }
            });
        }
        if col_len == 0 { ui.label(RichText::new("(no column split)").color(t.text_disabled)); }
        if let Some((i, v)) = toggle_col_sub {
            let spec = app.active_spec_mut();
            let mut subs = spec.effective_column_subtotals();
            if i < subs.len() { subs[i] = v; spec.column_subtotals = subs; }
            app.request_pivot_recompute();
        } else if let Some(i) = remove_col { app.remove_col_field(i); } else if let Some((a, b)) = move_col { app.move_col_field(a, b); }
    });

    ui.add_space(6.0);
    ui.group(|ui| {
        ui.label(RichText::new("Values (required)").strong().color(t.text_strong));
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt(("pick_val_field", sid)).selected_text(&app.pick_val_field).width(140.0).show_ui(ui, |ui| { for f in field_names { if ui.selectable_value(&mut app.pick_val_field, f.clone(), f).changed() { if let Some(dt) = app.field_type(&app.pick_val_field) { app.pick_val_agg = if matches!(dt, DataType::Integer | DataType::Decimal) { Aggregate::Sum } else { Aggregate::Count }; } } } });
            egui::ComboBox::from_id_salt(("pick_val_agg", sid)).selected_text(agg_label(app.pick_val_agg)).width(120.0).show_ui(ui, |ui| { for agg in [Aggregate::Sum, Aggregate::Count, Aggregate::CountDistinct, Aggregate::Average, Aggregate::Min, Aggregate::Max] { ui.selectable_value(&mut app.pick_val_agg, agg, agg_label(agg)); } });
            if ui.button("Add Value").clicked() { add_val = Some((app.pick_val_field.clone(), app.pick_val_agg)); }
        });
        let mut remove_val: Option<usize> = None;
        for (i, v) in app.active_spec().values.clone().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}({})", v.aggregate.label(), v.field.name)).color(t.text));
                if ui.small_button("x").clicked() { remove_val = Some(i); }
            });
        }
        if app.active_spec().values.is_empty() { ui.label(RichText::new("(add a Value to compute the pivot)").color(t.accent)); }
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
    if let Some(model) = app.active_model() {
        ui.label(RichText::new(&model.title).strong().color(t.text_strong).size(16.0));
        ui.label(RichText::new(format!("Worksheet: {}  |  Theme: {} / {}", app.active_sheet().name, app.table_color_theme.label(), app.table_border_theme.label())).color(t.text_muted).size(11.0));
        ui.add_space(6.0);
        render_model(model, ui, t, app.brightness, app.table_color_theme, app.table_border_theme);
    } else if app.table.is_some() {
        ui.label(RichText::new("Pivot Grid").strong().color(t.text_strong).size(16.0));
        let spec = app.active_spec();
        let has_vals = !spec.values.is_empty();
        egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0_f32, if has_vals { t.border } else { t.accent })).inner_margin(egui::Margin::same(10)).corner_radius(6.0).show(ui, |ui| {
            ui.label(RichText::new(format!("Build '{}' (Pivot Layout panel, right)", app.active_sheet().name)).strong().color(t.text_strong));
            let rows = if spec.rows.is_empty() { "(none - optional)".to_string() } else { spec.rows.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ") };
            let cols = if spec.columns.is_empty() { "(none - optional)".to_string() } else { spec.columns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ") };
            let vals = if has_vals { spec.values.iter().map(|v| format!("{}({})", v.aggregate.label(), v.field.name)).collect::<Vec<_>>().join(", ") } else { "(missing - required)".to_string() };
            ui.label(RichText::new(format!("Rows: {rows}")).color(t.text));
            ui.label(RichText::new(format!("Columns: {cols}")).color(t.text));
            ui.label(RichText::new(format!("Values: {vals}")).color(if has_vals { t.text } else { t.accent }));
            ui.add_space(4.0);
            if !has_vals { ui.label(RichText::new("Add a Value to compute a pivot. Use 'Manage columns...' to choose which source columns are available.").color(t.text_muted)); }
            else if app.pivot_computing.is_some() { ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("Computing pivot...").color(t.text_muted)); }); }
            else { ui.label(RichText::new("Click Recompute if the result doesn't appear.").color(t.text_muted)); }
        });
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Data loaded: {} rows x {} cols total. Raw data preview hidden; use source column selection and pivot layout.", app.row_count(), app.col_count())).color(t.text_muted).size(11.0));
    } else {
        ui.label(RichText::new("Pivot Grid").strong().color(t.text_strong).size(16.0));
        ui.label(RichText::new("Import data from Home to begin.").color(t.text_muted));
    }
}

fn render_model(model: &ThemedTableModel, ui: &mut egui::Ui, t: &Tokens, brightness: pivot_theme::Brightness, color_theme: pivot_theme::table::TableColorThemeId, border_theme: pivot_theme::table::BorderThemeId) {
    crate::pivot_grid::show(ui, model, t, brightness, color_theme, border_theme);
}

fn op_label(op: FilterOp) -> &'static str { match op { FilterOp::Equals => "Equals", FilterOp::NotEquals => "Not equals", FilterOp::Include => "Include", FilterOp::Exclude => "Exclude" } }
fn value_label(v: &CoreValue) -> String { match v { CoreValue::Null => "(empty)".to_string(), _ => { let s = v.as_display(); if s.is_empty() { "(empty)".to_string() } else { s } } } }

fn filters_ui(app: &mut PivotApp, ui: &mut egui::Ui, t: &Tokens) {
    let fields: Vec<String> = app.available_columns().into_iter().map(|(n, _)| n).collect();
    if fields.is_empty() { ui.label(RichText::new("No selected fields available for filters.").color(t.text_disabled)); return; }
    if app.filter_field.is_empty() || !fields.iter().any(|f| f == &app.filter_field) { app.filter_field = fields[0].clone(); app.filter_value_idx = 0; }
    let values = app.distinct_values_for(&app.filter_field, 400);
    if app.filter_value_idx >= values.len() { app.filter_value_idx = 0; }

    let mut add: Option<(String, FilterOp, CoreValue)> = None;
    let mut clear_all = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Field").color(t.text_muted).size(11.0));
        egui::ComboBox::from_id_salt(("filter_field", app.active_worksheet)).selected_text(&app.filter_field).width(140.0).show_ui(ui, |ui| { for f in &fields { if ui.selectable_value(&mut app.filter_field, f.clone(), f).changed() { app.filter_value_idx = 0; } } });
        ui.label(RichText::new("Op").color(t.text_muted).size(11.0));
        egui::ComboBox::from_id_salt(("filter_op", app.active_worksheet)).selected_text(op_label(app.filter_op)).width(110.0).show_ui(ui, |ui| { for op in [FilterOp::Equals, FilterOp::NotEquals, FilterOp::Include, FilterOp::Exclude] { ui.selectable_value(&mut app.filter_op, op, op_label(op)); } });
        ui.label(RichText::new("Value").color(t.text_muted).size(11.0));
        let selected = values.get(app.filter_value_idx).map(value_label).unwrap_or_else(|| "(no values)".to_string());
        egui::ComboBox::from_id_salt(("filter_value", app.active_worksheet)).selected_text(selected).width(170.0).show_ui(ui, |ui| {
            if values.is_empty() { ui.label(RichText::new("(no values)").color(t.text_disabled)); }
            for (i, v) in values.iter().enumerate() { ui.selectable_value(&mut app.filter_value_idx, i, value_label(v)); }
        });
        if ui.button("Add filter").clicked() { if let Some(v) = values.get(app.filter_value_idx).cloned() { add = Some((app.filter_field.clone(), app.filter_op, v)); } else { app.status = "No value selected for filter.".to_string(); } }
        if ui.button("Clear all").clicked() { clear_all = true; }
    });
    if let Some((f, op, v)) = add { app.add_filter(f, op, v); }
    if clear_all { app.clear_filters(); }

    let filters = app.active_spec().filters.clone();
    if filters.is_empty() {
        ui.label(RichText::new("No active filters. Dynamic title/Notes update when filters change.").color(t.text_muted).size(11.0));
    } else {
        let mut remove: Option<usize> = None;
        for (i, f) in filters.iter().enumerate() {
            let vtxt = f.values.first().map(value_label).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {} {}", f.field.name, op_label(f.op), vtxt)).color(t.text).size(12.0));
                if ui.small_button("x").clicked() { remove = Some(i); }
            });
        }
        if let Some(i) = remove { app.remove_filter(i); }
    }
}
