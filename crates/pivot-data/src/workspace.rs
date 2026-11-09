use crate::{DataError, Result};
use pivot_core::PivotSpec;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const WORKSPACE_EXT: &str = "pivot";
pub const MANIFEST_NAME: &str = "manifest.json";
pub const SPEC_NAME: &str = "pivot_spec.json"; // v1 compat (active spec)
pub const VIEW_NAME: &str = "view.json"; // v1 compat / global view
pub const SOURCE_NAME: &str = "source.json"; // v1 compat / legacy primary source
pub const PROJECT_NAME: &str = "project.json"; // v2+
pub const WORKSHEETS_NAME: &str = "worksheets.json"; // v2+
pub const DATA_MODEL_NAME: &str = "data_model.json"; // v3+ (optional explicit entry; also embedded in project)
pub const FORMAT_NAME: &str = "pivot-studio-workspace";
pub const FORMAT_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub format: String,
    pub version: u32,
    pub app: String,
}

impl Default for WorkspaceManifest {
    fn default() -> Self {
        Self { format: FORMAT_NAME.to_string(), version: FORMAT_VERSION, app: "pivot-studio".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceView {
    #[serde(default)]
    pub brightness_id: Option<String>,
    /// Color theme preset id. For legacy workspaces, may contain a combined/old theme id.
    #[serde(default)]
    pub table_theme_id: Option<String>,
    /// Border theme preset id. Absent in legacy workspaces; resolved via compat on load.
    #[serde(default)]
    pub border_theme_id: Option<String>,
    #[serde(default)]
    pub on_home: bool,
}

impl WorkspaceView {
    /// Resolve color + border theme ids with backward compatibility for legacy `table_theme_id` values
    /// (including old combined `horizontalRules` theme and any missing `border_theme_id`).
    pub fn resolved_theme_ids(&self) -> (String, String) {
        if let (Some(c), Some(b)) = (&self.table_theme_id, &self.border_theme_id) {
            let legacy = c.as_str();
            if legacy.eq_ignore_ascii_case("horizontalRules") {
                return ("classicGrid".to_string(), "horizontalRules".to_string());
            }
            return (c.clone(), b.clone());
        }
        if let Some(legacy) = &self.table_theme_id {
            return Self::compat_from_legacy(legacy);
        }
        ("classicGrid".to_string(), "full".to_string())
    }

    fn compat_from_legacy(s: &str) -> (String, String) {
        if s.eq_ignore_ascii_case("horizontalRules") {
            ("classicGrid".to_string(), "horizontalRules".to_string())
        } else if s.eq_ignore_ascii_case("full") || s.eq_ignore_ascii_case("none") {
            ("classicGrid".to_string(), s.to_ascii_lowercase())
        } else {
            (s.to_string(), "full".to_string())
        }
    }
}

/// Legacy single-source descriptor (v1/v2 compat and app convenience).
/// v3 projects store one or more datasets in `ProjectDataModel`; this remains
/// as the primary/base source view for compatibility with the legacy app path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum WorkspaceSource {
    #[default]
    None,
    File {
        path: String,
        #[serde(default)]
        file_kind: Option<String>,
        /// XLSX sheet name (v3 dataset origin; absent for legacy/v2 file sources).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sheet: Option<String>,
    },
    Sqlite {
        path: String,
        table: String,
    },
    MySql {
        profile: crate::db_import::MySqlProfile,
    },
}

impl WorkspaceSource {
    /// Ensure no secrets are persisted/loaded as trusted secrets (MySQL profile sanitized).
    pub fn sanitized(self) -> Self {
        match self {
            WorkspaceSource::MySql { profile } => WorkspaceSource::MySql { profile: profile.sanitized_for_storage() },
            other => other,
        }
    }

    pub fn is_none(&self) -> bool { matches!(self, WorkspaceSource::None) }
}

/// Relation type. Addendum (m00581): relations are **simple lookup/enrichment**
/// relations — they must **not change the sum/row count of processed base data**.
/// They replace a code/id with meaningful labels from a related lookup table.
/// `Lookup` (and legacy `Left`) preserves every base row and rejects duplicate
/// lookup keys (no fanout). `Inner` is legacy/unsupported for this rule and is
/// resolved as an error (it would filter base rows).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    #[default]
    Lookup,
    /// Legacy alias for lookup-style base-preserving enrichment; treated as `Lookup`.
    Left,
    /// Legacy/unsupported: would filter base rows, violating the lookup rule.
    Inner,
}

impl JoinType {
    pub fn is_base_preserving(&self) -> bool {
        matches!(self, JoinType::Lookup | JoinType::Left)
    }
}

/// A named dataset/source participating in a project's data model (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dataset {
    /// Stable dataset identifier (persisted; used by relations and future field refs).
    pub id: String,
    /// User-facing alias used in qualified field display (e.g. `orders.order_id`).
    pub alias: String,
    #[serde(default)]
    pub source: WorkspaceSource,
    /// Optional materialization/row cap hint for MVP loaders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_limit: Option<u64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

impl Dataset {
    pub fn from_legacy_source(id: impl Into<String>, alias: impl Into<String>, source: WorkspaceSource) -> Self {
        Self { id: id.into(), alias: alias.into(), source: source.sanitized(), row_limit: None, enabled: true }
    }
}

/// A column relation/join between two datasets (v3). MVP: equi-join on key pairs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relation {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub left_dataset_id: String,
    pub left_column: String,
    pub right_dataset_id: String,
    pub right_column: String,
    #[serde(default)]
    pub join_type: JoinType,
    /// Lookup label columns to append from the lookup (right) dataset (addendum m00581).
    /// `None`/empty = append all non-key lookup columns (MVP default/backward compatible).
    /// Names are normalized dataset column names as seen in the dataset schema (unqualified);
    /// resolution matches qualified/unqualified forms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_columns: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Project-level multi-source data model (v3). This is the persisted model;
/// the legacy single-source fields on `WorkspaceFile`/`WorkspaceProject` are
/// compatibility projections derived from it.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProjectDataModel {
    #[serde(default)]
    pub datasets: Vec<Dataset>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    /// Base/primary dataset id for legacy single-source app behavior and default
    /// relation anchoring. When absent, the first enabled dataset is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dataset_id: Option<String>,
    /// Exposed columns feeding pivots for the current model. `None` means all
    /// resolved columns (v1/v2 compat / default). Names are canonical/normalized
    /// column identifiers in the resolved context.
    #[serde(default)]
    pub exposed_columns: Option<Vec<String>>,
}

