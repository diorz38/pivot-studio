//! Application state and egui frame/update (DesignCraft pattern, simplified).

use crate::{chrome, dock, menus};
use pivot_core::pivot::{compute, resolve_field_index};
use pivot_core::{
    Aggregate, DataTable, DataType, FieldRef, Filter, FilterOp, FooterBandSpec, PivotSpec, SubtotalConfig,
    ThemedTableModel, Value as CoreValue, ValueField,
};
use pivot_data::{FileKind, db_import::{self, CredentialMode, MySqlProfile}};
use pivot_data::workspace::{self, WorkspaceFile, WorkspaceSource, WorkspaceView, WorkspaceWorksheet, WorksheetView};
use pivot_data::{Dataset, DatasetLoadOptions, JoinType, ProjectDataModel, Relation, ResolveDiagnostics, resolve_model};
use pivot_export::ExportStyle;
use pivot_theme::{
    apply,
    table::{parse_legacy_theme, BorderThemeId, TableColorThemeId},
    Brightness, Tokens,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;

pub enum LoadMsg {
    Progress(String),
    Done(Result<LoadedTable, String>),
    /// Resolved multi-source data model (Phase 3/5 MVP).
    ModelDone(Result<(DataTable, ResolveDiagnostics), String>),
}

pub enum PreviewMsg {
    /// Async dataset schema preview result (`Box`ed: the payload is large).
    Done(Result<Box<pivot_data::DatasetPreview>, String>),
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
    /// When loading as part of workspace restore, preserve saved worksheet specs/selection
    /// and keep the user on the worksheet view instead of jumping to Source Data.
    pub preserve_workspace: bool,
}

pub enum PivotMsg {
    Done { sheet_id: String, result: Result<ThemedTableModel, String> },
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

#[derive(Debug, Clone)]
pub struct WorksheetState {
    pub id: String,
    pub name: String,
    pub number: usize,
    pub spec: PivotSpec,
    pub model: Option<ThemedTableModel>,
}

impl WorksheetState {
    pub fn new(id: String, name: String, number: usize, spec: PivotSpec) -> Self {
        Self { id, name, number, spec, model: None }
    }
    pub fn from_workspace(ws: WorkspaceWorksheet, fallback_number: usize) -> Self {
        let number = if ws.number == 0 { fallback_number } else { ws.number };
        let mut spec = ws.spec;
        if let Some(over) = spec.title_override.as_mut() {
            let prefix = format!("Table {:02}", number);
            loop {
                let stripped = over.strip_prefix(&format!("{prefix} ")).or_else(|| if over.as_str() == prefix { Some("") } else { None });
                if let Some(s) = stripped { *over = s.trim().to_string(); } else { break; }
            }
            if over.is_empty() { spec.title_override = None; }
        }
        Self { id: ws.id, name: ws.name, number, spec, model: None }
    }
    pub fn default_name(number: usize) -> String {
        format!("Sheet {:02}", number)
    }
    pub fn table_title_prefix(&self) -> String {
        format!("Table {:02}", self.number)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActivityTab {
    #[default]
    Layout,
    Files,
    Model,
    Themes,
    Export,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DiagnosticsFilter {
    #[default]
    All,
    Errors,
    Warnings,
}

pub struct PivotApp {
    pub brightness: Brightness,
    pub table_color_theme: TableColorThemeId,
    pub table_border_theme: BorderThemeId,
    pub status: String,
    brightness_applied: Option<Brightness>,
    exit_requested: bool,
    pub on_home: bool,
    pub on_source_page: bool,
    pub active_stage: usize,
    pub activity_tab: ActivityTab,
    pub show_left_panel: bool,
    pub diagnostics_open: bool,
    pub diagnostics_filter: DiagnosticsFilter,
    pub table: Option<DataTable>,
    pub source_name: Option<String>,
    pub loading: Option<LoadState>,
    pub worksheets: Vec<WorksheetState>,
    pub active_worksheet: usize,
    pub selected_columns: Option<BTreeSet<String>>,
    /// v3 project data model (datasets/relations).
    pub data_model: ProjectDataModel,
    pub data_model_diagnostics: ResolveDiagnostics,
    pub data_model_resolving: bool,
    /// Cached normalized column names per dataset id for UI pickers/previews.
    pub dataset_schema_cache: std::collections::HashMap<String, Vec<String>>,
    /// Cached dataset schema previews (normalized columns + sampled rows) for the Data Model page.
    pub dataset_preview_cache: std::collections::HashMap<String, pivot_data::DatasetPreview>,
    pub schema_loading_dataset: Option<String>,
    /// In-flight async dataset schema preview (dataset id + channel) and its cancel flag.
    pub preview_rx: Option<(String, mpsc::Receiver<PreviewMsg>)>,
    pub preview_cancel: Option<Arc<AtomicBool>>,
    pub projection_message: Option<String>,
    /// Sheet/table picker draft for the "add dataset" flow (UI state only).
    pub add_dataset_sheet: String,
    pub add_dataset_table: String,
    // Relation editor draft (simple lookup/enrichment, m00581)
    pub rel_edit_id: Option<String>,
    pub rel_base_dataset: String,
    pub rel_base_column: String,
    pub rel_lookup_dataset: String,
    pub rel_lookup_column: String,
    pub rel_label_columns: std::collections::HashSet<String>,
    pub rel_name: String,
    pub rel_message: Option<String>,
    pub pivot_computing: Option<PivotComputeState>,
    pub exporting: Option<ExportState>,
    next_sheet_no: usize,
    // Filter builder draft (UI state, applies to active worksheet)
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
    pub db_sqlite_table: String,
    pub db_mysql_host: String,
    pub db_mysql_port: String,
    pub db_mysql_db: String,
    pub db_mysql_user: String,
    pub db_mysql_table: String,
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

fn default_spec() -> PivotSpec {
    PivotSpec {
        subtotals: SubtotalConfig { row_subtotals: true, grand_total: true, column_totals: false },
        ..Default::default()
    }
}

impl PivotApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
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
            on_source_page: false,
            active_stage: 3,
            activity_tab: ActivityTab::Layout,
            show_left_panel: true,
            diagnostics_open: false,
            diagnostics_filter: DiagnosticsFilter::All,
            table: None,
            source_name: None,
            loading: None,
            worksheets: vec![WorksheetState::new("sheet-1".to_string(), WorksheetState::default_name(1), 1, default_spec())],
            active_worksheet: 0,
            selected_columns: None,
            data_model: ProjectDataModel::default(),
            data_model_diagnostics: ResolveDiagnostics::default(),
            data_model_resolving: false,
            dataset_schema_cache: std::collections::HashMap::new(),
            dataset_preview_cache: std::collections::HashMap::new(),
            preview_rx: None,
            preview_cancel: None,
            projection_message: None,
            schema_loading_dataset: None,
            add_dataset_sheet: String::new(),
            add_dataset_table: String::new(),
            rel_edit_id: None,
            rel_base_dataset: String::new(),
            rel_base_column: String::new(),
            rel_lookup_dataset: String::new(),
            rel_lookup_column: String::new(),
            rel_label_columns: std::collections::HashSet::new(),
            rel_name: String::new(),
            rel_message: None,
            pivot_computing: None,
            exporting: None,
            next_sheet_no: 2,
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
            db_sqlite_table: String::new(),
            db_mysql_host: "127.0.0.1".to_string(),
            db_mysql_port: "3306".to_string(),
            db_mysql_db: String::new(),
            db_mysql_user: String::new(),
            db_mysql_table: String::new(),
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

    pub fn set_brightness(&mut self, b: Brightness, ctx: &egui::Context) {
        self.brightness = b;
        self.status = format!("Brightness set to {}.", b.label());
        let tokens = Tokens::for_brightness(b);
        apply(ctx, &tokens);
        self.brightness_applied = Some(b);
    }

    pub fn active_sheet(&self) -> &WorksheetState {
        &self.worksheets[self.active_worksheet.min(self.worksheets.len().saturating_sub(1))]
    }
    pub fn active_sheet_mut(&mut self) -> &mut WorksheetState {
        let i = self.active_worksheet.min(self.worksheets.len().saturating_sub(1));
        &mut self.worksheets[i]
    }
    pub fn active_spec(&self) -> &PivotSpec { &self.active_sheet().spec }
    pub fn active_spec_mut(&mut self) -> &mut PivotSpec { &mut self.active_sheet_mut().spec }
    pub fn active_model(&self) -> Option<&ThemedTableModel> { self.active_sheet().model.as_ref() }

    pub fn row_count(&self) -> usize {
        self.table.as_ref().map(|t| t.row_count()).unwrap_or(0)
    }

    pub fn col_count(&self) -> usize {
        self.table.as_ref().map(|t| t.column_count()).unwrap_or(0)
    }

    pub fn uses_multi_source_model(&self) -> bool {
        self.data_model.datasets.iter().filter(|d| d.enabled).count() > 1
            || (self.data_model.datasets.iter().filter(|d| d.enabled).count() == 1 && !self.data_model.relations.is_empty())
    }

    pub fn available_columns(&self) -> Vec<(String, DataType)> {
        let Some(table) = &self.table else { return Vec::new() };
        if self.uses_multi_source_model() || (self.data_model.exposed_columns.as_ref().map_or(true, |v| v.is_empty()) && self.table.as_ref().is_some_and(|t| t.columns.iter().any(|c| c.name.contains('.')))) {
            return table.columns.iter().map(|c| (c.name.clone(), c.effective_type())).collect();
        }
        table
            .columns
            .iter()
            .filter(|c| self.is_column_selected(&c.name))
            .map(|c| (c.name.clone(), c.effective_type()))
            .collect()
    }

    pub fn is_column_selected(&self, name: &str) -> bool {
        // Resolved multi-source tables expose columns directly; treat all as selected.
        if self.table.as_ref().is_some_and(|t| t.columns.iter().any(|c| c.name.contains('.'))) && self.uses_multi_source_model() {
            return true;
        }
        if let Some(idx) = self.table.as_ref().and_then(|t| resolve_field_index(t, name)) {
            let canon = self.table.as_ref().unwrap().columns[idx].name.as_str();
            return match &self.selected_columns {
                None => true,
                Some(set) => set.contains(canon) || set.contains(name),
            };
        }
        match &self.selected_columns {
            None => true,
            Some(set) => set.contains(name),
        }
    }

    pub fn set_column_selected(&mut self, name: &str, selected: bool) {
        let Some(table) = &self.table else { return };
        if self.selected_columns.is_none() {
            self.selected_columns = Some(table.columns.iter().map(|c| c.name.clone()).collect());
        }
        if let Some(set) = &mut self.selected_columns {
            if selected { set.insert(name.to_string()); } else { set.remove(name); }
        }
        for ws in &mut self.worksheets {
            ws.model = None;
        }
        crate::devlog!("set_column_selected: '{}'={} selected_count={:?} (cleared models, specs preserved) sheets=[{}]",
            name, selected, self.selected_columns.as_ref().map(|s| s.len()), self.debug_sheet_summary());
    }

    pub fn select_all_columns(&mut self, selected: bool) {
        let Some(table) = &self.table else { return };
        self.selected_columns = Some(if selected {
            table.columns.iter().map(|c| c.name.clone()).collect()
        } else {
            BTreeSet::new()
        });
        for ws in &mut self.worksheets { ws.model = None; }
        crate::devlog!("select_all_columns: {} selected_count={:?} (cleared models, specs preserved) sheets=[{}]",
            selected, self.selected_columns.as_ref().map(|s| s.len()), self.debug_sheet_summary());
    }

    pub fn stale_spec_refs(&self, spec: &PivotSpec) -> Vec<String> {
        let Some(table) = &self.table else { return Vec::new() };
        let mut refs = Vec::new();
        let mut push = |name: &str| {
            let ok = resolve_field_index(table, name).is_some() && self.is_column_selected(name);
            if !ok && !refs.iter().any(|r: &String| r == name) {
                refs.push(name.to_string());
            }
        };
        for f in &spec.rows { push(&f.name); }
        for f in &spec.columns { push(&f.name); }
        for v in &spec.values { push(&v.field.name); }
        for f in &spec.filters { push(&f.field.name); }
        refs
    }

    pub fn sync_layout_pickers(&mut self) {
        let fields = self.available_columns();
        if fields.is_empty() { return; }
        if self.pick_row_field.is_empty() || !fields.iter().any(|(n, _)| n == &self.pick_row_field) {
            self.pick_row_field = fields[0].0.clone();
            self.pick_row_as_type = fields[0].1;
        }
        if self.pick_col_field.is_empty() || !fields.iter().any(|(n, _)| n == &self.pick_col_field) {
            self.pick_col_field = fields[0].0.clone();
            self.pick_col_as_type = fields[0].1;
        }
        if self.pick_val_field.is_empty() || !fields.iter().any(|(n, _)| n == &self.pick_val_field) {
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
        if let Some(i) = idx { self.reinterpret_column(i, target); }
    }

    pub fn poll_loading(&mut self, ctx: &egui::Context) {
        enum Finished { File(Result<LoadedTable, String>), Model(Result<(DataTable, ResolveDiagnostics), String>) }
        let mut finished: Option<Finished> = None;
        if let Some(load) = &mut self.loading {
            while let Ok(msg) = load.rx.try_recv() {
                match msg {
                    LoadMsg::Progress(s) => { load.progress = s.clone(); self.status = s; }
                    LoadMsg::Done(res) => { finished = Some(Finished::File(res)); break; }
                    LoadMsg::ModelDone(res) => { finished = Some(Finished::Model(res)); break; }
                }
            }
        }
        if let Some(f) = finished {
            let preserve = self.loading.as_ref().map(|l| l.preserve_workspace).unwrap_or(false);
            self.loading = None;
            self.data_model_resolving = false;
            match f {
                Finished::File(res) => match res {
                    Ok(loaded) => {
                        self.source_name = Some(loaded.name);
                        self.loaded_file_path = Some(loaded.path);
                        self.loaded_file_kind = Some(loaded.kind);
                        if !preserve || self.selected_columns.is_none() {
                            self.selected_columns = Some(loaded.table.columns.iter().map(|c| c.name.clone()).collect());
                        } else if let (Some(sel), Some(table)) = (&mut self.selected_columns, Some(&loaded.table)) {
                            let valid: std::collections::BTreeSet<String> = table.columns.iter().map(|c| c.name.clone()).collect();
                            sel.retain(|c| valid.contains(c));
                            if sel.is_empty() { *sel = valid; }
                        }
                        self.table = Some(loaded.table);
                        self.db_source = None;
                        for ws in &mut self.worksheets { ws.model = None; }
                        self.on_home = false;
                        self.on_source_page = !preserve;
                        self.status = if preserve {
                            format!("Workspace source loaded ({} rows, {} cols).", self.row_count(), self.col_count())
                        } else {
                            format!("Loaded '{}' ({} rows, {} cols). Review source columns, then open a worksheet.",
                                self.source_name.as_deref().unwrap_or("data"), self.row_count(), self.col_count())
                        };
                    }
                    Err(e) => { self.status = format!("Load failed: {e}"); }
                },
                Finished::Model(res) => match res {
                    Ok((mut table, diag)) => {
                        table.source = Some("data-model".to_string());
                        self.data_model_diagnostics = diag.clone();
                        self.table = Some(table);
                        self.db_source = None;
                        self.source_name = Some("Data Model".to_string());
                        for ws in &mut self.worksheets { ws.model = None; }
                        self.on_home = false;
                        self.sync_layout_pickers();
                        let warns = diag.warnings.len();
                        let errs = diag.errors.len();
                        self.status = if errs > 0 {
                            format!("Data model resolved with {errs} error(s), {warns} warning(s), {} row(s). Fix errors before relying on results.", self.row_count())
                        } else {
                            format!("Data model resolved: {} row(s), {} column(s), {warns} warning(s).", self.row_count(), self.col_count())
                        };
                        // Persist-stable identity: upgrade legacy worksheet refs to alias.column.
                        self.canonicalize_spec_refs();
                    }
                    Err(e) => { self.data_model_diagnostics = ResolveDiagnostics::default(); self.status = format!("Data model resolve failed: {e}"); }
                },
            }
            ctx.request_repaint();
        }
    }

    pub fn poll_pivot(&mut self, ctx: &egui::Context) {
        let finished = self
            .pivot_computing
            .as_mut()
            .and_then(|state| state.rx.try_recv().ok());
        if let Some(PivotMsg::Done { sheet_id, result }) = finished {
            self.pivot_computing = None;
            match result {
                Ok(mut model) => {
                    crate::devlog!("poll_pivot: done sheet_id={} ok body_rows={}", sheet_id, model.body_rows.len());
                    let msg = format!(
                        "Pivot updated: {} body row(s), {} footer band(s).",
                        model.body_rows.len(),
                        model.footer_bands.len()
                    );
                    let mut applied_name = None;
                    if let Some(ws) = self.worksheets.iter_mut().find(|w| w.id == sheet_id) {
                        let prefix = ws.table_title_prefix();
                        let base = model.title
                            .strip_prefix(&format!("{prefix} "))
                            .or_else(|| model.title.strip_prefix(&prefix))
                            .unwrap_or(&model.title)
                            .trim();
                        model.title = if base.is_empty() { prefix } else { format!("{prefix} {base}") };
                        applied_name = Some(ws.name.clone());
                        ws.model = Some(model);
                    }
                    if let Some(name) = applied_name {
                        crate::devlog!("poll_pivot: applied sheet='{}' sheets=[{}]", name, self.debug_sheet_summary());
                    } else {
                        crate::devlog!("poll_pivot: sheet_id={} not found (closed?)", sheet_id);
                    }
                    self.status = msg;
                }
                Err(e) => {
                    crate::devlog!("poll_pivot: failed sheet_id={} err={e}", sheet_id);
                    self.status = format!("Pivot compute failed: {e}");
                }
            }
            ctx.request_repaint();
        }
    }

    fn start_load(&mut self, path: PathBuf, kind: FileKind) {
        self.start_load_inner(path, kind, false);
    }

    fn start_load_for_workspace(&mut self, path: PathBuf, kind: FileKind) {
        self.start_load_inner(path, kind, true);
    }

    fn start_load_inner(&mut self, path: PathBuf, kind: FileKind, preserve_workspace: bool) {
        crate::devlog!("start_load: path={} kind={:?} preserve_workspace={} current_sheets=[{}]", path.display(), kind, preserve_workspace, self.debug_sheet_summary());
        if self.loading.is_some() { self.status = "A load is already in progress.".to_string(); return; }
        let (tx, rx) = mpsc::channel::<LoadMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
        self.status = format!("Loading {name}...");
        self.loading = Some(LoadState { rx, progress: self.status.clone(), cancel, preserve_workspace });
        thread::spawn(move || {
            let _ = tx.send(LoadMsg::Progress(format!("Reading {name}...")));
            if cancel_flag.load(Ordering::Relaxed) { let _ = tx.send(LoadMsg::Done(Err("Cancelled.".to_string()))); return; }
            let result = pivot_data::load_file(&path, kind);
            if cancel_flag.load(Ordering::Relaxed) { let _ = tx.send(LoadMsg::Done(Err("Cancelled.".to_string()))); return; }
            match result {
                Ok(mut table) => {
                    if table.source.is_none() { table.source = Some(path.to_string_lossy().into_owned()); }
                    let _ = tx.send(LoadMsg::Progress(format!("Loaded {} rows.", table.row_count())));
                    let _ = tx.send(LoadMsg::Done(Ok(LoadedTable { name, path, kind, table })));
                }
                Err(e) => { let _ = tx.send(LoadMsg::Done(Err(e.to_string()))); }
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
        for (name, exts) in filters { dialog = dialog.add_filter(name, exts); }
        if let Some(path) = dialog.pick_file() { self.start_load(path, kind); }
        else { self.status = "Import cancelled.".to_string(); }
    }

    /// Add a dataset to the multi-source data model (Phase 2/5 MVP) without replacing the current resolved table.
    pub fn pick_and_add_dataset(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("Add dataset (CSV/XLSX/DBF)")
            .add_filter("Data", &["csv","tsv","txt","xlsx","xls","xlsm","dbf"]).pick_file() {
            self.add_dataset_from_file(path);
        }
    }

    #[allow(dead_code)]
    pub fn cancel_load(&mut self) {
        if let Some(load) = &self.loading { load.cancel.store(true, Ordering::Relaxed); self.status = "Cancelling load...".to_string(); }
    }
    pub fn cancel_pivot(&mut self) {
        if let Some(state) = &self.pivot_computing { state.cancel.store(true, Ordering::Relaxed); self.status = "Cancelling pivot...".to_string(); }
    }
    pub fn cancel_export(&mut self) {
        if let Some(state) = &self.exporting { state.cancel.store(true, Ordering::Relaxed); self.status = "Cancelling export...".to_string(); }
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

    #[allow(dead_code)]
    pub fn reinterpret_column(&mut self, col_idx: usize, target: DataType) {
        if let Some(table) = &mut self.table {
            if let Some(col) = table.columns.get_mut(col_idx) { col.override_type = Some(target); }
            table.reinterpret_column(col_idx);
            for ws in &mut self.worksheets { ws.model = None; }
            self.status = format!("Column type updated ({} rows). Recompute pivot if needed.", table.row_count());
        }
    }

    #[allow(dead_code)]
    pub fn db_pick_sqlite_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("Select SQLite database").add_filter("SQLite", &["db", "sqlite", "sqlite3"]).pick_file() {
            self.db_sqlite_path = path.to_string_lossy().to_string();
            self.db_sqlite_table.clear();
            self.refresh_sqlite_tables();
        }
    }

    #[allow(dead_code)]
    pub fn refresh_sqlite_tables(&mut self) {
        if self.db_sqlite_path.trim().is_empty() { return; }
        match db_import::list_sqlite_tables(&PathBuf::from(self.db_sqlite_path.trim())) {
            Ok(t) => {
                let count = t.len();
                if self.db_sqlite_table.is_empty() {
                    self.db_sqlite_table = t.first().cloned().unwrap_or_default();
                }
                self.status = format!("Found {count} SQLite table(s).");
            }
            Err(e) => self.status = format!("SQLite error: {e}"),
        }
    }

    #[allow(dead_code)]
    pub fn db_preview_sqlite(&mut self) {
        let path = PathBuf::from(self.db_sqlite_path.trim());
        let table = self.db_sqlite_table.trim().to_string();
        if table.is_empty() { self.status = "Choose a SQLite table.".to_string(); return; }
        match db_import::connect_and_preview_sqlite(&path, &table, 500, None) {
            Ok(t) => {
                self.source_name = Some(format!("sqlite:{}:{}", path.display(), table));
                self.selected_columns = Some(t.columns.iter().map(|c| c.name.clone()).collect());
                self.table = Some(t);
                self.db_source = Some(DbSource::Sqlite { path, table });
                self.loaded_file_path = None;
                self.loaded_file_kind = None;
                for ws in &mut self.worksheets { ws.model = None; }
                self.sync_layout_pickers();
                self.status = format!("Loaded SQLite preview ({} rows, {} cols). Review source columns, then open a worksheet.", self.row_count(), self.col_count());
                self.on_home = false;
                self.on_source_page = true;
            }
            Err(e) => self.status = format!("SQLite preview failed: {e}"),
        }
    }

    #[allow(dead_code)]
    pub fn mysql_credential_mode(&self) -> CredentialMode {
        if self.db_mysql_cred_mode_session { CredentialMode::SessionOnly } else { CredentialMode::OsKeychain }
    }
    #[allow(dead_code)]
    pub fn db_apply_mysql_profile(&mut self) -> Option<MySqlProfile> {
        self.status = "MySQL is temporarily disabled. Use CSV/XLSX/DBF or SQLite.".to_string();
        None
    }
    #[allow(dead_code)]
    pub fn db_preview_mysql(&mut self) {
        self.status = "MySQL is temporarily disabled. Use CSV/XLSX/DBF or SQLite.".to_string();
    }

    pub fn request_resolve_model(&mut self) {
        if self.data_model_resolving { return; }
        let enabled = self.data_model.datasets.iter().filter(|d| d.enabled && !matches!(d.source, WorkspaceSource::None)).count();
        if enabled == 0 { self.status = "Add at least one dataset in Data Model.".to_string(); return; }
        let model = self.data_model.clone();
        let (tx, rx) = mpsc::channel::<LoadMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        self.data_model_resolving = true;
        self.status = "Resolving data model (loading datasets/joins)...".to_string();
        let cancel_flag = Arc::clone(&cancel);
        thread::spawn(move || {
            let res = resolve_model(&model, &DatasetLoadOptions::default(), Some(cancel_flag.as_ref()));
            let _ = tx.send(LoadMsg::ModelDone(res.map(|r| (r.table, r.diagnostics)).map_err(|e| e.to_string())));
        });
        if let Some(load) = &self.loading { load.cancel.store(true, Ordering::Relaxed); }
        self.loading = Some(LoadState { rx, progress: "Resolving data model...".into(), cancel, preserve_workspace: true });
    }

    pub fn add_dataset_from_file(&mut self, path: PathBuf) {
        self.add_dataset_from_file_with_sheet(path, None);
    }

    /// True when the project already models datasets: importing another source then
    /// enriches the data model instead of replacing the single legacy table.
    pub fn import_adds_dataset(&self) -> bool {
        !self.data_model.datasets.is_empty()
    }

    pub fn add_dataset_from_file_with_sheet(&mut self, path: PathBuf, sheet: Option<String>) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let file_kind = match ext.as_str() { "tsv" => "tsv", "xlsx"|"xls"|"xlsm" => "xlsx", "dbf" => "dbf", _ => "csv" }.to_string();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("dataset");
        let alias = pivot_data::normalize::normalize_identifier(stem, 1);
        let mut base = alias.clone(); let mut n=2u32;
        while self.data_model.datasets.iter().any(|d| d.alias.eq_ignore_ascii_case(&base)) { base = format!("{alias}_{n}"); n+=1; }
        let id = format!("ds{}", self.data_model.datasets.len() + 1);
        self.data_model.datasets.push(Dataset::from_legacy_source(id, base.clone(), WorkspaceSource::File { path: path.to_string_lossy().to_string(), file_kind: Some(file_kind), sheet }));
        if self.data_model.base_dataset_id.is_none() { self.data_model.base_dataset_id = self.data_model.datasets.first().map(|d| d.id.clone()); }
        for ws in &mut self.worksheets { ws.model = None; }
        self.status = format!("Added dataset '{base}'. Configure relations/columns, then Resolve Data Model.");
        self.on_source_page = true;
    }

    pub fn add_dataset_sqlite(&mut self, path: PathBuf, table: String) {
        let alias = pivot_data::normalize::normalize_identifier(&table, 1);
        let mut base = alias.clone(); let mut n=2u32;
        while self.data_model.datasets.iter().any(|d| d.alias.eq_ignore_ascii_case(&base)) { base = format!("{alias}_{n}"); n+=1; }
        let id = format!("ds{}", self.data_model.datasets.len() + 1);
        self.data_model.datasets.push(Dataset::from_legacy_source(id, base.clone(), WorkspaceSource::Sqlite { path: path.to_string_lossy().to_string(), table }));
        if self.data_model.base_dataset_id.is_none() { self.data_model.base_dataset_id = self.data_model.datasets.first().map(|d| d.id.clone()); }
        for ws in &mut self.worksheets { ws.model = None; }
        self.status = format!("Added SQLite dataset '{base}'. Resolve Data Model to use it in pivots.");
        self.on_source_page = true;
    }

    pub fn dataset_by_id(&self, id: &str) -> Option<&Dataset> {
        self.data_model.datasets.iter().find(|d| d.id == id)
    }

    pub fn dataset_columns(&self, dataset_id: &str) -> &[String] {
        self.dataset_schema_cache.get(dataset_id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn load_dataset_schema(&mut self, dataset_id: &str) {
        let Some(ds) = self.dataset_by_id(dataset_id).cloned() else {
            self.status = format!("Dataset '{}' not found.", dataset_id);
            return;
        };
        if let Some((pending, _)) = &self.preview_rx {
            self.status = format!("A schema preview is already loading (dataset '{}').", pending);
            return;
        }
        let (tx, rx) = mpsc::channel::<PreviewMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        let id = dataset_id.to_string();
        self.schema_loading_dataset = Some(id.clone());
        self.preview_rx = Some((id, rx));
        self.preview_cancel = Some(cancel);
        self.status = format!("Loading schema preview for '{}'...", ds.alias);
        thread::spawn(move || {
            let opts = DatasetLoadOptions { row_limit: Some(200), preview_rows: 20, preview_only: true };
            let res = pivot_data::preview_dataset_schema(&ds, &opts, Some(&cancel_flag))
                .map(Box::new).map_err(|e| e.to_string());
            let _ = tx.send(PreviewMsg::Done(res));
        });
    }

    /// Drain the async schema preview result (UI thread).
    pub fn poll_preview(&mut self, ctx: &egui::Context) {
        let Some((dataset_id, rx)) = self.preview_rx.as_ref() else { return };
        let Ok(PreviewMsg::Done(res)) = rx.try_recv() else { return };
        let dataset_id = dataset_id.clone();
        self.preview_rx = None;
        self.preview_cancel = None;
        self.schema_loading_dataset = None;
        match res {
            Ok(preview) => {
                let preview = *preview;
                let cols = preview.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
                let note = if preview.capped { format!(" (capped at {} rows)", preview.row_count) } else { String::new() };
                let truncated = if preview.truncated { ", preview rows truncated" } else { "" };
                self.status = format!("Loaded {} column(s) for dataset '{}'{note}{truncated}.", preview.columns.len(), preview.alias);
                self.dataset_schema_cache.insert(dataset_id.clone(), cols);
                self.dataset_preview_cache.insert(dataset_id, preview);
                self.ensure_rel_draft_defaults();
            }
            Err(e) => { self.status = format!("Failed to load schema preview: {e}"); }
        }
        ctx.request_repaint();
    }

    pub fn cancel_preview(&mut self) {
        if let Some(c) = &self.preview_cancel { c.store(true, Ordering::Relaxed); }
        self.preview_cancel = None;
        self.preview_rx = None;
        self.schema_loading_dataset = None;
        self.status = "Schema preview cancelled.".to_string();
    }

    /// Migrate persisted worksheet field refs onto the resolved model's canonical
    /// identity (`alias.column`) so specs keep working after datasets are renamed or
    /// reordered. Refs that do not resolve are left untouched (stale detection reports
    /// them). Only applies to multi-source models; legacy single-source specs are
    /// preserved as-is for v1/v2 parity.
    pub fn canonicalize_spec_refs(&mut self) -> usize {
        if !self.uses_multi_source_model() { return 0; }
        let Some(table) = self.table.clone() else { return 0; };
        let mut changed = 0usize;
        let fix = |name: &mut String, changed: &mut usize| {
            if let Some(canon) = pivot_core::pivot::canonical_field_name(&table, name) {
                if canon != *name { *name = canon; *changed += 1; }
            }
        };
        for ws in &mut self.worksheets {
            for f in &mut ws.spec.rows { fix(&mut f.name, &mut changed); }
            for f in &mut ws.spec.columns { fix(&mut f.name, &mut changed); }
            for v in &mut ws.spec.values { fix(&mut v.field.name, &mut changed); }
            for f in &mut ws.spec.filters { fix(&mut f.field.name, &mut changed); }
        }
        if changed > 0 {
            self.status = format!("Migrated {changed} worksheet field reference(s) to resolved identity (alias.column).");
        }
        changed
    }

    pub fn ensure_rel_draft_defaults(&mut self) {
        let ids = self.data_model.datasets.iter().map(|d| d.id.clone()).collect::<Vec<_>>();
        if ids.is_empty() {
            self.rel_base_dataset.clear(); self.rel_base_column.clear();
            self.rel_lookup_dataset.clear(); self.rel_lookup_column.clear();
            self.rel_label_columns.clear();
            return;
        }
        let base_default = self.data_model.base_dataset_id.clone().filter(|id| ids.contains(id)).unwrap_or_else(|| ids[0].clone());
        if self.rel_base_dataset.is_empty() || !ids.contains(&self.rel_base_dataset) { self.rel_base_dataset = base_default; }
        if self.rel_lookup_dataset.is_empty() || !ids.contains(&self.rel_lookup_dataset) || self.rel_lookup_dataset == self.rel_base_dataset {
            if let Some(other) = ids.iter().find(|id| **id != self.rel_base_dataset) { self.rel_lookup_dataset = other.clone(); }
            else { self.rel_lookup_dataset = self.rel_base_dataset.clone(); }
        }
        let base_cols = self.dataset_columns(&self.rel_base_dataset).to_vec();
        if !base_cols.is_empty() && (self.rel_base_column.is_empty() || !base_cols.iter().any(|c| c == &self.rel_base_column)) {
            self.rel_base_column = base_cols[0].clone();
        }
        let lookup_cols = self.dataset_columns(&self.rel_lookup_dataset).to_vec();
        if !lookup_cols.is_empty() && (self.rel_lookup_column.is_empty() || !lookup_cols.iter().any(|c| c == &self.rel_lookup_column)) {
            self.rel_lookup_column = lookup_cols[0].clone();
        }
        self.rel_label_columns.retain(|c| lookup_cols.is_empty() || lookup_cols.iter().any(|x| x == c));
    }


    pub fn clear_relation_draft(&mut self) {
        self.rel_edit_id = None;
        self.rel_name.clear();
        self.rel_base_column.clear();
        self.rel_lookup_column.clear();
        self.rel_label_columns.clear();
        self.rel_message = None;
        self.ensure_rel_draft_defaults();
    }

    pub fn save_relation_from_draft(&mut self) {
        self.ensure_rel_draft_defaults();
        if self.rel_base_dataset.is_empty() || self.rel_lookup_dataset.is_empty() {
            self.rel_message = Some("Select base and lookup datasets.".to_string()); return;
        }
        if self.rel_base_dataset == self.rel_lookup_dataset {
            self.rel_message = Some("Base and lookup datasets must be different.".to_string()); return;
        }
        if self.rel_base_column.trim().is_empty() || self.rel_lookup_column.trim().is_empty() {
            self.rel_message = Some("Select base key and lookup key columns (load dataset columns first).".to_string()); return;
        }
        let valid_labels: Vec<String> = self.rel_label_columns.iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != self.rel_lookup_column.trim())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        if valid_labels.is_empty() && !self.dataset_columns(&self.rel_lookup_dataset).is_empty() {
            self.rel_message = Some("Select at least one lookup label column (non-key).".to_string()); return;
        }
        {
            let loaded = self.dataset_columns(&self.rel_lookup_dataset);
            if !loaded.is_empty() {
                let missing = valid_labels.iter().find(|c| !loaded.iter().any(|x| x == *c));
                if let Some(m) = missing {
                    let lookup_alias2 = self.dataset_by_id(&self.rel_lookup_dataset).map(|d| d.alias.clone()).unwrap_or_else(|| self.rel_lookup_dataset.clone());
                    self.rel_message = Some(format!("Lookup label column '{}' not found in loaded columns for '{}'.", m, lookup_alias2)); return;
                }
            }
        }
        let base_alias = self.dataset_by_id(&self.rel_base_dataset).map(|d| d.alias.clone()).unwrap_or_else(|| self.rel_base_dataset.clone());
        let lookup_alias = self.dataset_by_id(&self.rel_lookup_dataset).map(|d| d.alias.clone()).unwrap_or_else(|| self.rel_lookup_dataset.clone());
        let rel = Relation {
            id: self.rel_edit_id.clone().unwrap_or_else(|| format!("rel{}", self.data_model.relations.len() + 1 + (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 10000).unwrap_or(0)) as usize)),
            name: if self.rel_name.trim().is_empty() { Some(format!("{base_alias}->{lookup_alias}")) } else { Some(self.rel_name.trim().to_string()) },
            left_dataset_id: self.rel_base_dataset.clone(),
            left_column: self.rel_base_column.trim().to_string(),
            right_dataset_id: self.rel_lookup_dataset.clone(),
            right_column: self.rel_lookup_column.trim().to_string(),
            join_type: JoinType::Lookup,
            label_columns: if valid_labels.is_empty() { None } else {
                let mut v = valid_labels.clone();
                v.sort();
                v.dedup();
                Some(v)
            },
            enabled: true,
        };
        // simple validation against loaded schema when available
        let base_cols = self.dataset_columns(&rel.left_dataset_id);
        if !base_cols.is_empty() && !base_cols.iter().any(|c| c == &rel.left_column) {
            self.rel_message = Some(format!("Base key '{}' not found in loaded columns for '{}'.", rel.left_column, base_alias)); return;
        }
        let lookup_cols = self.dataset_columns(&rel.right_dataset_id);
        if !lookup_cols.is_empty() && !lookup_cols.iter().any(|c| c == &rel.right_column) {
            self.rel_message = Some(format!("Lookup key '{}' not found in loaded columns for '{}'.", rel.right_column, lookup_alias)); return;
        }
        if let Some(existing) = self.rel_edit_id.as_ref() {
            if let Some(slot) = self.data_model.relations.iter_mut().find(|r| &r.id == existing) { *slot = rel.clone(); }
        } else {
            self.data_model.relations.push(rel.clone());
        }
        for ws in &mut self.worksheets { ws.model = None; }
        self.table = None;
        self.rel_message = Some(format!("Saved lookup relation {}. Resolve Data Model to validate/apply (duplicate lookup keys are rejected; base row count is preserved).", rel.name.as_deref().unwrap_or(&rel.id)));
        self.status = "Relation saved. Resolve Data Model to apply it.".to_string();
        self.clear_relation_draft();
        // keep selection convenience
        self.rel_base_dataset = rel.left_dataset_id;
        self.rel_lookup_dataset = rel.right_dataset_id;
        self.ensure_rel_draft_defaults();
    }

    pub fn remove_relation(&mut self, rel_id: &str) {
        let before = self.data_model.relations.len();
        self.data_model.relations.retain(|r| r.id != rel_id);
        if self.data_model.relations.len() != before {
            for ws in &mut self.worksheets { ws.model = None; }
            self.table = None;
            if self.rel_edit_id.as_deref() == Some(rel_id) { self.clear_relation_draft(); }
            self.status = "Relation removed. Resolve Data Model to refresh results.".to_string();
        }
    }

    pub fn request_pivot_recompute(&mut self) {
        let sheet_id = self.active_sheet().id.clone();
        let spec = self.active_spec().clone();
        crate::devlog!("request_pivot_recompute: active={} sheet_id={} r{}c{}v{}f{} sheets=[{}]",
            self.active_worksheet, sheet_id, spec.rows.len(), spec.columns.len(), spec.values.len(), spec.filters.len(), self.debug_sheet_summary());
        if self.table.is_none() && self.db_source.is_none() {
            self.status = "Load data before computing a pivot. (For multi-source projects, resolve the data model first.)".to_string(); return;
        }
        if self.pivot_computing.is_some() {
            self.status = "Pivot computation already in progress.".to_string(); return;
        }
        let stale = self.stale_spec_refs(&spec);
        if !stale.is_empty() {
            crate::devlog!("request_pivot_recompute: blocked stale_refs={:?}", stale);
            self.status = format!("Cannot compute: pivot references unavailable/unselected columns: {}. Re-enable them in Source Data or update the layout.", stale.join(", "));
            return;
        }
        if spec.values.is_empty() {
            self.status = "Pivot needs a Values field: pick a field and click Sum, Count, or Avg.".to_string();
            self.active_sheet_mut().model = None;
            return;
        }
        let table = self.table.clone();
        let db = self.db_source.clone();
        let (tx, rx) = mpsc::channel::<PivotMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        self.status = if db.is_some() { "Computing pivot via database pushdown (SQL GROUP BY)...".to_string() } else { "Computing pivot...".to_string() };
        let sid = sheet_id.clone();
        self.pivot_computing = Some(PivotComputeState { rx, cancel });
        thread::spawn(move || {
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(PivotMsg::Done {
                    sheet_id: sid,
                    result: Err("Cancelled.".to_string()),
                });
                return;
            }
            let res = if let Some(src) = db {
                match src {
                    DbSource::Sqlite { path, table: tname } => {
                        db_import::execute_pivot_sqlite(&path, &tname, &spec, Some(cancel_flag.as_ref()))
                            .map_err(|e| e.to_string())
                    }
                    DbSource::MySql(profile) => {
                        db_import::execute_pivot_mysql(&profile, &spec, Some(cancel_flag.as_ref()))
                            .map_err(|e| e.to_string())
                    }
                }
            } else if let Some(table) = table {
                Ok(compute(&table, &spec))
            } else {
                Err("No data source.".to_string())
            };
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(PivotMsg::Done {
                    sheet_id: sid,
                    result: Err("Cancelled.".to_string()),
                });
                return;
            }
            let _ = tx.send(PivotMsg::Done {
                sheet_id: sid,
                result: res,
            });
        });
    }

    pub fn add_worksheet(&mut self) {
        let before = self.debug_sheet_summary();
        let n = self.next_sheet_no;
        self.next_sheet_no += 1;
        let name = WorksheetState::default_name(n);
        let new_id = format!("sheet-{n}-{}", self.worksheets.len() + 1);
        self.worksheets.push(WorksheetState::new(new_id, name.clone(), n, default_spec()));
        self.active_worksheet = self.worksheets.len() - 1;
        self.on_home = false;
        self.on_source_page = false;
        self.sync_layout_pickers();
        crate::devlog!("add_worksheet: before=[{}] after=[{}] active={}", before, self.debug_sheet_summary(), self.active_worksheet);
        self.status = format!("Added worksheet '{name}'.");
    }

    pub fn debug_sheet_summary(&self) -> String {
        self.worksheets.iter().enumerate().map(|(i,w)| {
            format!("{}:{}#{}[{}] r{}c{}v{}f{}{}", i, w.name, w.number, w.id, w.spec.rows.len(), w.spec.columns.len(), w.spec.values.len(), w.spec.filters.len(), if w.model.is_some() { "[M]" } else { "" })
        }).collect::<Vec<_>>().join(" | ")
    }

    pub fn select_worksheet(&mut self, idx: usize) {
        if idx < self.worksheets.len() {
            crate::devlog!("select_worksheet: from={} to={} sheets=[{}]", self.active_worksheet, idx, self.debug_sheet_summary());
            self.active_worksheet = idx;
            self.on_home = false;
            self.on_source_page = false;
            self.sync_layout_pickers();
        }
    }

    pub fn close_worksheet(&mut self, idx: usize) {
        if self.worksheets.len() <= 1 || idx >= self.worksheets.len() { return; }
        crate::devlog!("close_worksheet: idx={} before=[{}]", idx, self.debug_sheet_summary());
        let name = self.worksheets[idx].name.clone();
        let was_active = self.active_worksheet == idx;
        self.worksheets.remove(idx);
        if self.active_worksheet >= self.worksheets.len() {
            self.active_worksheet = self.worksheets.len() - 1;
        } else if self.active_worksheet > idx {
            self.active_worksheet -= 1;
        } else if was_active {
            self.active_worksheet = idx.min(self.worksheets.len().saturating_sub(1));
        }
        self.sync_layout_pickers();
        crate::devlog!("close_worksheet: after=[{}] active={}", self.debug_sheet_summary(), self.active_worksheet);
        self.status = format!("Closed worksheet '{name}'.");
    }

    pub fn rename_worksheet(&mut self, idx: usize, name: String) {
        if let Some(ws) = self.worksheets.get_mut(idx) {
            let name = name.trim();
            if !name.is_empty() { ws.name = name.to_string(); }
        }
    }

    pub fn add_row_field(&mut self, name: String) {
        if !self.active_spec().rows.iter().any(|f| f.name == name) {
            let n = name.clone();
            self.active_spec_mut().rows.push(FieldRef::new(name));
            if self.active_spec().values.is_empty() {
                self.status = format!("Row '{n}' added. Now add a Values field (Sum/Count/Avg) to compute the pivot.");
                self.active_sheet_mut().model = None;
            } else { self.request_pivot_recompute(); }
        } else { self.status = format!("'{name}' is already in Rows."); }
    }
    pub fn add_col_field(&mut self, name: String) {
        if !self.active_spec().columns.iter().any(|f| f.name == name) {
            let n = name.clone();
            let spec = self.active_spec_mut();
            spec.columns.push(FieldRef::new(name));
            spec.column_subtotals.push(false);
            let has_vals = !spec.values.is_empty();
            let _ = spec;
            if !has_vals {
                self.status = format!("Column '{n}' added. Now add a Values field (Sum/Count/Avg) to compute the pivot.");
                self.active_sheet_mut().model = None;
            } else { self.request_pivot_recompute(); }
        } else { self.status = format!("'{name}' is already in Columns."); }
    }
    pub fn add_value_field(&mut self, name: String, aggregate: Aggregate) {
        self.active_spec_mut().values.push(ValueField { field: FieldRef::new(name.clone()), aggregate, display_name: None });
        self.status = format!("Value {}({name}) added. Computing pivot...", aggregate.label());
        self.request_pivot_recompute();
    }
    pub fn remove_row_field(&mut self, idx: usize) {
        if idx < self.active_spec().rows.len() { self.active_spec_mut().rows.remove(idx); self.request_pivot_recompute(); }
    }
    pub fn remove_col_field(&mut self, idx: usize) {
        let spec = self.active_spec_mut();
        if idx < spec.columns.len() {
            spec.columns.remove(idx);
            if idx < spec.column_subtotals.len() { spec.column_subtotals.remove(idx); }
            self.request_pivot_recompute();
        }
    }
    pub fn remove_value_field(&mut self, idx: usize) {
        if idx < self.active_spec().values.len() {
            self.active_spec_mut().values.remove(idx);
            if self.active_spec().values.is_empty() {
                self.active_sheet_mut().model = None;
                self.status = "Values cleared. Add a Values field to compute a pivot.".to_string();
            } else { self.request_pivot_recompute(); }
        }
    }
    pub fn move_row_field(&mut self, from: usize, to: usize) {
        let len = self.active_spec().rows.len();
        if from < len && to < len && from != to { let item = self.active_spec_mut().rows.remove(from); self.active_spec_mut().rows.insert(to, item); self.request_pivot_recompute(); }
    }
    pub fn move_col_field(&mut self, from: usize, to: usize) {
        let spec = self.active_spec_mut();
        let len = spec.columns.len();
        if from < len && to < len && from != to {
            let item = spec.columns.remove(from);
            spec.columns.insert(to, item);
            let mut subs = spec.effective_column_subtotals();
            if from < subs.len() && to < subs.len() {
                let s = subs.remove(from);
                subs.insert(to, s);
                spec.column_subtotals = subs;
            }
            self.request_pivot_recompute();
        }
    }
    pub fn clear_layout(&mut self) {
        let i = self.active_worksheet.min(self.worksheets.len().saturating_sub(1));
        crate::devlog!("clear_layout: sheet={} [{}]", i, self.debug_sheet_summary());
        if let Some(ws) = self.worksheets.get_mut(i) {
            ws.spec.rows.clear();
            ws.spec.columns.clear();
            ws.spec.column_subtotals.clear();
            ws.spec.values.clear();
            ws.spec.filters.clear();
            ws.model = None;
        }
        self.status = "Pivot layout cleared.".to_string();
    }

    pub fn distinct_values_for(&self, col_name: &str, limit: usize) -> Vec<CoreValue> {
        let Some(table) = &self.table else { return Vec::new() };
        if !self.is_column_selected(col_name) { return Vec::new(); }
        let Some(ci) = table.columns.iter().position(|c| c.name == col_name) else { return Vec::new() };
        let mut seen = std::collections::BTreeSet::<String>::new();
        let mut out = Vec::new();
        for row in &table.rows {
            let v = row.get(ci).cloned().unwrap_or(CoreValue::Null);
            let key = v.as_display();
            if seen.insert(key) { out.push(v); if out.len() >= limit { break; } }
        }
        out
    }

    pub fn add_filter(&mut self, field: String, op: FilterOp, value: CoreValue) {
        if field.is_empty() { return; }
        self.active_spec_mut().filters.push(Filter { field: FieldRef::new(field), op, values: vec![value] });
        self.status = format!("Filter added ({} active).", self.active_spec().filters.len());
        self.request_pivot_recompute();
    }
    pub fn remove_filter(&mut self, idx: usize) {
        if idx < self.active_spec().filters.len() { self.active_spec_mut().filters.remove(idx); self.request_pivot_recompute(); }
    }
    pub fn clear_filters(&mut self) {
        if !self.active_spec().filters.is_empty() {
            self.active_spec_mut().filters.clear();
            self.request_pivot_recompute();
            self.status = "Filters cleared.".to_string();
        }
    }

    fn ensure_explicit_footer_bands(&mut self) {
        let effective = self.active_spec().effective_footer_bands();
        let spec = self.active_spec_mut();
        if spec.footer_bands.is_empty() { spec.footer_bands = effective; spec.subtotals.grand_total = false; }
    }
    pub fn add_footer_band(&mut self, band: FooterBandSpec) { self.ensure_explicit_footer_bands(); self.active_spec_mut().footer_bands.push(band); self.request_pivot_recompute(); }
    pub fn remove_footer_band(&mut self, idx: usize) { self.ensure_explicit_footer_bands(); if idx < self.active_spec().footer_bands.len() { self.active_spec_mut().footer_bands.remove(idx); self.request_pivot_recompute(); } }
    pub fn move_footer_band(&mut self, from: usize, to: usize) {
        self.ensure_explicit_footer_bands();
        let len = self.active_spec().footer_bands.len();
        if from < len && to < len && from != to { let item = self.active_spec_mut().footer_bands.remove(from); self.active_spec_mut().footer_bands.insert(to, item); self.request_pivot_recompute(); }
    }
    pub fn set_title_pinned(&mut self, pinned: bool) {
        if pinned {
            let current = self
                .active_model()
                .map(|m| m.title.clone())
                .or(self.active_spec().title_override.clone())
                .unwrap_or_default();
            let prefix = self.active_sheet().table_title_prefix();
            let base = current
                .strip_prefix(&format!("{prefix} "))
                .or_else(|| current.strip_prefix(&prefix))
                .unwrap_or(&current)
                .trim()
                .to_string();
            self.active_spec_mut().title_override = if base.is_empty() { None } else { Some(base) };
        } else {
            self.active_spec_mut().title_override = None;
        }
        self.request_pivot_recompute();
    }
    pub fn set_footer_band_decimals(&mut self, idx: usize, decimals: Option<u8>) {
        self.ensure_explicit_footer_bands();
        if let Some(b) = self.active_spec_mut().footer_bands.get_mut(idx) {
            b.decimals = decimals;
            self.status = format!("Average decimals set to {}.", decimals.map(|d| d.to_string()).unwrap_or_else(|| "auto".to_string()));
            self.request_pivot_recompute();
        }
    }

    fn file_kind_name(kind: FileKind) -> &'static str {
        match kind { FileKind::Csv => "csv", FileKind::Tsv => "tsv", FileKind::Xlsx => "xlsx", FileKind::Dbf => "dbf" }
    }
    fn file_kind_from_name(name: Option<&str>) -> Option<FileKind> {
        match name.unwrap_or("").to_ascii_lowercase().as_str() {
            "csv" => Some(FileKind::Csv), "tsv" => Some(FileKind::Tsv), "xlsx" => Some(FileKind::Xlsx), "dbf" => Some(FileKind::Dbf), _ => None,
        }
    }

    fn workspace_source(&self) -> WorkspaceSource {
        if let Some(db) = &self.db_source {
            match db {
                DbSource::Sqlite { path, table } => WorkspaceSource::Sqlite { path: path.to_string_lossy().to_string(), table: table.clone() },
                DbSource::MySql(profile) => WorkspaceSource::MySql { profile: profile.sanitized_for_storage() },
            }
        } else if let Some(path) = &self.loaded_file_path {
            WorkspaceSource::File { path: path.to_string_lossy().to_string(), file_kind: self.loaded_file_kind.map(Self::file_kind_name).map(str::to_string), sheet: None }
        } else { WorkspaceSource::None }
    }

    fn build_workspace_file(&self) -> WorkspaceFile {
        let source = self.workspace_source();
        let selected_columns = self.selected_columns.as_ref().map(|s| s.iter().cloned().collect());
        // Preserve an explicitly modeled multi-source project; otherwise keep the
        // legacy bridge coherent with the current single-source UI state.
        let data_model = if self.data_model.datasets.len() > 1 || !self.data_model.relations.is_empty() {
            let mut dm = self.data_model.clone();
            if dm.base_dataset_id.is_none() && !dm.datasets.is_empty() { dm.base_dataset_id = Some(dm.datasets[0].id.clone()); }
            dm.sanitized()
        } else {
            pivot_data::ProjectDataModel::from_legacy(source.clone(), selected_columns.clone())
        };
        WorkspaceFile {
            manifest: pivot_data::WorkspaceManifest::default(),
            worksheets: self.worksheets.iter().map(|w| WorkspaceWorksheet { id: w.id.clone(), name: w.name.clone(), number: w.number, spec: w.spec.clone(), view: WorksheetView::default() }).collect(),
            active_worksheet: self.active_worksheet,
            view: WorkspaceView {
                brightness_id: Some(self.brightness.id().to_string()),
                table_theme_id: Some(self.table_color_theme.id().to_string()),
                border_theme_id: Some(self.table_border_theme.id().to_string()),
                on_home: self.on_home,
            },
            source,
            selected_columns,
            data_model,
        }
    }

    fn new_workspace(&mut self) {
        self.table = None;
        self.source_name = None;
        self.loaded_file_path = None;
        self.loaded_file_kind = None;
        self.db_source = None;
        self.pivot_computing = None;
        self.selected_columns = None;
        self.data_model = ProjectDataModel::default();
        self.data_model_diagnostics = ResolveDiagnostics::default();
        self.data_model_resolving = false;
        self.dataset_schema_cache.clear();
        self.dataset_preview_cache.clear();
        self.projection_message = None;
        self.schema_loading_dataset = None;
        self.rel_edit_id = None;
        self.rel_base_dataset.clear();
        self.rel_base_column.clear();
        self.rel_lookup_dataset.clear();
        self.rel_lookup_column.clear();
        self.rel_label_columns.clear();
        self.rel_name.clear();
        self.rel_message = None;
        self.worksheets = vec![WorksheetState::new("sheet-1".to_string(), WorksheetState::default_name(1), 1, default_spec())];
        self.active_worksheet = 0;
        self.next_sheet_no = 2;
        self.workspace_path = None;
        self.on_home = true;
        self.on_source_page = false;
        self.status = "New workspace.".to_string();
    }

    fn save_workspace(&mut self, save_as: bool) {
        let path = if !save_as { self.workspace_path.clone() } else { None };
        let path = match path {
            Some(p) => p,
            None => {
                let title = self.active_model().map(|m| m.title.as_str()).unwrap_or("workspace");
                let default = workspace::suggested_workspace_path(None, title);
                match rfd::FileDialog::new()
                    .set_title("Save Pivot Studio Workspace")
                    .add_filter("Pivot Workspace", &["pivot"])
                    .set_file_name(default.file_name().and_then(|n| n.to_str()).unwrap_or("workspace.pivot"))
                    .save_file()
                {
                    Some(mut p) => { if p.extension().and_then(|e| e.to_str()) != Some("pivot") { p.set_extension("pivot"); } p }
                    None => { self.status = "Save workspace cancelled.".to_string(); return; }
                }
            }
        };
        let ws = self.build_workspace_file();
        let sheets = ws.worksheets.len();
        let configured = ws.worksheets.iter().filter(|w| !w.spec.rows.is_empty() || !w.spec.columns.is_empty() || !w.spec.values.is_empty() || !w.spec.filters.is_empty() || w.spec.title_override.is_some() || w.spec.stub_head.is_some()).count();
        crate::devlog!("save_workspace: path={} sheets={} configured={} active={} summary=[{}] selected_columns={:?}",
            path.display(), sheets, configured, ws.active_worksheet, self.debug_sheet_summary(),
            ws.selected_columns.as_ref().map(|v| v.len()));
        match workspace::save_workspace(&path, &ws) {
            Ok(()) => { self.workspace_path = Some(path.clone()); self.status = format!("Workspace saved: {} ({} sheet(s), {} configured).", path.display(), sheets, configured); }
            Err(e) => self.status = format!("Save workspace failed: {e}"),
        }
    }

    fn open_workspace(&mut self) {
        let path = match rfd::FileDialog::new().set_title("Open Pivot Studio Workspace").add_filter("Pivot Workspace", &["pivot"]).pick_file() {
            Some(p) => p, None => { self.status = "Open workspace cancelled.".to_string(); return; }
        };
        crate::devlog!("open_workspace: path={} before=[{}]", path.display(), self.debug_sheet_summary());
        let ws = match workspace::load_workspace(&path) {
            Ok(w) => w, Err(e) => { crate::devlog!("open_workspace: load failed: {e}"); self.status = format!("Open workspace failed: {e}"); return; }
        };
        crate::devlog!("open_workspace: loaded worksheets={} active={} selected_columns={:?} source={:?}",
            ws.worksheets.len(), ws.active_worksheet, ws.selected_columns.as_ref().map(|v| v.len()),
            match &ws.source { WorkspaceSource::None=>"None", WorkspaceSource::File{..}=>"File", WorkspaceSource::Sqlite{..}=>"Sqlite", WorkspaceSource::MySql{..}=>"MySql" });
        if let Some(load) = &self.loading { load.cancel.store(true, Ordering::Relaxed); }
        self.loading = None;
        if let Some(state) = &self.pivot_computing { state.cancel.store(true, Ordering::Relaxed); }
        self.pivot_computing = None;
        self.workspace_path = Some(path.clone());
        self.worksheets = ws.worksheets.into_iter().enumerate().map(|(i, w)| WorksheetState::from_workspace(w, i + 1)).collect();
        if self.worksheets.is_empty() { self.worksheets.push(WorksheetState::new("sheet-1".to_string(), WorksheetState::default_name(1), 1, default_spec())); }
        self.active_worksheet = ws.active_worksheet.min(self.worksheets.len().saturating_sub(1));
        self.next_sheet_no = self.worksheets.iter().map(|w| w.number).max().unwrap_or(self.worksheets.len()) + 1;
        self.selected_columns = ws.selected_columns.map(|v| v.into_iter().collect());
        self.data_model = ws.data_model;
        self.dataset_schema_cache.clear();
        self.dataset_preview_cache.clear();
        self.projection_message = None;
        if let Some(id) = ws.view.brightness_id.as_deref() { if let Some(b) = Brightness::parse(id) { self.brightness = b; } }
        if ws.view.border_theme_id.is_some() || ws.view.table_theme_id.is_some() {
            let (cid, bid) = ws.view.resolved_theme_ids();
            if let Some(t) = TableColorThemeId::parse(&cid) { self.table_color_theme = t; }
            if let Some(b) = BorderThemeId::parse(&bid) { self.table_border_theme = b; }
        }
        self.on_home = false;
        self.on_source_page = false;
        self.table = None; self.source_name = None; self.loaded_file_path = None; self.loaded_file_kind = None;
        self.db_source = None;
        self.sync_layout_pickers();
        crate::devlog!("open_workspace: restored sheets=[{}] active={}", self.debug_sheet_summary(), self.active_worksheet);

        // Multi-source v3 workspace: resolve data model (datasets + relations) instead of legacy single-source reload.
        let multi = self.data_model.datasets.iter().filter(|d| d.enabled).count() > 1 || !self.data_model.relations.is_empty();
        if multi {
            self.on_source_page = false;
            self.status = format!("Workspace opened: {}. Resolving multi-source data model...", path.display());
            self.request_resolve_model();
            crate::devlog!("open_workspace: final (multi-source resolve queued) on_home={} on_source_page={} active={} sheets=[{}]",
                self.on_home, self.on_source_page, self.active_worksheet, self.debug_sheet_summary());
            return;
        }

        match ws.source {
            WorkspaceSource::None => {
                self.on_home = ws.view.on_home && self.worksheets.iter().all(|w| w.spec.rows.is_empty() && w.spec.columns.is_empty() && w.spec.values.is_empty() && w.spec.filters.is_empty());
                self.status = format!("Workspace opened: {} (no data source).", path.display());
            }
            WorkspaceSource::File { path: sp, file_kind, .. } => {
                let sp = PathBuf::from(sp);
                let kind = Self::file_kind_from_name(file_kind.as_deref()).unwrap_or_else(|| match sp.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
                    "tsv" => FileKind::Tsv, "xlsx"|"xls"|"xlsm" => FileKind::Xlsx, "dbf" => FileKind::Dbf, _ => FileKind::Csv,
                });
                if sp.exists() {
                    self.start_load_for_workspace(sp, kind);
                    self.status = format!("Workspace opened: {}. Reloading data source; worksheet setup preserved.", path.display());
                } else {
                    self.status = format!("Workspace opened, but data file not found: {}. Worksheet setup preserved; reconnect/import data to recompute.", sp.display());
                    self.on_source_page = true;
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
                            if self.selected_columns.is_none() { self.selected_columns = Some(t.columns.iter().map(|c| c.name.clone()).collect()); }
                            self.table = Some(t);
                            self.source_name = Some(format!("sqlite:{}:{}", sp.display(), table));
                            self.on_home = false;
                            self.status = format!("Workspace opened (SQLite {} / {}).", sp.display(), table);
                            self.sync_layout_pickers();
                        }
                        Err(e) => { crate::devlog!("open_workspace: sqlite preview failed: {e}"); self.status = format!("Workspace opened (SQLite), preview failed: {e}. Reconnect in Database panel."); }
                    }
                } else { crate::devlog!("open_workspace: sqlite file missing: {}", sp.display()); self.status = format!("Workspace opened, SQLite file not found: {}. Reconnect in Database panel.", sp.display()); }
            }
            WorkspaceSource::MySql { profile } => {
                self.db_source = Some(DbSource::MySql(profile.clone()));
                self.db_mysql_host = profile.host; self.db_mysql_port = profile.port.to_string(); self.db_mysql_db = profile.database;
                self.db_mysql_user = profile.user; self.db_mysql_table = profile.table; self.db_mysql_ssl = profile.ssl;
                self.db_mysql_cred_mode_session = matches!(profile.credential_mode, CredentialMode::SessionOnly);
                self.db_panel_open = false; self.on_home = false;
                crate::devlog!("open_workspace: mysql source restore disabled (setup preserved)");
                self.status = "Workspace has a MySQL source, but MySQL is temporarily disabled. Use CSV/XLSX/DBF or SQLite.".to_string();
            }
        }
        crate::devlog!("open_workspace: final on_home={} on_source_page={} has_table={} active={} sheets=[{}]",
            self.on_home, self.on_source_page, self.table.is_some(), self.active_worksheet, self.debug_sheet_summary());
        crate::devlog!("open_workspace: final on_home={} on_source_page={} has_table={} active={} sheets=[{}]",
            self.on_home, self.on_source_page, self.table.is_some(), self.active_worksheet, self.debug_sheet_summary());
    }

    pub fn export_pivot(&mut self, format: &str) {
        if self.exporting.is_some() { self.status = "An export is already in progress.".to_string(); return; }
        let style = ExportStyle { color_theme: self.table_color_theme, border_theme: self.table_border_theme, brightness: self.brightness, output_language: self.active_spec().output_language };
        let (ext, filter_name, filter_ext) = match format { "xlsx" => ("xlsx", "Excel Workbook", "xlsx"), "pdf" => ("pdf", "PDF Document", "pdf"), _ => ("html", "HTML", "html") };
        let sheets: Vec<(String, ThemedTableModel)> = self.worksheets.iter().filter_map(|w| w.model.clone().map(|m| (w.name.clone(), m))).collect();
        if format == "pdf" {
            let Some(model) = self.active_model().cloned() else { self.status = "Nothing to export: build the active pivot first (add a Values field).".to_string(); return; };
            let default_name = default_export_name(&model.title, ext);
            let Some(path) = rfd::FileDialog::new().set_title(format!("Export Pivot as {filter_name} (active sheet)")).add_filter(filter_name, &[filter_ext]).set_file_name(&default_name).save_file() else { self.status = "Export cancelled.".to_string(); return; };
            let (tx, rx) = mpsc::channel::<ExportMsg>();
            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_flag = Arc::clone(&cancel);
            self.status = format!("Exporting {ext} (active sheet) in background...");
            self.exporting = Some(ExportState { rx, progress: self.status.clone(), cancel });
            let title = model.title.clone();
            thread::spawn(move || {
                let _ = tx.send(ExportMsg::Progress(format!("Rendering {ext} export...")));
                if cancel_flag.load(Ordering::Relaxed) { let _ = tx.send(ExportMsg::Done(Err("Cancelled.".to_string()))); return; }
                match pivot_export::export_pdf(&path, &model, style) {
                    Ok(()) if cancel_flag.load(Ordering::Relaxed) => { let _ = tx.send(ExportMsg::Done(Err("Cancelled after write.".to_string()))); }
                    Ok(()) => { let _ = tx.send(ExportMsg::Done(Ok(format!("Exported {title} ({ext}) to {}.", path.display())))); }
                    Err(e) => { let _ = tx.send(ExportMsg::Done(Err(e.to_string()))); }
                }
            });
            return;
        }
        if sheets.is_empty() { self.status = "Nothing to export: build at least one worksheet pivot (add a Values field and recompute).".to_string(); return; }
        let default_title = self.active_model().map(|m| m.title.as_str()).unwrap_or("pivot");
        let default_name = default_export_name(default_title, ext);
        let Some(path) = rfd::FileDialog::new().set_title(format!("Export Pivot as {filter_name}")).add_filter(filter_name, &[filter_ext]).set_file_name(&default_name).save_file() else { self.status = "Export cancelled.".to_string(); return; };
        let (tx, rx) = mpsc::channel::<ExportMsg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        let sheet_count = sheets.len();
        self.status = format!("Exporting {sheet_count} sheet(s) to {ext} in background...");
        self.exporting = Some(ExportState { rx, progress: self.status.clone(), cancel });
        thread::spawn(move || {
            let _ = tx.send(ExportMsg::Progress(format!("Rendering {ext} export ({sheet_count} sheet(s))...")));
            if cancel_flag.load(Ordering::Relaxed) { let _ = tx.send(ExportMsg::Done(Err("Cancelled.".to_string()))); return; }
            let sheet_refs: Vec<(&str, &ThemedTableModel)> = sheets.iter().map(|(n, m)| (n.as_str(), m)).collect();
            let res = match ext {
                "xlsx" => pivot_export::export_xlsx_sheets(&path, &sheet_refs, style).map_err(|e| e.to_string()),
                _ => pivot_export::export_html_sheets(&path, &sheet_refs, style).map_err(|e| e.to_string()),
            };
            match res {
                Ok(()) if cancel_flag.load(Ordering::Relaxed) => { let _ = tx.send(ExportMsg::Done(Err("Cancelled after write.".to_string()))); }
                Ok(()) => { let _ = tx.send(ExportMsg::Done(Ok(format!("Exported {sheet_count} sheet(s) ({ext}) to {}.", path.display())))); }
                Err(e) => { let _ = tx.send(ExportMsg::Done(Err(e.to_string()))); }
            }
        });
    }

    pub fn dispatch(&mut self, id: &str, params: &Value, ctx: &egui::Context) {
        match id {
            "app.exit" | "window.close" => { self.exit_requested = true; }
            "window.minimize" => { ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)); }
            "window.maximize" => {
                let on = params.get("on").and_then(|v| v.as_bool()).unwrap_or_else(|| !ctx.input(|i| i.viewport().maximized.unwrap_or(false)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(on));
            }
            "nav.home" => { self.on_home = true; self.on_source_page = false; self.status = "Home - open a saved project or import data to start.".to_string(); }
            "nav.source" => { self.on_home = false; self.on_source_page = true; self.status = "Source Data - select columns to include in pivots.".to_string(); }
            "nav.worksheet" => {
                self.on_home = false; self.on_source_page = false;
                self.status = if let Some(name) = &self.source_name { format!("Worksheet '{}' - {name} ({} rows, {} cols).", self.active_sheet().name, self.row_count(), self.col_count()) }
                else { "Worksheet - import data to populate fields.".to_string() };
            }
            "file.import" | "file.import.csv" => {
                if self.import_adds_dataset() { self.on_home = false; self.on_source_page = true; self.pick_and_add_dataset(); }
                else { self.pick_and_load(FileKind::Csv) }
            }
            "file.import.xlsx" => {
                if self.import_adds_dataset() { self.on_home = false; self.on_source_page = true; self.pick_and_add_dataset(); }
                else { self.pick_and_load(FileKind::Xlsx) }
            }
            "file.import.dbf" => {
                if self.import_adds_dataset() { self.on_home = false; self.on_source_page = true; self.pick_and_add_dataset(); }
                else { self.pick_and_load(FileKind::Dbf) }
            }
            "file.new" => self.new_workspace(),
            "file.open" => self.open_workspace(),
            "file.save" => self.save_workspace(false),
            "file.save_as" => self.save_workspace(true),
            "data.connect" => { self.on_home = false; self.on_source_page = false; self.db_panel_open = true; self.db_tab_mysql = false; self.status = "Database: connect to SQLite (pushdown). CSV, XLSX, DBF, SQLite supported; MySQL temporarily disabled.".to_string(); }
            "data.resolve_model" => { self.on_home = false; self.on_source_page = true; self.request_resolve_model(); }
            "data.add_dataset" => { self.on_home = false; self.on_source_page = true; self.pick_and_add_dataset(); }
            "file.export" | "file.export.xlsx" | "file.export.pdf" | "file.export.html" => {
                let format = if id.ends_with(".xlsx") {
                    "xlsx".to_string()
                } else if id.ends_with(".pdf") {
                    "pdf".to_string()
                } else if id.ends_with(".html") {
                    "html".to_string()
                } else {
                    params.get("format").and_then(|v| v.as_str()).unwrap_or("html").to_string()
                };
                self.export_pivot(&format);
            }
            "data.cancel_load" => { self.cancel_load(); self.cancel_export(); self.cancel_pivot(); }
            "pivot.recompute" => self.request_pivot_recompute(),
            "pivot.clear" => self.clear_layout(),
            "pivot.add_sheet" => self.add_worksheet(),
            "view.brightness" => {
                let mode_str = params.get("mode")
                    .or_else(|| params.get("id"))
                    .and_then(|v| v.as_str());
                if let Some(mode) = mode_str {
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
                if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
                    if let Some(theme) = TableColorThemeId::parse(id) { self.table_color_theme = theme; self.status = format!("Color theme set to {}.", theme.label()); }
                    else if let Some(border) = BorderThemeId::parse(id) { self.table_border_theme = border; self.status = format!("Border theme set to {}.", border.label()); }
                    else { let (cid, bid) = parse_legacy_theme(id); self.table_color_theme = cid; self.table_border_theme = bid; self.status = format!("Theme set to {} / {}.", cid.label(), bid.label()); }
                }
            }
            "format.color_theme" => { if let Some(id) = params.get("id").and_then(|v| v.as_str()) { if let Some(theme) = TableColorThemeId::parse(id) { self.table_color_theme = theme; self.status = format!("Color theme set to {}.", theme.label()); } } }
            "format.border_theme" => { if let Some(id) = params.get("id").and_then(|v| v.as_str()) { if let Some(theme) = BorderThemeId::parse(id) { self.table_border_theme = theme; self.status = format!("Border theme set to {}.", theme.label()); } } }
            "view.reset_layout" => { self.status = "Layout reset (placeholder).".to_string(); }
            "help.about" => { self.status = "Pivot Studio v0.2.2 - polished pivot tables (CSV/XLSX/DBF/SQLite), multi-source data model with column relations (MVP), multi-worksheet projects, themes, export, .pivot workspace v3. Recompute between sheets as needed; PDF exports the active sheet.".to_string(); }
            _ => { self.status = format!("Command '{id}' is a placeholder."); }
        }
    }

    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::menu::bar(ui, |ui| {
            for (title, items) in menus::menu_tree() {
                ui.menu_button(title, |ui| { Self::render_menu_items(ui, &items, &ctx, self); });
            }
        });
    }

    fn render_menu_items(ui: &mut egui::Ui, items: &[menus::Item], ctx: &egui::Context, app: &mut PivotApp) {
        for item in items {
            match item {
                menus::Item::Sep => { ui.separator(); }
                menus::Item::Sub(label, children) => { ui.menu_button(label, |ui| { Self::render_menu_items(ui, children, ctx, app); }); }
                menus::Item::Cmd { label, id, params, shortcut } => {
                    let mut text = label.clone();
                    if let Some(sc) = shortcut { text = format!("{label}  ({sc})"); }
                    let checked = if id == "view.brightness" { params.get("mode").and_then(|v| v.as_str()).and_then(Brightness::parse).map(|b| b == app.brightness) }
                    else if id == "format.theme" || id == "format.color_theme" { params.get("id").and_then(|v| v.as_str()).and_then(TableColorThemeId::parse).map(|t| t == app.table_color_theme) }
                    else if id == "format.border_theme" { params.get("id").and_then(|v| v.as_str()).and_then(BorderThemeId::parse).map(|t| t == app.table_border_theme) }
                    else { None };
                    let response = if let Some(is_checked) = checked { ui.selectable_label(is_checked, text) } else { ui.button(text) };
                    if response.clicked() { app.dispatch(id, params, ctx); ui.close_menu(); }
                }
            }
        }
    }
}

