//! Data connectors (CSV/XLSX/DBF), type inference, and query helpers.
//! Phase 1: file-based ingest into `pivot_core::DataTable`.

pub mod csv_import;
pub mod dbf_import;
pub mod infer;
pub mod xlsx_import;

use pivot_core::DataTable;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("csv: {0}")]
    Csv(#[from] csv::Error),
    #[error("xlsx: {0}")]
    Xlsx(String),
    #[error("dbf: {0}")]
    Dbf(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileKind {
    #[default]
    Csv,
    Tsv,
    Xlsx,
    Dbf,
}

pub fn load_file(path: &Path, kind: FileKind) -> Result<DataTable> {
    match kind {
        FileKind::Csv => csv_import::load_csv(path, b',', true, None),
        FileKind::Tsv => csv_import::load_csv(path, b'\t', true, None),
        FileKind::Xlsx => xlsx_import::load_first_sheet(path),
        FileKind::Dbf => dbf_import::load_dbf(path),
    }
}