impl ProjectDataModel {
    pub fn from_legacy(source: WorkspaceSource, selected_columns: Option<Vec<String>>) -> Self {
        let source = source.sanitized();
        if source.is_none() {
            return Self { datasets: Vec::new(), relations: Vec::new(), base_dataset_id: None, exposed_columns: selected_columns };
        }
        let id = "ds1".to_string();
        let alias = legacy_alias_for_source(&source);
        Self {
            datasets: vec![Dataset::from_legacy_source(id.clone(), alias, source)],
            relations: Vec::new(),
            base_dataset_id: Some(id),
            exposed_columns: selected_columns,
        }
    }

    pub fn sanitized(mut self) -> Self {
        for ds in &mut self.datasets { ds.source = ds.source.clone().sanitized(); }
        self
    }

    pub fn legacy_source(&self) -> WorkspaceSource {
        let id = self.base_dataset_id.as_deref();
        let ds = id.and_then(|id| self.datasets.iter().find(|d| d.id == id))
            .or_else(|| self.datasets.iter().find(|d| d.enabled))
            .or_else(|| self.datasets.first());
        ds.map(|d| d.source.clone().sanitized()).unwrap_or_default()
    }

    pub fn is_empty_single_legacy(&self) -> bool { self.datasets.is_empty() && self.relations.is_empty() }
}

fn legacy_alias_for_source(source: &WorkspaceSource) -> String {
    match source {
        WorkspaceSource::None => "source".to_string(),
        WorkspaceSource::File { path, .. } => Path::new(path).file_stem().and_then(|s| s.to_str()).map(sanitize_identifier_alias).filter(|s| !s.is_empty()).unwrap_or_else(|| "source".to_string()),
        WorkspaceSource::Sqlite { table, .. } => sanitize_identifier_alias(table),
        WorkspaceSource::MySql { profile } => sanitize_identifier_alias(&profile.table),
    }
}

