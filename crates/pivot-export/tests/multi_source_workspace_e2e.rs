//! End-to-end regression: multi-dataset `.pivot` workspace (v3) round-trip ->
//! lookup resolution -> pivot compute -> XLSX export.
//!
//! Covers the persisted-model path (save_workspace/load_workspace) rather than an
//! in-memory `ProjectDataModel`, plus the addendum m00581 invariant that lookup
//! relations preserve the base row count and append qualified label columns.

use pivot_core::{Aggregate, FieldRef, PivotSpec, ValueField};
use pivot_data::workspace::{
    Dataset, JoinType, ProjectDataModel, Relation, WorkspaceFile, WorkspaceSource, WorkspaceWorksheet,
};
use pivot_data::{load_workspace, resolve_model, save_workspace, DatasetLoadOptions};

#[test]
fn multi_source_workspace_roundtrip_resolve_compute_export() {
    let dir = tempfile::tempdir().unwrap();
    let ws_path = dir.path().join("multi.pivot");
    let out_path = dir.path().join("out.xlsx");

    // 1. Base CSV.
    let orders_csv = dir.path().join("orders.csv");
    std::fs::write(&orders_csv, "Order ID,customer_id,amt\n1,10,100\n2,20,200\n3,30,300\n").unwrap();

    // 2. Lookup XLSX (order 3 / customer_id 30 is unmatched -> null label).
    let customers_xlsx = dir.path().join("customers.xlsx");
    {
        let mut wb = rust_xlsxwriter::Workbook::new();
        let sh = wb.add_worksheet();
        sh.set_name("Customers").unwrap();
        sh.write_string(0, 0, "ID").unwrap();
        sh.write_string(0, 1, "Customer Name").unwrap();
        sh.write_number(1, 0, 10.0).unwrap();
        sh.write_string(1, 1, "Alice").unwrap();
        sh.write_number(2, 0, 20.0).unwrap();
        sh.write_string(2, 1, "Bob").unwrap();
        wb.save(&customers_xlsx).unwrap();
    }

    // 3. Workspace file with the v3 data model + one worksheet pivot spec.
    let spec = PivotSpec {
        rows: vec![FieldRef::new("orders.customer_id")],
        values: vec![ValueField {
            field: FieldRef::new("orders.amt"),
            aggregate: Aggregate::Sum,
            display_name: None,
        }],
        ..Default::default()
    };
    let data_model = ProjectDataModel {
        datasets: vec![
            Dataset::from_legacy_source(
                "orders",
                "orders",
                WorkspaceSource::File {
                    path: orders_csv.to_string_lossy().to_string(),
                    file_kind: Some("csv".into()),
                    sheet: None,
                },
            ),
            Dataset::from_legacy_source(
                "customers",
                "customers",
                WorkspaceSource::File {
                    path: customers_xlsx.to_string_lossy().to_string(),
                    file_kind: Some("xlsx".into()),
                    sheet: Some("Customers".into()),
                },
            ),
        ],
        relations: vec![Relation {
            id: "r1".into(),
            name: Some("orders->customers".into()),
            left_dataset_id: "orders".into(),
            left_column: "orders.customer_id".into(),
            right_dataset_id: "customers".into(),
            right_column: "customers.id".into(),
            join_type: JoinType::Lookup,
            label_columns: None,
            enabled: true,
        }],
        base_dataset_id: Some("orders".into()),
        exposed_columns: None,
    };
    let ws = WorkspaceFile {
        worksheets: vec![WorkspaceWorksheet {
            id: "sheet-1".into(),
            name: "Sheet 01".into(),
            number: 1,
            spec: spec.clone(),
            view: Default::default(),
        }],
        active_worksheet: 0,
        data_model,
        ..Default::default()
    };

    // 4. Save -> load round-trip.
    save_workspace(&ws_path, &ws).unwrap();
    let reloaded = load_workspace(&ws_path).unwrap();
    assert_eq!(reloaded.manifest.version, pivot_data::workspace::FORMAT_VERSION);
    assert_eq!(reloaded.data_model.datasets.len(), 2, "both datasets persist");
    assert_eq!(reloaded.data_model.relations.len(), 1, "relation persists");
    assert_eq!(reloaded.data_model.base_dataset_id.as_deref(), Some("orders"));
    assert_eq!(reloaded.worksheets.len(), 1);
    assert_eq!(reloaded.worksheets[0].spec.rows[0].name, "orders.customer_id");
    assert_eq!(reloaded.worksheets[0].spec.values[0].field.name, "orders.amt");
    assert_eq!(reloaded.worksheets[0].spec.values[0].aggregate, Aggregate::Sum);
    let reloaded_sources: Vec<&WorkspaceSource> =
        reloaded.data_model.datasets.iter().map(|d| &d.source).collect();
    match reloaded_sources[1] {
        WorkspaceSource::File { file_kind, sheet, .. } => {
            assert_eq!(file_kind.as_deref(), Some("xlsx"));
            assert_eq!(sheet.as_deref(), Some("Customers"));
        }
        other => panic!("expected file dataset, got {other:?}"),
    }

    // 5. Resolve the reloaded model: base rows preserved, label column appended.
    let resolved = resolve_model(&reloaded.data_model, &DatasetLoadOptions::default(), None).unwrap();
    assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics.errors);
    assert_eq!(resolved.table.row_count(), 3, "lookup must preserve base row count (m00581)");
    assert_eq!(resolved.diagnostics.input_rows.get("orders").copied(), Some(3));
    assert_eq!(resolved.diagnostics.output_rows, 3);
    let names: Vec<&str> = resolved.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"orders.customer_id"), "resolved columns: {names:?}");
    assert!(names.contains(&"customers.customer_name"), "resolved columns: {names:?}");
    assert!(
        resolved.diagnostics.warnings.iter().any(|w| w.contains("null labels") || w.contains("no lookup match")),
        "unmatched order 3 should be reported: {:?}",
        resolved.diagnostics.warnings
    );

    // 6. Compute the persisted spec against the resolved table.
    let themed = pivot_core::pivot::compute(&resolved.table, &reloaded.worksheets[0].spec);
    assert_eq!(themed.body_rows.len(), 3, "one pivot row per distinct customer_id");

    // 7. Export XLSX through the crate's public entry point.
    pivot_export::export_xlsx(&out_path, &themed, pivot_export::ExportStyle::default()).unwrap();
    assert!(out_path.exists(), "export did not create a file");
    let bytes = std::fs::read(&out_path).unwrap();
    assert!(!bytes.is_empty(), "xlsx export produced no bytes");
    assert!(bytes.starts_with(b"PK"), "not a zip/xlsx: {:?}", &bytes[..4.min(bytes.len())]);
}