fn default_export_name(title: &str, ext: &str) -> String {
    let base = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect::<String>();
    let base = base.trim();
    if base.is_empty() { format!("pivot.{ext}") } else { format!("{base}.{ext}") }
}

impl eframe::App for PivotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_theme(ctx);
        self.poll_loading(ctx);
        self.poll_preview(ctx);
        self.poll_pivot(ctx);
        self.poll_export(ctx);
        if self.loading.is_some() || self.preview_rx.is_some() || self.pivot_computing.is_some() || self.exporting.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.exit_requested { ctx.send_viewport_cmd(egui::ViewportCommand::Close); self.exit_requested = false; }

        let t = Tokens::get(ctx);
        egui::TopBottomPanel::top("app_bar").frame(egui::Frame::new().fill(t.app_bar)).show(ctx, |ui| { chrome::app_bar(self, ui); });
        egui::TopBottomPanel::top("stage_ribbon").frame(egui::Frame::new().fill(t.panel_alt)).show(ctx, |ui| { chrome::stage_ribbon(self, ui); });
        egui::TopBottomPanel::bottom("status_bar").frame(egui::Frame::new().fill(t.status_bar)).show(ctx, |ui| { chrome::status_bar(self, ui); });
        egui::SidePanel::left("activity_bar").exact_width(44.0).frame(egui::Frame::new().fill(t.app_bar)).show(ctx, |ui| { chrome::activity_bar(self, ui); });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.canvas)).show(ctx, |ui| { dock::show(self, ui); });
    }
}