fn sanitize_identifier_alias(s: &str) -> String {
    let mut out = String::new();
    for ch in s.trim().chars() {
        let c = if ch.is_ascii_alphanumeric() || ch == '_' { ch.to_ascii_lowercase() } else { '_' };
        out.push(c);
    }
    while out.contains("__") { out = out.replace("__", "_"); }
    let out = out.trim_matches('_').to_string();
    let out = if out.is_empty() { "source".to_string() } else { out };
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) { format!("t_{out}") } else { out }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorksheetView {
    // Reserved for per-worksheet UI state (scroll/layout). Global theme/nav lives in WorkspaceView.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceWorksheet {
    #[serde(default = "default_sheet_id")]
    pub id: String,
    #[serde(default = "default_sheet_name")]
    pub name: String,
    /// Stable worksheet sequence number used for default naming and table title prefixes.
    /// `0` means derive for compatibility.
    #[serde(default)]
    pub number: usize,
    #[serde(default)]
    pub spec: PivotSpec,
    #[serde(default)]
    pub view: WorksheetView,
}

fn default_sheet_id() -> String { "sheet-1".to_string() }
fn default_sheet_name() -> String { "Sheet 01".to_string() }
fn default_sheet_number() -> usize { 1 }

impl Default for WorkspaceWorksheet {
    fn default() -> Self {
        Self { id: default_sheet_id(), name: default_sheet_name(), number: default_sheet_number(), spec: PivotSpec::default(), view: WorksheetView::default() }
    }
}

/// v2/v3 project payload stored in `project.json`.
/// v3 shape stores `data_model`; legacy `source`/`selected_columns` are accepted
/// on read for migration and emitted as compatibility projections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceProject {
    /// v3 project data model (datasets + relations + projection). Present for v3;
    /// derived from legacy fields when loading v1/v2.
    #[serde(default)]
    pub data_model: Option<ProjectDataModel>,
    /// Legacy v1/v2 compatibility: primary source.
    #[serde(default)]
    pub source: WorkspaceSource,
    /// Legacy v1/v2 compatibility: `None` means all columns selected.
    #[serde(default)]
    pub selected_columns: Option<Vec<String>>,
    #[serde(default)]
    pub view: WorkspaceView,
    #[serde(default)]
    pub active_worksheet: usize,
}

impl WorkspaceProject {
    pub fn into_parts(self) -> (ProjectDataModel, WorkspaceView, usize) {
        let dm = self.data_model.unwrap_or_else(|| ProjectDataModel::from_legacy(self.source, self.selected_columns)).sanitized();
        (dm, self.view, self.active_worksheet)
    }
}

/// In-memory/open project (v3 shape with legacy convenience fields).
/// `load_workspace` migrates v1/v2 files into this shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    #[serde(default)]
    pub manifest: WorkspaceManifest,
    #[serde(default = "default_worksheets")]
    pub worksheets: Vec<WorkspaceWorksheet>,
    #[serde(default)]
    pub active_worksheet: usize,
    #[serde(default)]
    pub view: WorkspaceView,
    /// Legacy primary source convenience field (mirrors `data_model` base dataset).
    #[serde(default)]
    pub source: WorkspaceSource,
    /// Legacy selection convenience field. `None` means all columns selected.
    #[serde(default)]
    pub selected_columns: Option<Vec<String>>,
    /// v3 project data model.
    #[serde(default)]
    pub data_model: ProjectDataModel,
}

fn default_worksheets() -> Vec<WorkspaceWorksheet> { vec![WorkspaceWorksheet::default()] }

impl Default for WorkspaceFile {
    fn default() -> Self {
        Self {
            manifest: WorkspaceManifest::default(),
            worksheets: default_worksheets(),
            active_worksheet: 0,
            view: WorkspaceView::default(),
            source: WorkspaceSource::default(),
            selected_columns: None,
            data_model: ProjectDataModel::default(),
        }
    }
}

impl WorkspaceFile {
    pub fn from_v1(spec: PivotSpec, view: WorkspaceView, source: WorkspaceSource) -> Self {
        let source = source.sanitized();
        Self::from_legacy_source_parts(
            vec![WorkspaceWorksheet { id: "sheet-1".into(), name: "Sheet 01".into(), number: 1, spec, view: WorksheetView::default() }],
            0, view, source, None,
        )
    }

