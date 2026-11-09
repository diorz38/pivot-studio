//! Application state and egui frame/update (DesignCraft pattern, simplified).

use crate::{chrome, dock, menus};
use pivot_core::pivot::compute;
use pivot_core::{
    Aggregate, DataTable, DataType, FieldRef, Filter, FilterOp, FooterBandSpec, PivotSpec, SubtotalConfig,
    ThemedTableModel, Value as CoreValue, ValueField,
};
use pivot_data::{FileKind, db_import::{self, CredentialMode, MySqlProfile}};
use pivot_data::workspace::{self, WorkspaceFile, WorkspaceSource, WorkspaceView};
use pivot_export::ExportStyle;
use pivot_theme::{
    apply,
    table::{parse_legacy_theme, BorderThemeId, TableColorThemeId},
    Brightness, Tokens,
};
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
    pub path: PathBuf,
    pub kind: FileKind,
    pub table: DataTable,
}

pub struct LoadState {
    pub rx: Receiver<LoadMsg>,
    pub progress: String,
    pub cancel: Arc<AtomicBool>,
}

pub enum PivotMsg {
    Done(Result<ThemedTableModel, String>),
}

pub struct PivotComputeState {
    pub rx: Receiver<PivotMsg>,
    pub cancel: Arc<AtomicBool>,
}

pub enum ExportMsg {
    Progress(String),
    Done(Result<String, String>),
}

pub struct ExportState {
    pub rx: Receiver<ExportMsg>,
    pub progress: String,
    pub cancel: Arc<AtomicBool>,
}

pub struct PivotApp {
    pub brightness: Brightness,
    pub table_color_theme: TableColorThemeId,
    pub table_border_theme: BorderThemeId,
    pub status: String,
    brightness_applied: Option<Brightness>,
    exit_requested: bool,
    pub on_home: bool,
    pub table: Option<DataTable>,
    pub source_name: Option<String>,
    pub loading: Option<LoadState>,
    pub pivot_spec: PivotSpec,
    pub pivot_model: Option<ThemedTableModel>,
    pub pivot_computing: Option<PivotComputeState>,
    pub exporting: Option<ExportState>,
    // Filter builder draft (UI state)
    pub filter_field: String,
    pub filter_op: FilterOp,
    pub filter_value_idx: usize,
    // Layout picker draft (UI state)
    pub pick_row_field: String,
    pub pick_row_as_type: DataType,
    pub pick_col_field: String,
    pub pick_col_as_type: DataType,
    pub pick_val_field: String,
    pub pick_val_agg: Aggregate,
    // Phase 5: database connect/pushdown
    pub db_panel_open: bool,
    pub db_tab_mysql: bool,
    pub db_sqlite_path: String,
    pub db_sqlite_tables: Vec<String>,
    pub db_sqlite_table: String,
    pub db_mysql_host: String,
    pub db_mysql_port: String,
    pub db_mysql_db: String,
    pub db_mysql_user: String,
    pub db_mysql_table: String,
    pub db_mysql_password: String,
    pub db_mysql_ssl: bool,
    pub db_mysql_cred_mode_session: bool,
    pub db_source: Option<DbSource>,
    pub workspace_path: Option<PathBuf>,
    pub loaded_file_path: Option<PathBuf>,
    pub loaded_file_kind: Option<FileKind>,
}

#[derive(Debug, Clone)]
pub enum DbSource {
    Sqlite { path: PathBuf, table: String },
    MySql(MySqlProfile),
}

