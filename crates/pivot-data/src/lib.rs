pub mod csv_import;
pub mod db_import;
pub mod dbf_import;
pub mod infer;
pub mod normalize;
pub mod registry;
pub mod resolve;
pub mod workspace;
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
    #[error("database: {0}")]
    Database(String),
    #[error("credentials: {0}")]
    Credentials(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("cancelled")]
    Cancelled,
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

pub use db_import::{connect_and_preview_sqlite, execute_pivot_mysql, execute_pivot_sqlite, list_sqlite_tables, load_sqlite_table, preview_mysql, store_secret, CredentialMode, DbProfile, DbTableRef, MySqlProfile, SqliteProfile};
pub use xlsx_import::list_sheets;
pub use registry::{load_dataset_table, preview_dataset_columns, preview_dataset_schema, qualify_column_names, DatasetLoadOptions, DatasetPreview, LoadedDataset};
pub use resolve::{resolve_model, ResolveDiagnostics, ResolvedModel};
pub use workspace::{
    load_workspace, save_workspace, sanitize_filename, suggested_workspace_path, Dataset,
    JoinType, ProjectDataModel, Relation, WorksheetView, WorkspaceFile, WorkspaceManifest,
    WorkspaceProject, WorkspaceSource, WorkspaceView, WorkspaceWorksheet, DATA_MODEL_NAME,
    FORMAT_VERSION, PROJECT_NAME, WORKSHEETS_NAME, WORKSPACE_EXT,
};