    pub fn from_legacy_source_parts(
        worksheets: Vec<WorkspaceWorksheet>, active_worksheet: usize, view: WorkspaceView,
        source: WorkspaceSource, selected_columns: Option<Vec<String>>,
    ) -> Self {
        let source = source.sanitized();
        let data_model = ProjectDataModel::from_legacy(source.clone(), selected_columns.clone());
        Self { manifest: WorkspaceManifest::default(), worksheets, active_worksheet, view, source, selected_columns, data_model }
    }

    pub fn active_spec(&self) -> &PivotSpec {
        self.worksheets.get(self.active_worksheet.min(self.worksheets.len().saturating_sub(1))).map(|w| &w.spec).unwrap_or_else(|| &self.worksheets[0].spec)
    }

    pub fn normalized(mut self) -> Self {
        if self.worksheets.is_empty() { self.worksheets = default_worksheets(); }
        if self.active_worksheet >= self.worksheets.len() { self.active_worksheet = 0; }
        // Keep legacy fields and data model coherent; prefer explicit data model when it has datasets.
        self.source = self.source.sanitized();
        if self.data_model.datasets.is_empty() && !self.source.is_none() {
            self.data_model = ProjectDataModel::from_legacy(self.source.clone(), self.selected_columns.clone());
        } else if !self.data_model.datasets.is_empty() {
            self.data_model = self.data_model.sanitized();
            self.source = self.data_model.legacy_source();
            if self.selected_columns.is_none() && self.data_model.exposed_columns.is_some() {
                self.selected_columns = self.data_model.exposed_columns.clone();
            }
        } else {
            self.data_model = self.data_model.sanitized();
        }
        self
    }
}

pub fn save_workspace(path: &Path, workspace: &WorkspaceFile) -> Result<()> {
    let ws = workspace.clone().normalized();
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut manifest = ws.manifest.clone();
    manifest.format = FORMAT_NAME.to_string();
    manifest.version = FORMAT_VERSION;
    if manifest.app.is_empty() { manifest.app = "pivot-studio".to_string(); }
    write_json_entry(&mut zip, opts, MANIFEST_NAME, &manifest)?;
    let project = WorkspaceProject {
        data_model: Some(ws.data_model.clone().sanitized()),
        source: ws.source.clone().sanitized(),
        selected_columns: ws.selected_columns.clone(),
        view: ws.view.clone(),
        active_worksheet: ws.active_worksheet,
    };
    write_json_entry(&mut zip, opts, PROJECT_NAME, &project)?;
    write_json_entry(&mut zip, opts, WORKSHEETS_NAME, &ws.worksheets)?;
    write_json_entry(&mut zip, opts, DATA_MODEL_NAME, &ws.data_model.clone().sanitized())?;
    // v1 compat entries (active sheet / legacy primary source) for older readers/tools.
    let active = ws.worksheets.get(ws.active_worksheet).cloned().unwrap_or_default();
    write_json_entry(&mut zip, opts, SPEC_NAME, &active.spec)?;
    write_json_entry(&mut zip, opts, VIEW_NAME, &ws.view)?;
    write_json_entry(&mut zip, opts, SOURCE_NAME, &ws.source.clone().sanitized())?;
    zip.finish().map_err(|e| DataError::Invalid(format!("failed to write .pivot zip: {e}")))?;
    Ok(())
}

fn write_json_entry<T: Serialize>(zip: &mut ZipWriter<File>, opts: SimpleFileOptions, name: &str, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| DataError::Invalid(format!("failed to serialize {name}: {e}")))?;
    zip.start_file(name, opts).map_err(|e| DataError::Invalid(format!("failed to add {name} to .pivot: {e}")))?;
    zip.write_all(&bytes)?;
    Ok(())
}

