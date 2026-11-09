//! Integration parity check: multi-source lookup resolution -> pivot compute -> XLSX export.
//!
//! Addendum m00581: lookup relations must preserve the base row count.

use pivot_core::{Aggregate, FieldRef, PivotSpec, ValueField};
use pivot_data::workspace::{Dataset, JoinType, ProjectDataModel, Relation, WorkspaceSource};
use pivot_data::{resolve_model, DatasetLoadOptions};

fn csv_ds(id: &str, alias: &str, path: &str) -> Dataset {
    Dataset::from_legacy_source(
        id,
        alias,
        WorkspaceSource::File { path: path.into(), file_kind: Some("csv".into()), sheet: None },
    )
}

#[test]
fn lookup_resolution_preserves_rows_and_exports_xlsx() {
    let dir = tempfile::tempdir().unwrap();
    let base_csv = dir.path().join("base.csv");
    let lookup_csv = dir.path().join("lookup.csv");
    std::fs::write(&base_csv, "id,amt\n1,10\n2,20\n3,30\n").unwrap();
    std::fs::write(&lookup_csv, "id,label\n2,B\n").unwrap();

    let model = ProjectDataModel {
        datasets: vec![
            csv_ds("base", "base", &base_csv.to_string_lossy()),
            csv_ds("lookup", "lookup", &lookup_csv.to_string_lossy()),
        ],
        relations: vec![Relation {
            id: "r1".into(),
            name: Some("base->lookup".into()),
            left_dataset_id: "base".into(),
            left_column: "base.id".into(),
            right_dataset_id: "lookup".into(),
            right_column: "lookup.id".into(),
            join_type: JoinType::Lookup,
            label_columns: None,
            enabled: true,
        }],
        base_dataset_id: Some("base".into()),
        exposed_columns: None,
    };

    let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
    assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics.errors);
    let base_rows = 3usize;
    assert_eq!(resolved.table.row_count(), base_rows, "lookup must preserve base row count (m00581)");
    assert_eq!(resolved.diagnostics.output_rows, base_rows);
    assert_eq!(
        resolved.diagnostics.input_rows.get("base").copied(),
        Some(base_rows),
        "base input rows recorded in diagnostics"
    );

    let names: Vec<&str> = resolved.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"base.amt"), "resolved columns: {names:?}");
    assert!(names.iter().any(|n| n.contains("label")), "lookup label column appended: {names:?}");

    let spec = PivotSpec {
        rows: vec![FieldRef::new("base.id")],
        values: vec![ValueField {
            field: FieldRef::new("base.amt"),
            aggregate: Aggregate::Sum,
            display_name: None,
        }],
        ..Default::default()
    };
    let themed = pivot_core::pivot::compute(&resolved.table, &spec);
    assert_eq!(themed.body_rows.len(), base_rows, "pivot keeps one row per base id");
    assert!(!themed.header_rows.is_empty() || !themed.body_rows.is_empty());

    let out = dir.path().join("out.xlsx");
    pivot_export::export_xlsx(&out, &themed, pivot_export::ExportStyle::default()).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(!bytes.is_empty(), "xlsx export produced no bytes");
    assert!(bytes.starts_with(b"PK"), "not a zip/xlsx: {:?}", &bytes[..4.min(bytes.len())]);
}