impl PivotApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let brightness = Brightness::default();
        let tokens = Tokens::for_brightness(brightness);
        apply(&cc.egui_ctx, &tokens);
        Self {
            brightness,
            table_color_theme: TableColorThemeId::default(),
            table_border_theme: BorderThemeId::default(),
            status: "Home - open a saved project or import data to start.".to_string(),
            brightness_applied: Some(brightness),
            exit_requested: false,
            on_home: true,
            table: None,
            source_name: None,
            loading: None,
            pivot_spec: PivotSpec {
                subtotals: SubtotalConfig {
                    row_subtotals: true,
                    grand_total: true,
                },
                ..Default::default()
            },
            pivot_model: None,
            pivot_computing: None,
            exporting: None,
            filter_field: String::new(),
            filter_op: FilterOp::Equals,
            filter_value_idx: 0,
            pick_row_field: String::new(),
            pick_row_as_type: DataType::String,
            pick_col_field: String::new(),
            pick_col_as_type: DataType::String,
            pick_val_field: String::new(),
            pick_val_agg: Aggregate::Count,
            db_panel_open: false,
            db_tab_mysql: false,
            db_sqlite_path: String::new(),
            db_sqlite_tables: Vec::new(),
            db_sqlite_table: String::new(),
            db_mysql_host: "127.0.0.1".to_string(),
            db_mysql_port: "3306".to_string(),
            db_mysql_db: String::new(),
            db_mysql_user: String::new(),
            db_mysql_table: String::new(),
            db_mysql_password: String::new(),
            db_mysql_ssl: false,
            db_mysql_cred_mode_session: true,
            db_source: None,
            workspace_path: None,
            loaded_file_path: None,
            loaded_file_kind: None,
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

    pub fn sync_layout_pickers(&mut self) {
        let fields: Vec<(String, DataType)> = self
            .table
            .as_ref()
            .map(|t| t.columns.iter().map(|c| (c.name.clone(), c.effective_type())).collect())
            .unwrap_or_default();
        if fields.is_empty() {
            return;
        }
        if self.pick_row_field.is_empty() || !fields.iter().any(|(n, _)| n == &self.pick_row_field) {
            self.pick_row_field = fields[0].0.clone();
            self.pick_row_as_type = fields[0].1;
        }
        if self.pick_col_field.is_empty() || !fields.iter().any(|(n, _)| n == &self.pick_col_field) {
            self.pick_col_field = fields[0].0.clone();
            self.pick_col_as_type = fields[0].1;
        }
        if self.pick_val_field.is_empty() || !fields.iter().any(|(n, _)| n == &self.pick_val_field) {
            // Prefer a numeric field for values when available.
            if let Some((n, _dt)) = fields.iter().find(|(_, dt)| matches!(dt, DataType::Integer | DataType::Decimal)) {
                self.pick_val_field = n.clone();
                self.pick_val_agg = Aggregate::Sum;
            } else {
                self.pick_val_field = fields[0].0.clone();
                self.pick_val_agg = Aggregate::Count;
            }
        }
    }

    pub fn field_type(&self, name: &str) -> Option<DataType> {
        self.table.as_ref()?.columns.iter().find(|c| c.name == name).map(|c| c.effective_type())
    }

    pub fn reinterpret_named_column(&mut self, name: &str, target: DataType) {
        let idx = self.table.as_ref().and_then(|t| t.columns.iter().position(|c| c.name == name));
        if let Some(i) = idx {
            self.reinterpret_column(i, target);
        }
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
                        "Loaded '{}' ({} rows, {} cols). Assign fields to build a pivot.",
                        loaded.name,
                        loaded.table.row_count(),
                        loaded.table.column_count()
                    );
                    self.source_name = Some(loaded.name);
                    self.table = Some(loaded.table);
                    self.pivot_model = None;
                    self.db_source = None;
                    self.on_home = false;
                }
                Err(e) => {
                    self.status = format!("Load failed: {e}");
                }
            }
            ctx.request_repaint();
        }
    }

    pub fn poll_pivot(&mut self, ctx: &egui::Context) {
        let mut finished: Option<Result<ThemedTableModel, String>> = None;
        if let Some(state) = &mut self.pivot_computing {
            while let Ok(msg) = state.rx.try_recv() {
                match msg {
                    PivotMsg::Done(res) => {
                        finished = Some(res);
                        break;
                    }
                }
            }
        }
        if let Some(res) = finished {
            self.pivot_computing = None;
            match res {
                Ok(model) => {
                    self.status = format!(
                        "Pivot updated: {} body row(s), {} footer band(s).",
                        model.body_rows.len(),
                        model.footer_bands.len()
                    );
                    self.pivot_model = Some(model);
                }
                Err(e) => {
                    self.status = format!("Pivot compute failed: {e}");
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
                    let _ = tx.send(LoadMsg::Done(Ok(LoadedTable { name, path, kind, table })));
                }
                Err(e) => {
                    let _ = tx.send(LoadMsg::Done(Err(e.to_string())));
                }
            }
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

    pub fn cancel_pivot(&mut self) {
        if let Some(state) = &self.pivot_computing {
            state.cancel.store(true, Ordering::Relaxed);
            self.status = "Cancelling pivot...".to_string();
        }
    }

    pub fn cancel_export(&mut self) {
        if let Some(state) = &self.exporting {
            state.cancel.store(true, Ordering::Relaxed);
            self.status = "Cancelling export...".to_string();
        }
    }

    pub fn poll_export(&mut self, ctx: &egui::Context) {
        let mut finished: Option<Result<String, String>> = None;
        if let Some(state) = &mut self.exporting {
            while let Ok(msg) = state.rx.try_recv() {
                match msg {
                    ExportMsg::Progress(s) => {
                        state.progress = s.clone();
                        self.status = s;
                    }
                    ExportMsg::Done(res) => {
                        finished = Some(res);
                        break;
                    }
                }
            }
        }
        if let Some(res) = finished {
            self.exporting = None;
            match res {
                Ok(s) => self.status = s,
                Err(e) => self.status = format!("Export failed: {e}"),
            }
            ctx.request_repaint();
        }
    }

    pub fn reinterpret_column(&mut self, col_idx: usize, target: DataType) {
        if let Some(table) = &mut self.table {
            if let Some(col) = table.columns.get_mut(col_idx) {
                col.override_type = Some(target);
            }
            table.reinterpret_column(col_idx);
            self.pivot_model = None;
            self.status = format!("Column type updated ({} rows). Recompute pivot if needed.", table.row_count());
        }
    }

    pub fn db_pick_sqlite_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("Select SQLite database").add_filter("SQLite", &["db", "sqlite", "sqlite3"]).pick_file() {
            self.db_sqlite_path = path.to_string_lossy().to_string();
            self.db_sqlite_tables.clear();
            self.db_sqlite_table.clear();
            self.refresh_sqlite_tables();
        }
    }

    pub fn refresh_sqlite_tables(&mut self) {
        if self.db_sqlite_path.trim().is_empty() { return; }
        match db_import::list_sqlite_tables(&PathBuf::from(self.db_sqlite_path.trim())) {
            Ok(t) => {
                self.db_sqlite_tables = t;
                if self.db_sqlite_table.is_empty() {
                    self.db_sqlite_table = self.db_sqlite_tables.first().cloned().unwrap_or_default();
                }
                self.status = format!("Found {} SQLite table(s).", self.db_sqlite_tables.len());
            }
            Err(e) => self.status = format!("SQLite error: {e}"),
        }
    }

    pub fn db_preview_sqlite(&mut self) {
        let path = PathBuf::from(self.db_sqlite_path.trim());
        let table = self.db_sqlite_table.trim().to_string();
        if table.is_empty() { self.status = "Choose a SQLite table.".to_string(); return; }
        match db_import::connect_and_preview_sqlite(&path, &table, 500, None) {
            Ok(t) => {
                self.source_name = Some(format!("sqlite:{}:{}", path.display(), table));
                self.table = Some(t);
                self.db_source = Some(DbSource::Sqlite { path, table });
                self.pivot_model = None;
                self.sync_layout_pickers();
                self.status = format!("Loaded SQLite preview ({} rows, {} cols). Recompute uses pushdown SQL (GROUP BY).", self.row_count(), self.col_count());
                self.on_home = false;
            }
            Err(e) => self.status = format!("SQLite preview failed: {e}"),
        }
    }

    pub fn mysql_credential_mode(&self) -> CredentialMode {
        if self.db_mysql_cred_mode_session { CredentialMode::SessionOnly } else { CredentialMode::OsKeychain }
    }

    pub fn db_apply_mysql_profile(&mut self) -> Option<MySqlProfile> {
        self.status = "MySQL is temporarily disabled. Use CSV/XLSX/DBF or SQLite.".to_string();
        None
    }

    pub fn db_preview_mysql(&mut self) {
        self.status = "MySQL is temporarily disabled. Use CSV/XLSX/DBF or SQLite.".to_string();
    }

    pub fn request_pivot_recompute(&mut self) {
        if self.table.is_none() && self.db_source.is_none() {
            self.status = "Load data before computing a pivot.".to_string();
            return;
        }
        if self.pivot_computing.is_some() {
            self.status = "Pivot computation already in progress.".to_string();
            return;
        }
        if self.pivot_spec.values.is_empty() {
            self.status = "Pivot needs a Values field: pick a field on the left and click Sum, Count, or Avg (Rows/Columns are optional groupings).".to_string();
            self.pivot_model = None;
            return;
        }
        let spec = self.pivot_spec.clone();
        let table = self.table.clone();
        let db = self.db_source.clone();
        let (tx, rx) = mpsc::channel::<PivotMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        self.status = if db.is_some() { "Computing pivot via database pushdown (SQL GROUP BY)...".to_string() } else { "Computing pivot...".to_string() };
        self.pivot_computing = Some(PivotComputeState { rx, cancel });
        thread::spawn(move || {
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(PivotMsg::Done(Err("Cancelled.".to_string())));
                return;
            }
            let res = if let Some(src) = db {
                match src {
                    DbSource::Sqlite { path, table: tname } => {
                        db_import::execute_pivot_sqlite(&path, &tname, &spec, Some(cancel_flag.as_ref())).map_err(|e| e.to_string())
                    }
                    DbSource::MySql(profile) => {
                        db_import::execute_pivot_mysql(&profile, &spec, Some(cancel_flag.as_ref())).map_err(|e| e.to_string())
                    }
                }
            } else if let Some(table) = table {
                Ok(compute(&table, &spec))
            } else {
                Err("No data source.".to_string())
            };
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(PivotMsg::Done(Err("Cancelled.".to_string())));
                return;
            }
            let _ = tx.send(PivotMsg::Done(res));
        });
    }

    pub fn add_row_field(&mut self, name: String) {
        if !self.pivot_spec.rows.iter().any(|f| f.name == name) {
            let n = name.clone();
            self.pivot_spec.rows.push(FieldRef::new(name));
            if self.pivot_spec.values.is_empty() {
                self.status = format!("Row '{n}' added. Now add a Values field (Sum/Count/Avg) to compute the pivot.");
                self.pivot_model = None;
            } else {
                self.request_pivot_recompute();
            }
        } else {
            self.status = format!("'{name}' is already in Rows.");
        }
    }
    pub fn add_col_field(&mut self, name: String) {
        if !self.pivot_spec.columns.iter().any(|f| f.name == name) {
            let n = name.clone();
            self.pivot_spec.columns.push(FieldRef::new(name));
            if self.pivot_spec.values.is_empty() {
                self.status = format!("Column '{n}' added. Now add a Values field (Sum/Count/Avg) to compute the pivot.");
                self.pivot_model = None;
            } else {
                self.request_pivot_recompute();
            }
        } else {
            self.status = format!("'{name}' is already in Columns.");
        }
    }
    pub fn add_value_field(&mut self, name: String, aggregate: Aggregate) {
        self.pivot_spec.values.push(ValueField {
            field: FieldRef::new(name.clone()),
            aggregate,
            display_name: None,
        });
        self.status = format!("Value {}({name}) added. Computing pivot...", aggregate.label());
        self.request_pivot_recompute();
    }
    pub fn remove_row_field(&mut self, idx: usize) {
        if idx < self.pivot_spec.rows.len() {
            self.pivot_spec.rows.remove(idx);
            self.request_pivot_recompute();
        }
    }

    pub fn remove_col_field(&mut self, idx: usize) {
        if idx < self.pivot_spec.columns.len() {
            self.pivot_spec.columns.remove(idx);
            self.request_pivot_recompute();
        }
    }

    pub fn remove_value_field(&mut self, idx: usize) {
        if idx < self.pivot_spec.values.len() {
            self.pivot_spec.values.remove(idx);
            if self.pivot_spec.values.is_empty() {
                self.pivot_model = None;
                self.status = "Values cleared. Add a Values field to compute a pivot.".to_string();
            } else {
                self.request_pivot_recompute();
            }
        }
    }

    pub fn move_row_field(&mut self, from: usize, to: usize) {
        let len = self.pivot_spec.rows.len();
        if from < len && to < len && from != to {
            let item = self.pivot_spec.rows.remove(from);
            self.pivot_spec.rows.insert(to, item);
            self.request_pivot_recompute();
        }
    }

    pub fn move_col_field(&mut self, from: usize, to: usize) {
        let len = self.pivot_spec.columns.len();
        if from < len && to < len && from != to {
            let item = self.pivot_spec.columns.remove(from);
            self.pivot_spec.columns.insert(to, item);
            self.request_pivot_recompute();
        }
    }

    pub fn clear_layout(&mut self) {
        self.pivot_spec.rows.clear();
        self.pivot_spec.columns.clear();
        self.pivot_spec.values.clear();
        self.pivot_spec.filters.clear();
        // Keep polished config (stub_head/title/footer bands) as user preference? Clear layout focuses on fields.
        self.pivot_model = None;
        self.status = "Pivot layout cleared.".to_string();
    }

    /// Distinct display values for a column (capped), for filter pickers.
    pub fn distinct_values_for(&self, col_name: &str, limit: usize) -> Vec<CoreValue> {
        let Some(table) = &self.table else { return Vec::new() };
        let Some(ci) = table.columns.iter().position(|c| c.name == col_name) else { return Vec::new() };
        let mut seen = std::collections::BTreeSet::<String>::new();
        let mut out = Vec::new();
        for row in &table.rows {
            let v = row.get(ci).cloned().unwrap_or(CoreValue::Null);
            let key = v.as_display();
            if seen.insert(key) {
                out.push(v);
                if out.len() >= limit { break; }
            }
        }
        out
    }

    pub fn add_filter(&mut self, field: String, op: FilterOp, value: CoreValue) {
        if field.is_empty() { return; }
        self.pivot_spec.filters.push(Filter { field: FieldRef::new(field), op, values: vec![value] });
        self.status = format!("Filter added ({} active).", self.pivot_spec.filters.len());
        self.request_pivot_recompute();
    }

    pub fn remove_filter(&mut self, idx: usize) {
        if idx < self.pivot_spec.filters.len() {
            self.pivot_spec.filters.remove(idx);
            self.request_pivot_recompute();
        }
    }

    pub fn clear_filters(&mut self) {
        if !self.pivot_spec.filters.is_empty() {
            self.pivot_spec.filters.clear();
            self.request_pivot_recompute();
            self.status = "Filters cleared.".to_string();
        }
    }

    fn ensure_explicit_footer_bands(&mut self) {
        if self.pivot_spec.footer_bands.is_empty() {
            self.pivot_spec.footer_bands = self.pivot_spec.effective_footer_bands();
            self.pivot_spec.subtotals.grand_total = false; // explicit bands now authoritative
        }
    }

    pub fn add_footer_band(&mut self, band: FooterBandSpec) {
        self.ensure_explicit_footer_bands();
        self.pivot_spec.footer_bands.push(band);
        self.request_pivot_recompute();
    }

    pub fn remove_footer_band(&mut self, idx: usize) {
        self.ensure_explicit_footer_bands();
        if idx < self.pivot_spec.footer_bands.len() {
            self.pivot_spec.footer_bands.remove(idx);
            self.request_pivot_recompute();
        }
    }

    pub fn move_footer_band(&mut self, from: usize, to: usize) {
        self.ensure_explicit_footer_bands();
        let len = self.pivot_spec.footer_bands.len();
        if from < len && to < len && from != to {
            let item = self.pivot_spec.footer_bands.remove(from);
            self.pivot_spec.footer_bands.insert(to, item);
            self.request_pivot_recompute();
        }
    }

    pub fn set_title_pinned(&mut self, pinned: bool) {
        if pinned {
            let current = self
                .pivot_model
                .as_ref()
                .map(|m| m.title.clone())
                .or(self.pivot_spec.title_override.clone())
                .unwrap_or_default();
            self.pivot_spec.title_override = Some(current);
        } else {
            self.pivot_spec.title_override = None;
        }
        self.request_pivot_recompute();
    }


    pub fn set_footer_band_decimals(&mut self, idx: usize, decimals: Option<u8>) {
        self.ensure_explicit_footer_bands();
        if let Some(b) = self.pivot_spec.footer_bands.get_mut(idx) {
            b.decimals = decimals;
            self.status = format!(
                "Average decimals set to {}.",
                decimals.map(|d| d.to_string()).unwrap_or_else(|| "auto".to_string())
            );
            self.request_pivot_recompute();
        }
    }

    fn file_kind_name(kind: FileKind) -> &'static str {
        match kind {
            FileKind::Csv => "csv",
            FileKind::Tsv => "tsv",
            FileKind::Xlsx => "xlsx",
            FileKind::Dbf => "dbf",
        }
    }

    fn file_kind_from_name(name: Option<&str>) -> Option<FileKind> {
        match name.unwrap_or("").to_ascii_lowercase().as_str() {
            "csv" => Some(FileKind::Csv),
            "tsv" => Some(FileKind::Tsv),
            "xlsx" => Some(FileKind::Xlsx),
            "dbf" => Some(FileKind::Dbf),
            _ => None,
        }
    }

    fn build_workspace_file(&self) -> WorkspaceFile {
        let source = if let Some(db) = &self.db_source {
            match db {
                DbSource::Sqlite { path, table } => WorkspaceSource::Sqlite {
                    path: path.to_string_lossy().to_string(),
                    table: table.clone(),
                },
                DbSource::MySql(profile) => WorkspaceSource::MySql {
                    profile: profile.sanitized_for_storage(),
                },
            }
        } else if let Some(path) = &self.loaded_file_path {
            WorkspaceSource::File {
                path: path.to_string_lossy().to_string(),
                file_kind: self.loaded_file_kind.map(Self::file_kind_name).map(str::to_string),
            }
        } else {
            WorkspaceSource::None
        };
        WorkspaceFile {
            manifest: pivot_data::WorkspaceManifest::default(),
            spec: self.pivot_spec.clone(),
            view: WorkspaceView {
                brightness_id: Some(self.brightness.id().to_string()),
                table_theme_id: Some(self.table_color_theme.id().to_string()),
                border_theme_id: Some(self.table_border_theme.id().to_string()),
                on_home: self.on_home,
            },
            source,
        }
    }

    fn new_workspace(&mut self) {
        self.table = None;
        self.source_name = None;
        self.loaded_file_path = None;
        self.loaded_file_kind = None;
        self.db_source = None;
        self.pivot_model = None;
        self.pivot_computing = None;
        self.pivot_spec = PivotSpec {
            subtotals: pivot_core::SubtotalConfig { row_subtotals: true, grand_total: true },
            ..Default::default()
        };
        self.workspace_path = None;
        self.on_home = true;
        self.status = "New workspace.".to_string();
    }

    fn save_workspace(&mut self, save_as: bool) {
        let path = if !save_as {
            if let Some(p) = &self.workspace_path { Some(p.clone()) } else { None }
        } else {
            None
        };
        let path = match path {
            Some(p) => p,
            None => {
                let title = self.pivot_model.as_ref().map(|m| m.title.as_str()).unwrap_or("workspace");
                let default = workspace::suggested_workspace_path(None, title);
                match rfd::FileDialog::new()
                    .set_title("Save Pivot Studio Workspace")
                    .add_filter("Pivot Workspace", &["pivot"])
                    .set_file_name(default.file_name().and_then(|n| n.to_str()).unwrap_or("workspace.pivot"))
                    .save_file()
                {
                    Some(mut p) => {
                        if p.extension().and_then(|e| e.to_str()) != Some("pivot") { p.set_extension("pivot"); }
                        p
                    }
                    None => { self.status = "Save workspace cancelled.".to_string(); return; }
                }
            }
        };
        let ws = self.build_workspace_file();
        match workspace::save_workspace(&path, &ws) {
            Ok(()) => {
                self.workspace_path = Some(path.clone());
                self.status = format!("Workspace saved: {}", path.display());
            }
            Err(e) => self.status = format!("Save workspace failed: {e}"),
        }
    }

    fn open_workspace(&mut self) {
        let path = match rfd::FileDialog::new()
            .set_title("Open Pivot Studio Workspace")
            .add_filter("Pivot Workspace", &["pivot"])
            .pick_file()
        {
            Some(p) => p,
            None => { self.status = "Open workspace cancelled.".to_string(); return; }
        };
        let ws = match workspace::load_workspace(&path) {
            Ok(w) => w,
            Err(e) => { self.status = format!("Open workspace failed: {e}"); return; }
        };
        self.workspace_path = Some(path.clone());
        self.pivot_spec = ws.spec;
        if let Some(id) = ws.view.brightness_id.as_deref() {
            if let Some(b) = Brightness::parse(id) { self.brightness = b; }
        }
        if ws.view.border_theme_id.is_some() || ws.view.table_theme_id.is_some() {
            let (cid, bid) = ws.view.resolved_theme_ids();
            if let Some(t) = TableColorThemeId::parse(&cid) { self.table_color_theme = t; }
            if let Some(b) = BorderThemeId::parse(&bid) { self.table_border_theme = b; }
        }
        self.on_home = ws.view.on_home;
        self.table = None;
        self.source_name = None;
        self.loaded_file_path = None;
        self.loaded_file_kind = None;
        self.db_source = None;
        self.pivot_model = None;
        self.pivot_computing = None;
        self.sync_layout_pickers();
        match ws.source {
            WorkspaceSource::None => {
                self.status = format!("Workspace opened: {} (no data source).", path.display());
            }
            WorkspaceSource::File { path: sp, file_kind } => {
                let sp = PathBuf::from(sp);
                let kind = Self::file_kind_from_name(file_kind.as_deref()).unwrap_or_else(|| match sp.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
                    "tsv" => FileKind::Tsv, "xlsx"|"xls"|"xlsm" => FileKind::Xlsx, "dbf" => FileKind::Dbf, _ => FileKind::Csv,
                });
                if sp.exists() {
                    self.start_load(sp, kind);
                    self.status = format!("Workspace opened: {}. Reloading data source...", path.display());
                } else {
                    self.status = format!("Workspace opened, but data file not found: {}. Import data to continue.", sp.display());
                    self.on_home = true;
                }
            }
            WorkspaceSource::Sqlite { path: sp, table } => {
                let sp = PathBuf::from(sp);
                if sp.exists() {
                    self.db_source = Some(DbSource::Sqlite { path: sp.clone(), table: table.clone() });
                    self.db_sqlite_path = sp.to_string_lossy().to_string();
                    self.db_sqlite_table = table.clone();
                    match db_import::connect_and_preview_sqlite(&sp, &table, 200, None) {
                        Ok(t) => {
                            self.table = Some(t);
                            self.source_name = Some(format!("sqlite:{}:{}", sp.display(), table));
                            self.on_home = false;
                            self.status = format!("Workspace opened (SQLite {} / {}).", sp.display(), table);
                            self.sync_layout_pickers();
                            self.request_pivot_recompute();
                        }
                        Err(e) => self.status = format!("Workspace opened (SQLite), preview failed: {e}. Reconnect in Database panel."),
                    }
                } else {
                    self.status = format!("Workspace opened, SQLite file not found: {}. Reconnect in Database panel.", sp.display());
                }
            }
            WorkspaceSource::MySql { profile } => {
                let _msg = match profile.credential_mode {
                    CredentialMode::SessionOnly => "enter password (session-only) and preview.",
                    CredentialMode::OsKeychain => "stored in OS keychain if available; preview to reconnect.",
                };
                self.db_source = Some(DbSource::MySql(profile.clone()));
                self.db_mysql_host = profile.host;
                self.db_mysql_port = profile.port.to_string();
                self.db_mysql_db = profile.database;
                self.db_mysql_user = profile.user;
                self.db_mysql_table = profile.table;
                self.db_mysql_ssl = profile.ssl;
                self.db_mysql_cred_mode_session = matches!(profile.credential_mode, CredentialMode::SessionOnly);
                self.db_panel_open = true;
                self.on_home = false;
                self.status = "Workspace has a MySQL source, but MySQL is temporarily disabled. Use CSV/XLSX/DBF or SQLite.".to_string(); self.db_panel_open = false;
            }
        }
    }
    pub fn export_pivot(&mut self, format: &str) {
        if self.exporting.is_some() {
            self.status = "An export is already in progress.".to_string();
            return;
        }
        let Some(model) = self.pivot_model.clone() else {
            self.status = "Nothing to export: build a pivot first (add a Values field).".to_string();
            return;
        };
        let (ext, filter_name, filter_ext) = match format {
            "xlsx" => ("xlsx", "Excel Workbook", "xlsx"),
            "pdf" => ("pdf", "PDF Document", "pdf"),
            _ => ("html", "HTML", "html"),
        };
        let default_name = {
            let base = model
                .title
                .chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
                .collect::<String>();
            let base = base.trim();
            if base.is_empty() { "pivot".to_string() } else { format!("{base}.{ext}") }
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(format!("Export Pivot as {}", filter_name))
            .add_filter(filter_name, &[filter_ext])
            .set_file_name(&default_name)
            .save_file()
        else {
            self.status = "Export cancelled.".to_string();
            return;
        };
        let style = ExportStyle {
            color_theme: self.table_color_theme,
            border_theme: self.table_border_theme,
            brightness: self.brightness,
        };
        let (tx, rx) = mpsc::channel::<ExportMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        self.status = format!("Exporting {ext} in background...");
        self.exporting = Some(ExportState { rx, progress: self.status.clone(), cancel });
        let title = model.title.clone();
        thread::spawn(move || {
            let _ = tx.send(ExportMsg::Progress(format!("Rendering {ext} export...")));
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(ExportMsg::Done(Err("Cancelled.".to_string())));
                return;
            }
            match pivot_export::export(&path, &model, style) {
                Ok(()) if cancel_flag.load(Ordering::Relaxed) => {
                    let _ = tx.send(ExportMsg::Done(Err("Cancelled after write.".to_string())));
                }
                Ok(()) => {
                    let _ = tx.send(ExportMsg::Done(Ok(format!("Exported {} ({ext}) to {}.", title, path.display()))));
                }
                Err(e) => {
                    let _ = tx.send(ExportMsg::Done(Err(e.to_string())));
                }
            }
        });
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
                    "Worksheet - import data to populate fields (Phase 2).".to_string()
                };
            }
            "file.import" | "file.import.csv" => self.pick_and_load(FileKind::Csv),
            "file.import.xlsx" => self.pick_and_load(FileKind::Xlsx),
            "file.import.dbf" => self.pick_and_load(FileKind::Dbf),
            "file.new" => self.new_workspace(),
            "file.open" => self.open_workspace(),
            "file.save" => self.save_workspace(false),
            "file.save_as" => self.save_workspace(true),
            "data.connect" => {
                self.on_home = false;
                self.db_panel_open = true;
                self.status = "Database: connect to SQLite and compute pivots via pushdown. Supported now: CSV, XLSX, DBF, SQLite. MySQL temporarily disabled.".to_string(); self.db_tab_mysql = false;
            }
            "file.export" => {
                let format = params.get("format").and_then(|v| v.as_str()).unwrap_or("html").to_string();
                self.export_pivot(&format);
            }
            "data.cancel_load" => { self.cancel_load(); self.cancel_export(); self.cancel_pivot(); }
            "pivot.recompute" => self.request_pivot_recompute(),
            "pivot.clear" => self.clear_layout(),
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
            "format.theme" => {
                // Legacy combined command: try color, border, or legacy mapping.
                if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
                    if let Some(theme) = TableColorThemeId::parse(id) {
                        self.table_color_theme = theme;
                        self.status = format!("Color theme set to {}.", theme.label());
                    } else if let Some(border) = BorderThemeId::parse(id) {
                        self.table_border_theme = border;
                        self.status = format!("Border theme set to {}.", border.label());
                    } else {
                        let (cid, bid) = parse_legacy_theme(id);
                        self.table_color_theme = cid;
                        self.table_border_theme = bid;
                        self.status = format!("Theme set to {} / {}.", cid.label(), bid.label());
                    }
                } else {
                    self.status = "Select a theme from Format > Color/Border Theme.".to_string();
                }
            }
            "format.color_theme" => {
                if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
                    if let Some(theme) = TableColorThemeId::parse(id) {
                        self.table_color_theme = theme;
                        self.status = format!("Color theme set to {}.", theme.label());
                    } else {
                        self.status = format!("Unknown color theme: {id}.");
                    }
                } else {
                    self.status = "Select a color theme from Format > Color Theme.".to_string();
                }
            }
            "format.border_theme" => {
                if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
                    if let Some(theme) = BorderThemeId::parse(id) {
                        self.table_border_theme = theme;
                        self.status = format!("Border theme set to {}.", theme.label());
                    } else {
                        self.status = format!("Unknown border theme: {id}.");
                    }
                } else {
                    self.status = "Select a border theme from Format > Border Theme.".to_string();
                }
            }
            "view.reset_layout" => {
                self.status = "Layout reset (placeholder).".to_string();
            }
            "help.about" => {
                self.status = "Pivot Studio - polished pivot tables (CSV/XLSX/DBF/SQLite), themes, export, .pivot workspace. MySQL temporarily disabled.".to_string();
            }
            _ => {
                self.status = format!("Command '{id}' is a placeholder.");
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
                menus::Item::Sep => { ui.separator(); }
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
                    } else if id == "format.theme" || id == "format.color_theme" {
                        params
                            .get("id")
                            .and_then(|v| v.as_str())
                            .and_then(TableColorThemeId::parse)
                            .map(|t| t == app.table_color_theme)
                    } else if id == "format.border_theme" {
                        params
                            .get("id")
                            .and_then(|v| v.as_str())
                            .and_then(BorderThemeId::parse)
                            .map(|t| t == app.table_border_theme)
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
        self.poll_pivot(ctx);
        self.poll_export(ctx);
        if self.loading.is_some() || self.pivot_computing.is_some() || self.exporting.is_some() {
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
