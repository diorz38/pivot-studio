use crate::workspace::{Dataset, WorkspaceSource};
use crate::{db_import, load_file, DataError, FileKind, Result};
use pivot_core::{Column, DataTable};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Default)]
pub struct DatasetLoadOptions {
    /// Materialization row cap (MVP safety). `None` = load all rows.
    pub row_limit: Option<usize>,
    /// Number of preview rows to keep when `preview_only` is true.
    pub preview_rows: usize,
    pub preview_only: bool,
}

#[derive(Debug, Clone)]
pub struct LoadedDataset {
    pub dataset_id: String,
    pub alias: String,
    pub table: DataTable,
    pub row_count: usize,
    pub capped: bool,
    pub warnings: Vec<String>,
}

/// Preview-only dataset schema returned by `preview_dataset_schema`.
/// Columns are the normalized dataset schema (pre-qualification), with inferred
/// types/nullability when available. Rows are optional sampled rows (capped).
#[derive(Debug, Clone)]
pub struct DatasetPreview {
    pub dataset_id: String,
    pub alias: String,
    pub columns: Vec<Column>,
    pub preview_rows: Vec<Vec<pivot_core::Value>>,
    pub row_count: usize,
    pub capped: bool,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

pub fn load_dataset_table(dataset: &Dataset, opts: &DatasetLoadOptions, cancel: Option<&AtomicBool>) -> Result<LoadedDataset> {
    if cancel.map(|c| c.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    if !dataset.enabled {
        return Err(DataError::Invalid(format!("dataset '{}' is disabled", dataset.alias)));
    }
    let mut warnings = Vec::new();
    let (table, capped_hint, _truncated) = match &dataset.source {
        WorkspaceSource::None => return Err(DataError::Invalid(format!("dataset '{}' has no source", dataset.alias))),
        WorkspaceSource::File { path, file_kind, sheet } => {
            let path = PathBuf::from(path);
            if !path.exists() {
                return Err(DataError::Invalid(format!("dataset '{}' source file not found: {}", dataset.alias, path.display())));
            }
            let kind = file_kind_from_name(file_kind.as_deref()).unwrap_or_else(|| kind_from_ext(&path));
            let mut t = load_file_with_sheet(&path, kind, sheet.as_deref())?;
            let limit = effective_limit(dataset, opts);
            let capped = apply_row_limit(&mut t, limit);
            (t, capped, false)
        }
        WorkspaceSource::Sqlite { path, table } => {
            let path = PathBuf::from(path);
            if !path.exists() {
                return Err(DataError::Invalid(format!("dataset '{}' SQLite file not found: {}", dataset.alias, path.display())));
            }
            let limit = effective_limit(dataset, opts);
            let t = db_import::load_sqlite_table(&path, table, limit, cancel)?;
            let capped = limit.is_some_and(|n| t.rows.len() >= n);
            (t, capped, capped)
        }
        WorkspaceSource::MySql { profile } => {
            // MVP: MySQL restore/preview path is constrained in-app; provide an attributable error.
            return Err(DataError::Unsupported(format!(
                "dataset '{}' uses MySQL ({} / {} / {}) which is not available in the MVP multi-source loader; use CSV/XLSX/DBF or SQLite",
                dataset.alias, profile.host, profile.database, profile.table
            )));
        }
    };
    let row_count = table.rows.len();
    if capped_hint { warnings.push(format!("dataset '{}' was capped at {} rows", dataset.alias, row_count)); }
    if table.columns.is_empty() { warnings.push(format!("dataset '{}' has no columns", dataset.alias)); }
    Ok(LoadedDataset { dataset_id: dataset.id.clone(), alias: dataset.alias.clone(), table, row_count, capped: capped_hint, warnings })
}

/// Preview a dataset's schema (and a capped sample of rows) without full materialization semantics.
/// `opts.preview_rows` limits sampled rows (0 = schema only); `opts.preview_only` forces schema path.
/// Current file loaders are in-memory in MVP, so preview loads and truncates to `preview_rows`.
pub fn preview_dataset_schema(dataset: &Dataset, opts: &DatasetLoadOptions, cancel: Option<&AtomicBool>) -> Result<DatasetPreview> {
    if cancel.map(|c| c.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    if !dataset.enabled {
        return Err(DataError::Invalid(format!("dataset '{}' is disabled", dataset.alias)));
    }
    let sample = opts.preview_rows.max(1);
    let limited = DatasetLoadOptions { row_limit: Some(sample), preview_rows: sample, preview_only: true };
    let loaded = load_dataset_table(dataset, &limited, cancel)?;
    let mut columns = loaded.table.columns.clone();
    // Schema-only preview still needs a DB path when table is huge: `load_sqlite_table` already caps via LIMIT.
    // Preserve original/normalized metadata for UI display.
    for col in &mut columns {
        if col.original_name.is_none() && col.display_name.is_none() { /* keep as-is; registries already set original_name for file datasets */ }
    }
    let truncated = loaded.capped;
    let mut preview_rows = loaded.table.rows.clone();
    if preview_rows.len() > sample { preview_rows.truncate(sample); }
    Ok(DatasetPreview {
        dataset_id: dataset.id.clone(),
        alias: dataset.alias.clone(),
        columns,
        preview_rows,
        row_count: loaded.row_count,
        capped: loaded.capped,
        truncated,
        warnings: loaded.warnings,
    })
}

pub fn preview_dataset_columns(dataset: &Dataset, cancel: Option<&AtomicBool>) -> Result<Vec<Column>> {
    let preview = preview_dataset_schema(dataset, &DatasetLoadOptions { row_limit: Some(1), preview_rows: 0, preview_only: true }, cancel)?;
    Ok(preview.columns)
}

fn effective_limit(dataset: &Dataset, opts: &DatasetLoadOptions) -> Option<usize> {
    let ds = dataset.row_limit.map(|n| n as usize).filter(|n| *n > 0);
    let opt = opts.row_limit.filter(|n| *n > 0);
    match (ds, opt) { (Some(a), Some(b)) => Some(a.min(b)), (Some(a), None) => Some(a), (None, Some(b)) => Some(b), (None, None) => None }
}

fn apply_row_limit(table: &mut DataTable, limit: Option<usize>) -> bool {
    if let Some(n) = limit {
        if table.rows.len() > n { table.rows.truncate(n); return true; }
    }
    false
}

fn load_file_with_sheet(path: &Path, kind: FileKind, sheet: Option<&str>) -> Result<DataTable> {
    match (kind, sheet) {
        (FileKind::Xlsx, Some(name)) if !name.trim().is_empty() => crate::xlsx_import::load_sheet_by_name(path, name),
        _ => load_file(path, kind),
    }
}

fn file_kind_from_name(name: Option<&str>) -> Option<FileKind> {
    match name.unwrap_or("").to_ascii_lowercase().as_str() {
        "csv" => Some(FileKind::Csv),
        "tsv" => Some(FileKind::Tsv),
        "xlsx" | "xls" | "xlsm" => Some(FileKind::Xlsx),
        "dbf" => Some(FileKind::Dbf),
        _ => None,
    }
}

fn kind_from_ext(path: &Path) -> FileKind {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "tsv" => FileKind::Tsv,
        "xlsx" | "xls" | "xlsm" => FileKind::Xlsx,
        "dbf" => FileKind::Dbf,
        _ => FileKind::Csv,
    }
}

/// Expose columns as `alias.column` in the resolved dataset to avoid
/// cross-dataset name collisions. Keep `original_name`/`display_name`
/// metadata; the canonical materialized name becomes qualified.
pub fn qualify_column_names(table: &mut DataTable, alias: &str, _warnings: &mut Vec<String>) {
    let alias = alias.trim();
    if alias.is_empty() { return; }
    for col in &mut table.columns {
        let base = col.name.clone();
        col.name = format!("{alias}.{base}");
        if col.original_name.is_none() { col.original_name = Some(base); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Dataset;
    use std::fs;

    #[test]
    fn loads_csv_dataset_with_normalized_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orders.csv");
        fs::write(&path, "Order ID,Customer Name\n1,Alice\n2,Bob\n").unwrap();
        let ds = Dataset::from_legacy_source("ds1", "orders", WorkspaceSource::File { path: path.to_string_lossy().to_string(), file_kind: Some("csv".into()), sheet: None });
        let mut loaded = load_dataset_table(&ds, &DatasetLoadOptions::default(), None).unwrap();
        assert_eq!(loaded.row_count, 2);
        assert_eq!(loaded.table.columns[0].name, "order_id");
        assert_eq!(loaded.table.columns[1].name, "customer_name");
        assert_eq!(loaded.table.columns[0].original_name.as_deref(), Some("Order ID"));
        qualify_column_names(&mut loaded.table, "orders", &mut Vec::new());
        assert_eq!(loaded.table.columns[0].name, "orders.order_id");
    }

    #[test]
    fn loads_xlsx_dataset_sheet_and_preserves_original_header_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("Orders").unwrap();
        sheet.write_string(0, 0, "Order ID").unwrap();
        sheet.write_number(1, 0, 10.0).unwrap();
        workbook.save(&path).unwrap();
        let ds = Dataset::from_legacy_source("ds1", "orders", WorkspaceSource::File { path: path.to_string_lossy().to_string(), file_kind: Some("xlsx".into()), sheet: Some("Orders".into()) });
        let mut loaded = load_dataset_table(&ds, &DatasetLoadOptions::default(), None).unwrap();
        assert_eq!(loaded.table.columns[0].name, "order_id");
        qualify_column_names(&mut loaded.table, "orders", &mut Vec::new());
        assert_eq!(loaded.table.columns[0].name, "orders.order_id");
        assert!(loaded.table.rows.len() == 1);
    }

    #[test]
    fn loads_sqlite_table_dataset_and_caps_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE orders(id INTEGER, name TEXT); INSERT INTO orders VALUES(1,'A'); INSERT INTO orders VALUES(2,'B'); INSERT INTO orders VALUES(3,'C');").unwrap();
        drop(conn);
        let ds = Dataset::from_legacy_source("ds1", "orders", WorkspaceSource::Sqlite { path: path.to_string_lossy().to_string(), table: "orders".into() });
        let mut loaded = load_dataset_table(&ds, &DatasetLoadOptions { row_limit: Some(2), preview_rows: 0, preview_only: false }, None).unwrap();
        assert_eq!(loaded.row_count, 2);
        assert!(loaded.capped);
        assert_eq!(loaded.table.columns[0].name, "id");
        qualify_column_names(&mut loaded.table, "orders", &mut Vec::new());
        assert_eq!(loaded.table.columns[0].name, "orders.id");
    }

    #[test]
    fn missing_file_has_attributable_error() {
        let ds = Dataset::from_legacy_source("ds1", "orders", WorkspaceSource::File { path: "C:/no/such/file.csv".into(), file_kind: Some("csv".into()), sheet: None });
        let err = load_dataset_table(&ds, &DatasetLoadOptions::default(), None).unwrap_err().to_string();
        assert!(err.contains("orders"));
        assert!(err.to_lowercase().contains("not found") || err.to_lowercase().contains("no such"));
    }

    #[test]
    fn preview_dataset_schema_returns_columns_and_capped_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orders.csv");
        fs::write(&path, "Order ID,Customer Name,Amount\n1,Alice,10\n2,Bob,20\n3,Cara,30\n").unwrap();
        let ds = Dataset::from_legacy_source("ds1", "orders", WorkspaceSource::File { path: path.to_string_lossy().to_string(), file_kind: Some("csv".into()), sheet: None });
        let preview = preview_dataset_schema(&ds, &DatasetLoadOptions { row_limit: Some(2), preview_rows: 2, preview_only: true }, None).unwrap();
        assert_eq!(preview.columns.len(), 3);
        assert_eq!(preview.columns[0].name, "order_id");
        assert_eq!(preview.columns[0].original_name.as_deref(), Some("Order ID"));
        assert_eq!(preview.row_count, 2);
        assert!(preview.capped);
        assert!(preview.truncated);
        assert_eq!(preview.preview_rows.len(), 2);
        let cols = preview_dataset_columns(&ds, None).unwrap();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].effective_type(), pivot_core::DataType::Integer);
    }
}