pub fn load_workspace(path: &Path) -> Result<WorkspaceFile> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(|e| DataError::Invalid(format!("not a valid .pivot ZIP file: {e}")))?;
    let manifest: WorkspaceManifest = read_json_entry(&mut archive, MANIFEST_NAME).unwrap_or_default();
    if manifest.format != FORMAT_NAME {
        return Err(DataError::Invalid(format!("unsupported workspace format '{}'; expected '{}'", manifest.format, FORMAT_NAME)));
    }
    if manifest.version > FORMAT_VERSION {
        return Err(DataError::Invalid(format!("workspace version {} is newer than supported version {}", manifest.version, FORMAT_VERSION)));
    }
    // v2+/v3+: project.json + worksheets.json
    if manifest.version >= 2 {
        if let (Ok(project), Ok(worksheets)) = (
            read_json_entry::<WorkspaceProject>(&mut archive, PROJECT_NAME),
            read_json_entry::<Vec<WorkspaceWorksheet>>(&mut archive, WORKSHEETS_NAME),
        ) {
            let (data_model, view, active_worksheet) = project.into_parts();
            return Ok(WorkspaceFile::from_legacy_source_parts(worksheets, active_worksheet, view, data_model.legacy_source(), data_model.exposed_columns.clone())
                .with_data_model(data_model)
                .with_manifest(manifest)
                .normalized());
        }
        // fall through to v1 entries if v2/v3 entries missing
    }
    // v1 migration (or fallback): pivot_spec.json + view/source
    let spec: PivotSpec = read_json_entry(&mut archive, SPEC_NAME)
        .map_err(|e| DataError::Invalid(format!("invalid .pivot workspace (pivot_spec.json): {e}")))?;
    let view: WorkspaceView = read_json_entry(&mut archive, VIEW_NAME).unwrap_or_default();
    let source: WorkspaceSource = read_json_entry(&mut archive, SOURCE_NAME).unwrap_or_default();
    Ok(WorkspaceFile::from_v1(spec, view, source.sanitized()).with_manifest(manifest))
}

impl WorkspaceFile {
    pub fn with_data_model(mut self, data_model: ProjectDataModel) -> Self {
        self.data_model = data_model.sanitized();
        self.source = self.data_model.legacy_source();
        if self.selected_columns.is_none() { self.selected_columns = self.data_model.exposed_columns.clone(); }
        self
    }
    pub fn with_manifest(mut self, manifest: WorkspaceManifest) -> Self { self.manifest = manifest; self }
}

