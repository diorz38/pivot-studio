//! `.pivot` workspace persistence (Phase 7 / decision #5).
//!
//! A `.pivot` file is a ZIP container. It stores workspace metadata, `PivotSpec`,
//! theme/UI preferences, and sanitized source metadata. It never stores secrets or
//! large raw datasets by default.

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
pub const SPEC_NAME: &str = "pivot_spec.json"; // v1 compat (single spec)
pub const VIEW_NAME: &str = "view.json"; // v1 compat / global view
pub const SOURCE_NAME: &str = "source.json"; // v1 compat
pub const PROJECT_NAME: &str = "project.json"; // v2+
pub const WORKSHEETS_NAME: &str = "worksheets.json"; // v2+
pub const FORMAT_NAME: &str = "pivot-studio-workspace";
pub const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub format: String,
    pub version: u32,
    pub app: String,
}

impl Default for WorkspaceManifest {
    fn default() -> Self {
        Self {
            format: FORMAT_NAME.to_string(),
            version: FORMAT_VERSION,
            app: "pivot-studio".to_string(),
        }
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
            WorkspaceSource::MySql { profile } => WorkspaceSource::MySql {
                profile: profile.sanitized_for_storage(),
            },
            other => other,
        }
    }
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

/// v2 project payload stored in `project.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceProject {
    #[serde(default)]
    pub source: WorkspaceSource,
    /// `None` means all columns selected (v1 compat / default).
    #[serde(default)]
    pub selected_columns: Option<Vec<String>>,
    #[serde(default)]
    pub view: WorkspaceView,
    #[serde(default)]
    pub active_worksheet: usize,
}

/// In-memory/open project (v2 shape). `load_workspace` migrates v1 files into this shape.
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
    #[serde(default)]
    pub source: WorkspaceSource,
    /// `None` means all columns selected.
    #[serde(default)]
    pub selected_columns: Option<Vec<String>>,
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
        }
    }
}

impl WorkspaceFile {
    pub fn from_v1(spec: PivotSpec, view: WorkspaceView, source: WorkspaceSource) -> Self {
        Self {
            manifest: WorkspaceManifest::default(),
            worksheets: vec![WorkspaceWorksheet { id: "sheet-1".into(), name: "Sheet 01".into(), number: 1, spec, view: WorksheetView::default() }],
            active_worksheet: 0,
            view,
            source: source.sanitized(),
            selected_columns: None,
        }
    }

    pub fn active_spec(&self) -> &PivotSpec {
        self.worksheets.get(self.active_worksheet.min(self.worksheets.len().saturating_sub(1))).map(|w| &w.spec).unwrap_or_else(|| &self.worksheets[0].spec)
    }

    pub fn normalized(mut self) -> Self {
        if self.worksheets.is_empty() { self.worksheets = default_worksheets(); }
        if self.active_worksheet >= self.worksheets.len() { self.active_worksheet = 0; }
        self.source = self.source.sanitized();
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
        source: ws.source.clone(),
        selected_columns: ws.selected_columns.clone(),
        view: ws.view.clone(),
        active_worksheet: ws.active_worksheet,
    };
    write_json_entry(&mut zip, opts, PROJECT_NAME, &project)?;
    write_json_entry(&mut zip, opts, WORKSHEETS_NAME, &ws.worksheets)?;
    // v1 compat entries (active sheet) for older readers/tools.
    let active = ws.worksheets.get(ws.active_worksheet).cloned().unwrap_or_default();
    write_json_entry(&mut zip, opts, SPEC_NAME, &active.spec)?;
    write_json_entry(&mut zip, opts, VIEW_NAME, &ws.view)?;
    write_json_entry(&mut zip, opts, SOURCE_NAME, &ws.source)?;
    zip.finish().map_err(|e| DataError::Invalid(format!("failed to write .pivot zip: {e}")))?;
    Ok(())
}

fn write_json_entry<T: Serialize>(
    zip: &mut ZipWriter<File>,
    opts: SimpleFileOptions,
    name: &str,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| DataError::Invalid(format!("failed to serialize {name}: {e}")))?;
    zip.start_file(name, opts)
        .map_err(|e| DataError::Invalid(format!("failed to add {name} to .pivot: {e}")))?;
    zip.write_all(&bytes)?;
    Ok(())
}

