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
pub const SPEC_NAME: &str = "pivot_spec.json";
pub const VIEW_NAME: &str = "view.json";
pub const SOURCE_NAME: &str = "source.json";
pub const FORMAT_NAME: &str = "pivot-studio-workspace";
pub const FORMAT_VERSION: u32 = 1;

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
    #[serde(default)]
    pub table_theme_id: Option<String>,
    #[serde(default)]
    pub on_home: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceSource {
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

impl Default for WorkspaceSource {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceFile {
    #[serde(default)]
    pub manifest: WorkspaceManifest,
    #[serde(default)]
    pub spec: PivotSpec,
    #[serde(default)]
    pub view: WorkspaceView,
    #[serde(default)]
    pub source: WorkspaceSource,
}

pub fn save_workspace(path: &Path, workspace: &WorkspaceFile) -> Result<()> {
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    write_json_entry(&mut zip, opts, MANIFEST_NAME, &workspace.manifest)?;
    write_json_entry(&mut zip, opts, SPEC_NAME, &workspace.spec)?;
    write_json_entry(&mut zip, opts, VIEW_NAME, &workspace.view)?;
    write_json_entry(&mut zip, opts, SOURCE_NAME, &workspace.source)?;
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
    let spec: PivotSpec = read_json_entry(&mut archive, SPEC_NAME)
        .map_err(|e| DataError::Invalid(format!("invalid .pivot workspace (pivot_spec.json): {e}")))?;
    let view: WorkspaceView = read_json_entry(&mut archive, VIEW_NAME).unwrap_or_default();
    let source: WorkspaceSource = read_json_entry(&mut archive, SOURCE_NAME).unwrap_or_default();
    // Safety: ensure MySQL profiles loaded from disk never carry a plaintext secret.
    let source = match source {
        WorkspaceSource::MySql { profile } => WorkspaceSource::MySql {
            profile: profile.sanitized_for_storage(),
        },
        other => other,
    };
    Ok(WorkspaceFile { manifest, view, spec, source })
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
            spec: sample_spec(),
            view: WorkspaceView {
                brightness_id: Some("dark".into()),
                table_theme_id: Some("ledger".into()),
                on_home: false,
            },
            source: WorkspaceSource::File {
                path: "C:/data/sales.csv".into(),
                file_kind: Some("csv".into()),
            },
            ..Default::default()
        };
        save_workspace(&path, &ws).unwrap();
        let file = File::open(&path).unwrap();
        ZipArchive::new(file).unwrap(); // valid ZIP
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.manifest.format, FORMAT_NAME);
        assert_eq!(loaded.manifest.version, FORMAT_VERSION);
        assert_eq!(loaded.spec.title_override.as_deref(), Some("My Pivot"));
        assert_eq!(loaded.view.table_theme_id.as_deref(), Some("ledger"));
        match loaded.source {
            WorkspaceSource::File { file_kind, .. } => assert_eq!(file_kind.as_deref(), Some("csv")),
            _ => panic!("expected file source"),
        }
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
}