fn read_json_entry<T: for<'de> Deserialize<'de>>(archive: &mut ZipArchive<File>, name: &str) -> std::result::Result<T, String> {
    let mut entry = archive.by_name(name).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

pub fn suggested_workspace_path(base: Option<&Path>, title: &str) -> PathBuf {
    let stem = sanitize_filename(if title.trim().is_empty() { "pivot-workspace" } else { title.trim() });
    let mut path = base
        .map(|p| if p.is_dir() { p.to_path_buf() } else { p.parent().unwrap_or(Path::new(".")).to_path_buf() })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    path.push(format!("{stem}.{WORKSPACE_EXT}"));
    path
}

pub fn sanitize_filename(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push(' ');
        } else {
            out.push('_');
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() { "pivot-workspace".to_string() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pivot_core::{Aggregate, FieldRef, FooterBandSpec, PivotSpec, ValueField};
    use tempfile::tempdir;

    fn sample_spec() -> PivotSpec {
        PivotSpec {
            values: vec![ValueField { field: FieldRef::new("Amount"), aggregate: Aggregate::Sum, display_name: None }],
            footer_bands: vec![FooterBandSpec::average()],
            title_override: Some("My Pivot".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn ac7_1_workspace_is_valid_zip_and_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.pivot");
        let ws = WorkspaceFile {
            worksheets: vec![WorkspaceWorksheet { id: "s1".into(), name: "Sheet 01".into(), number: 1, spec: sample_spec(), view: WorksheetView::default() }],
            active_worksheet: 0,
            view: WorkspaceView { brightness_id: Some("dark".into()), table_theme_id: Some("ledger".into()), border_theme_id: Some("horizontalRules".into()), on_home: false },
            source: WorkspaceSource::File { path: "C:/data/sales.csv".into(), file_kind: Some("csv".into()), sheet: None },
            selected_columns: None,
            data_model: ProjectDataModel::default(),
            ..Default::default()
        };
        save_workspace(&path, &ws).unwrap();
        let file = File::open(&path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap(); // valid ZIP
        assert!(archive.by_name(PROJECT_NAME).is_ok(), "v3 must include project.json");
        assert!(archive.by_name(WORKSHEETS_NAME).is_ok(), "v3 must include worksheets.json");
        assert!(archive.by_name(DATA_MODEL_NAME).is_ok(), "v3 must include data_model.json");
        assert!(archive.by_name(SPEC_NAME).is_ok(), "v3 should keep v1 compat pivot_spec.json");
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.manifest.format, FORMAT_NAME);
        assert_eq!(loaded.manifest.version, FORMAT_VERSION);
        assert_eq!(loaded.worksheets.len(), 1);
        assert_eq!(loaded.worksheets[0].spec.title_override.as_deref(), Some("My Pivot"));
        assert_eq!(loaded.view.table_theme_id.as_deref(), Some("ledger"));
        assert_eq!(loaded.view.border_theme_id.as_deref(), Some("horizontalRules"));
        let (c, b) = loaded.view.resolved_theme_ids();
        assert_eq!(c, "ledger");
        assert_eq!(b, "horizontalRules");
        assert!(loaded.selected_columns.is_none(), "missing selection defaults to all");
        match loaded.source {
            WorkspaceSource::File { file_kind, .. } => assert_eq!(file_kind.as_deref(), Some("csv")),
            _ => panic!("expected file source"),
        }
        // v1/v2/v3 compat projection: legacy single source becomes one dataset.
        assert_eq!(loaded.data_model.datasets.len(), 1, "legacy source migrates to one dataset");
        assert!(loaded.data_model.relations.is_empty());
    }

    #[test]
    fn ac7_compat_legacy_theme_ids() {
        let v = WorkspaceView { table_theme_id: Some("horizontalRules".to_string()), border_theme_id: None, ..WorkspaceView::default() };
        assert_eq!(v.resolved_theme_ids(), ("classicGrid".to_string(), "horizontalRules".to_string()));
        let v = WorkspaceView { table_theme_id: Some("ledger".to_string()), border_theme_id: None, ..WorkspaceView::default() };
        assert_eq!(v.resolved_theme_ids(), ("ledger".to_string(), "full".to_string()));
        let v = WorkspaceView { table_theme_id: Some("ledger".to_string()), border_theme_id: Some("none".to_string()), ..WorkspaceView::default() };
        assert_eq!(v.resolved_theme_ids(), ("ledger".to_string(), "none".to_string()));
    }

    #[test]
    fn ac7_2_corruption_reports_clear_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.pivot");
        std::fs::write(&path, b"not a zip").unwrap();
        let err = load_workspace(&path).unwrap_err().to_string();
        assert!(err.to_lowercase().contains("zip") || err.to_lowercase().contains("pivot") || err.to_lowercase().contains("valid"));
    }

    #[test]
    fn ac7_5_mysql_profile_is_sanitized() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db.pivot");
        let mut profile = crate::db_import::MySqlProfile {
            host: "127.0.0.1".into(), port: 3306, database: "app".into(), user: "u".into(), table: "sales".into(),
            ssl: false, credential_mode: crate::db_import::CredentialMode::SessionOnly, credential_ref: "pivot:mysql:ref".into(),
        };
        profile = profile.sanitized_for_storage();
        let ws = WorkspaceFile::from_legacy_source_parts(
            vec![WorkspaceWorksheet::default()], 0, WorkspaceView::default(),
            WorkspaceSource::MySql { profile }, None,
        );
        save_workspace(&path, &ws).unwrap();
        let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap_or_default();
        assert!(!text.contains("secret-password"));
        let loaded = load_workspace(&path).unwrap();
        match loaded.source {
            WorkspaceSource::MySql { profile } => assert_eq!(profile.host, "127.0.0.1"),
            _ => panic!("expected mysql source"),
        }
        assert_eq!(loaded.data_model.datasets.len(), 1);
        match &loaded.data_model.datasets[0].source {
            WorkspaceSource::MySql { profile } => assert_eq!(profile.host, "127.0.0.1"),
            _ => panic!("expected mysql dataset source"),
        }
    }

    #[test]
    fn v2_multi_worksheet_roundtrips_specs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("multi.pivot");
        let spec1 = sample_spec();
        let spec2 = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Count, display_name: None }],
            title_override: Some("Sheet 2 Pivot".to_string()),
            ..Default::default()
        };
        let ws = WorkspaceFile::from_legacy_source_parts(
            vec![
                WorkspaceWorksheet { id: "s1".into(), name: "Sheet 01".into(), number: 1, spec: spec1, view: WorksheetView::default() },
                WorkspaceWorksheet { id: "s2".into(), name: "Sheet 02".into(), number: 2, spec: spec2, view: WorksheetView::default() },
            ],
            1, WorkspaceView::default(), WorkspaceSource::None, None,
        );
        save_workspace(&path, &ws).unwrap();
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.worksheets.len(), 2);
        assert_eq!(loaded.active_worksheet, 1);
        assert_eq!(loaded.worksheets[0].spec.title_override.as_deref(), Some("My Pivot"));
        assert_eq!(loaded.worksheets[0].spec.values.len(), 1);
        assert_eq!(loaded.worksheets[1].spec.rows.len(), 1);
        assert_eq!(loaded.worksheets[1].spec.rows[0].name, "Region");
        assert_eq!(loaded.worksheets[1].spec.title_override.as_deref(), Some("Sheet 2 Pivot"));
        assert_eq!(loaded.worksheets[1].number, 2);
    }

    #[test]
    fn v1_v2_migration_projects_single_dataset_no_relations() {
        let dir = tempdir().unwrap();
        // Synthesize a v2-shaped project.json + worksheets.json payload in a zip.
        let path = dir.path().join("v2.pivot");
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = WorkspaceManifest { format: FORMAT_NAME.to_string(), version: 2, app: "pivot-studio".to_string() };
        let project = serde_json::json!({
            "source": {"kind": "file", "path": "C:/data/orders.xlsx", "file_kind": "xlsx"},
            "selected_columns": ["order_id", "customer_name"],
            "view": {"brightness_id": null, "table_theme_id": null, "border_theme_id": null, "on_home": false},
            "active_worksheet": 0
        });
        let worksheets = serde_json::json!([{ "id": "s1", "name": "Sheet 01", "number": 1, "spec": sample_spec(), "view": {} }]);
        for (name, value) in [(MANIFEST_NAME, serde_json::to_value(&manifest).unwrap()), (PROJECT_NAME, project), (WORKSHEETS_NAME, worksheets)] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(serde_json::to_vec_pretty(&value).unwrap().as_slice()).unwrap();
        }
        zip.finish().unwrap();
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.worksheets.len(), 1);
        assert_eq!(loaded.data_model.datasets.len(), 1);
        assert!(loaded.data_model.relations.is_empty());
        assert_eq!(loaded.data_model.exposed_columns.as_ref().map(|v| v.len()), Some(2));
        assert_eq!(loaded.selected_columns.as_ref().map(|v| v.len()), Some(2));
        match &loaded.data_model.datasets[0].source {
            WorkspaceSource::File { path, file_kind, .. } => { assert_eq!(path, "C:/data/orders.xlsx"); assert_eq!(file_kind.as_deref(), Some("xlsx")); }
            _ => panic!("expected file dataset"),
        }
    }

    #[test]
    fn v3_multi_source_roundtrips_and_sanitizes_mysql() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("model.pivot");
        let profile = crate::db_import::MySqlProfile {
            host: "db.example".into(), port: 3306, database: "app".into(), user: "u".into(), table: "customers".into(),
            ssl: true, credential_mode: crate::db_import::CredentialMode::SessionOnly, credential_ref: "pivot:mysql:ref".into(),
        };
        let data_model = ProjectDataModel {
            datasets: vec![
                Dataset::from_legacy_source("orders", "orders", WorkspaceSource::File { path: "C:/data/orders.csv".into(), file_kind: Some("csv".into()), sheet: None }),
                Dataset::from_legacy_source("customers", "customers", WorkspaceSource::MySql { profile }),
            ],
            relations: vec![Relation { id: "r1".into(), name: Some("orders->customers".into()), left_dataset_id: "orders".into(), left_column: "customer_id".into(), right_dataset_id: "customers".into(), right_column: "id".into(), join_type: JoinType::Left, label_columns: None, enabled: true }],
            base_dataset_id: Some("orders".into()),
            exposed_columns: Some(vec!["orders.order_id".into(), "customers.name".into()]),
        };
        let ws = WorkspaceFile { data_model: data_model.clone(), ..Default::default() };
        save_workspace(&path, &ws).unwrap();
        let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap_or_default();
        assert!(!text.contains("secret-password"));
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.data_model.datasets.len(), 2);
        assert_eq!(loaded.data_model.relations.len(), 1);
        assert_eq!(loaded.data_model.relations[0].join_type, JoinType::Left);
        assert_eq!(loaded.data_model.exposed_columns.as_ref().unwrap()[0], "orders.order_id");
        match &loaded.data_model.datasets[1].source {
            WorkspaceSource::MySql { profile } => assert_eq!(profile.database, "app"),
            _ => panic!("expected mysql dataset"),
        }
    }
}