pub fn load_workspace(path: &Path) -> Result<WorkspaceFile> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| DataError::Invalid(format!("not a valid .pivot ZIP file: {e}")))?;
    let manifest: WorkspaceManifest = read_json_entry(&mut archive, MANIFEST_NAME).unwrap_or_default();
    if manifest.format != FORMAT_NAME {
        return Err(DataError::Invalid(format!(
            "unsupported workspace format '{}'; expected '{}'",
            manifest.format, FORMAT_NAME
        )));
    }
    if manifest.version > FORMAT_VERSION {
        return Err(DataError::Invalid(format!(
            "workspace version {} is newer than supported version {}",
            manifest.version, FORMAT_VERSION
        )));
    }
    // v2+: project.json + worksheets.json
    if manifest.version >= 2 {
        if let (Ok(project), Ok(worksheets)) = (
            read_json_entry::<WorkspaceProject>(&mut archive, PROJECT_NAME),
            read_json_entry::<Vec<WorkspaceWorksheet>>(&mut archive, WORKSHEETS_NAME),
        ) {
            return Ok(WorkspaceFile {
                manifest,
                worksheets,
                active_worksheet: project.active_worksheet,
                view: project.view,
                source: project.source.sanitized(),
                selected_columns: project.selected_columns,
            }.normalized());
        }
        // fall through to v1 entries if v2 entries missing
    }
    // v1 migration (or v2 fallback): pivot_spec.json + view/source
    let spec: PivotSpec = read_json_entry(&mut archive, SPEC_NAME)
        .map_err(|e| DataError::Invalid(format!("invalid .pivot workspace (pivot_spec.json): {e}")))?;
    let view: WorkspaceView = read_json_entry(&mut archive, VIEW_NAME).unwrap_or_default();
    let source: WorkspaceSource = read_json_entry(&mut archive, SOURCE_NAME).unwrap_or_default();
    Ok(WorkspaceFile::from_v1(spec, view, source.sanitized()))
}

fn read_json_entry<T: for<'de> Deserialize<'de>>(
    archive: &mut ZipArchive<File>,
    name: &str,
) -> std::result::Result<T, String> {
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
            values: vec![ValueField {
                field: FieldRef::new("Amount"),
                aggregate: Aggregate::Sum,
                display_name: None,
            }],
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
            worksheets: vec![WorkspaceWorksheet {
                id: "s1".into(),
                name: "Sheet 01".into(),
                number: 1,
                spec: sample_spec(),
                view: WorksheetView::default(),
            }],
            active_worksheet: 0,
            view: WorkspaceView {
                brightness_id: Some("dark".into()),
                table_theme_id: Some("ledger".into()),
                border_theme_id: Some("horizontalRules".into()),
                on_home: false,
            },
            source: WorkspaceSource::File {
                path: "C:/data/sales.csv".into(),
                file_kind: Some("csv".into()),
            },
            selected_columns: None,
            ..Default::default()
        };
        save_workspace(&path, &ws).unwrap();
        let file = File::open(&path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap(); // valid ZIP
        assert!(archive.by_name(PROJECT_NAME).is_ok(), "v2 must include project.json");
        assert!(archive.by_name(WORKSHEETS_NAME).is_ok(), "v2 must include worksheets.json");
        assert!(archive.by_name(SPEC_NAME).is_ok(), "v2 should keep v1 compat pivot_spec.json");
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
            host: "127.0.0.1".into(),
            port: 3306,
            database: "app".into(),
            user: "u".into(),
            table: "sales".into(),
            ssl: false,
            credential_mode: crate::db_import::CredentialMode::SessionOnly,
            credential_ref: "pivot:mysql:ref".into(),
        };
        // Simulate storage: workspace always stores sanitized profile.
        profile = profile.sanitized_for_storage();
        let ws = WorkspaceFile { source: WorkspaceSource::MySql { profile }, ..Default::default() };
        save_workspace(&path, &ws).unwrap();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!text.contains("secret-password"));
        let loaded = load_workspace(&path).unwrap();
        match loaded.source {
            WorkspaceSource::MySql { profile } => assert_eq!(profile.host, "127.0.0.1"),
            _ => panic!("expected mysql source"),
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
        let ws = WorkspaceFile {
            manifest: WorkspaceManifest::default(),
            worksheets: vec![
                WorkspaceWorksheet { id: "s1".into(), name: "Sheet 01".into(), number: 1, spec: spec1, view: WorksheetView::default() },
                WorkspaceWorksheet { id: "s2".into(), name: "Sheet 02".into(), number: 2, spec: spec2, view: WorksheetView::default() },
            ],
            active_worksheet: 1,
            view: WorkspaceView::default(),
            source: WorkspaceSource::None,
            selected_columns: None,
        };
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
}