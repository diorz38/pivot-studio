use crate::registry::{load_dataset_table, DatasetLoadOptions, LoadedDataset};
use crate::workspace::{ProjectDataModel, Relation, WorkspaceSource};
use crate::{DataError, Result};
use pivot_core::{Column, DataTable, Value};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

const LOOKUP_DUPLICATE_SAMPLE: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct ResolveDiagnostics {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub input_rows: BTreeMap<String, usize>,
    pub output_rows: usize,
}

impl ResolveDiagnostics {
    pub fn ok(&self) -> bool { self.errors.is_empty() }
}

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub table: DataTable,
    pub diagnostics: ResolveDiagnostics,
}

/// Resolve a project data model into a single analysis `DataTable`.
/// Addendum (m00581): relations are **simple lookup/enrichment** relations.
/// Rule: do **not** change the sum/row count of processed base data. A relation
/// replaces a code/id with meaningful labels from a related lookup table.
/// - Preserves every base row (no inner-join row filtering).
/// - Must not multiply base rows: duplicate lookup keys are a validation error.
/// - Unmatched/null keys produce null labels.
/// - Legacy `Inner` relations are rejected (they would filter base rows).
pub fn resolve_model(model: &ProjectDataModel, opts: &DatasetLoadOptions, cancel: Option<&AtomicBool>) -> Result<ResolvedModel> {
    let mut diag = ResolveDiagnostics::default();
    let mut loaded: BTreeMap<String, LoadedDataset> = BTreeMap::new();
    for ds in &model.datasets {
        if !ds.enabled { continue; }
        if matches!(ds.source, WorkspaceSource::None) { diag.warnings.push(format!("dataset '{}' skipped: no source", ds.alias)); continue; }
        match load_dataset_table(ds, opts, cancel) {
            Ok(mut ld) => {
                diag.input_rows.insert(ld.dataset_id.clone(), ld.row_count);
                diag.warnings.extend(ld.warnings.clone());
                let mut qual_w = Vec::new();
                crate::registry::qualify_column_names(&mut ld.table, &ld.alias, &mut qual_w);
                ld.warnings.extend(qual_w);
                loaded.insert(ld.dataset_id.clone(), ld);
            }
            Err(e) => diag.errors.push(format!("dataset '{}' ({}): {e}", ds.alias, ds.id)),
        }
    }
    if !diag.errors.is_empty() { return Err(DataError::Invalid(diag.errors.join("; "))); }
    let base_id = model.base_dataset_id.clone()
        .filter(|id| loaded.contains_key(id))
        .or_else(|| model.datasets.iter().find(|d| d.enabled && loaded.contains_key(&d.id)).map(|d| d.id.clone()));
    let Some(base_id) = base_id else {
        return Ok(ResolvedModel { table: DataTable { columns: vec![], rows: vec![], source: Some("resolved:empty".into()) }, diagnostics: diag });
    };
    let mut current = loaded.get(&base_id).ok_or_else(|| DataError::Invalid(format!("base dataset '{base_id}' not loaded")))?.table.clone();
    let base_rows = current.row_count();
    let mut joined: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    joined.insert(base_id.clone());
    let mut relations: Vec<&Relation> = model.relations.iter().filter(|r| r.enabled).collect();
    relations.sort_by(|a, b| a.id.cmp(&b.id));
    // Deterministic pass: apply lookup relations that connect processed data to an unjoined lookup dataset.
    let mut progress = true;
    while progress {
        progress = false;
        for rel in &relations {
            let (left_in, right_in) = (joined.contains(&rel.left_dataset_id), joined.contains(&rel.right_dataset_id));
            if left_in && right_in { continue; }
            if !rel.join_type.is_base_preserving() {
                diag.errors.push(format!("relation '{}' uses {:?} which is not supported: lookup relations must not filter base rows (addendum m00581)", rel.id, rel.join_type));
                continue;
            }
            if left_in && !right_in {
                let Some(lookup) = loaded.get(&rel.right_dataset_id) else { diag.errors.push(format!("relation '{}' references missing dataset '{}'", rel.id, rel.right_dataset_id)); continue; };
                current = apply_lookup(&current, &lookup.table, &rel.left_column, &rel.right_column, rel, &lookup.alias, &mut diag)?;
                joined.insert(rel.right_dataset_id.clone());
                progress = true;
            } else if !left_in && right_in {
                let Some(lookup) = loaded.get(&rel.left_dataset_id) else { diag.errors.push(format!("relation '{}' references missing dataset '{}'", rel.id, rel.left_dataset_id)); continue; };
                current = apply_lookup(&current, &lookup.table, &rel.right_column, &rel.left_column, rel, &lookup.alias, &mut diag)?;
                joined.insert(rel.left_dataset_id.clone());
                progress = true;
            }
        }
    }
    if !diag.errors.is_empty() { return Err(DataError::Invalid(diag.errors.join("; "))); }
    for rel in &relations {
        if !(joined.contains(&rel.left_dataset_id) && joined.contains(&rel.right_dataset_id)) {
            diag.warnings.push(format!("relation '{}' was not applied (graph not connected to base in MVP resolver)", rel.id));
        }
    }
    current.source = Some(format!("resolved:{}+{}", base_id, joined.len().saturating_sub(1)));
    diag.output_rows = current.rows.len();
    if diag.output_rows != base_rows {
        return Err(DataError::Invalid(format!(
            "lookup resolution changed base row count (base {base_rows} -> output {}), which violates simple-relation rule (m00581): {:?}",
            diag.output_rows, diag.errors
        )));
    }
    apply_exposed_columns(&mut current, model, &mut diag)?;
    Ok(ResolvedModel { table: current, diagnostics: diag })
}

/// Apply a simple lookup/enrichment relation (addendum m00581).
/// Preserves every base row (base row count unchanged). Replaces/enriches by
/// appending selected label columns from the lookup table matched on key.
/// - Unmatched/null base keys -> null labels.
/// - Duplicate lookup keys -> validation error (would create fanout/ambiguous label).
fn apply_lookup(base: &DataTable, lookup: &DataTable, base_key: &str, lookup_key: &str, rel: &Relation, lookup_alias: &str, diag: &mut ResolveDiagnostics) -> Result<DataTable> {
    let bi = resolve_col_index(base, base_key).ok_or_else(|| DataError::Invalid(format!("relation '{}' base key '{}' not found", rel.id, base_key)))?;
    let li = resolve_col_index(lookup, lookup_key).ok_or_else(|| DataError::Invalid(format!("relation '{}' lookup key '{}' not found in '{}'", rel.id, lookup_key, lookup_alias)))?;

    // Build lookup index: key -> exactly one row index. Duplicate non-null keys are errors (no fanout).
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    let mut duplicate_keys: Vec<String> = Vec::new();
    for (ridx, row) in lookup.rows.iter().enumerate() {
        let v = row.get(li).unwrap_or(&Value::Null);
        if matches!(v, Value::Null) { continue; }
        let key = join_key_display(v);
        if index.insert(key.clone(), ridx).is_some() && !duplicate_keys.iter().any(|k| k == &key) {
            duplicate_keys.push(key);
            if duplicate_keys.len() >= LOOKUP_DUPLICATE_SAMPLE { /* keep sampling cap */ }
        }
    }
    if !duplicate_keys.is_empty() {
        let sample = duplicate_keys.iter().take(LOOKUP_DUPLICATE_SAMPLE).cloned().collect::<Vec<_>>().join(", ");
        return Err(DataError::Invalid(format!(
            "relation '{}': lookup key '{}' in dataset '{}' has duplicate values (sample: {}); lookup relations must not multiply base rows (addendum m00581). Deduplicate the lookup table or use a unique key.",
            rel.id, lookup_key, lookup_alias, sample
        )));
    }

    let mut out_cols: Vec<Column> = base.columns.clone();
    let requested_labels = rel.label_columns.as_ref()
        .map(|v| v.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut candidate_idx: Vec<usize> = if requested_labels.is_empty() {
        (0..lookup.columns.len()).filter(|&i| i != li).collect()
    } else {
        let mut picked: Vec<usize> = Vec::new();
        for name in &requested_labels {
            let Some(idx) = resolve_col_index(lookup, name) else {
                return Err(DataError::Invalid(format!("relation '{}': lookup label column '{}' not found in dataset '{}'", rel.id, name, lookup_alias)));
            };
            if idx == li {
                diag.warnings.push(format!("relation '{}': lookup label column '{}' is the lookup key; skipped", rel.id, name));
                continue;
            }
            if !picked.contains(&idx) { picked.push(idx); }
        }
        if picked.is_empty() {
            return Err(DataError::Invalid(format!("relation '{}': no valid lookup label columns selected for dataset '{}'", rel.id, lookup_alias)));
        }
        picked.sort_unstable();
        picked
    };
    candidate_idx.sort_unstable();
    candidate_idx.dedup();
    let mut lookup_keep: Vec<(usize, Column)> = Vec::new();
    for i in candidate_idx {
        let c = &lookup.columns[i];
        if out_cols.iter().any(|oc| oc.name == c.name) || lookup_keep.iter().any(|(_, oc)| oc.name == c.name) {
            let mut name = format!("{}_label", c.name);
            let mut n = 2usize;
            while out_cols.iter().any(|oc| oc.name == name) || lookup_keep.iter().any(|(_, oc)| oc.name == name) { name = format!("{}_label{n}", c.name); n += 1; }
            let mut cc = c.clone(); cc.name = name;
            diag.warnings.push(format!("relation '{}': duplicate output column '{}' renamed to '{}'", rel.id, c.name, cc.name));
            lookup_keep.push((i, cc));
        } else {
            lookup_keep.push((i, c.clone()));
        }
    }
    if requested_labels.is_empty() && lookup_keep.is_empty() {
        return Err(DataError::Invalid(format!("relation '{}': lookup dataset '{}' has no label columns (besides key)", rel.id, lookup_alias)));
    }
    out_cols.extend(lookup_keep.iter().map(|(_, c)| c.clone()));
    let lookup_nulls: Vec<Value> = lookup_keep.iter().map(|_| Value::Null).collect();

    let mut rows = Vec::with_capacity(base.rows.len());
    let mut unmatched_base = 0usize;
    for brow in &base.rows {
        let bval = brow.get(bi).unwrap_or(&Value::Null);
        if matches!(bval, Value::Null) {
            let mut out = brow.clone();
            out.extend(lookup_nulls.clone());
            rows.push(out);
            unmatched_base += 1;
            continue;
        }
        let key = join_key_display(bval);
        if let Some(&ridx) = index.get(&key) {
            let lrow = &lookup.rows[ridx];
            let mut out = brow.clone();
            for (i, _) in &lookup_keep { out.push(lrow.get(*i).cloned().unwrap_or(Value::Null)); }
            rows.push(out);
        } else {
            unmatched_base += 1;
            let mut out = brow.clone();
            out.extend(lookup_nulls.clone());
            rows.push(out);
        }
    }
    debug_assert_eq!(rows.len(), base.rows.len(), "lookup relation must preserve base row count");
    if unmatched_base > 0 {
        diag.warnings.push(format!("relation '{}': {} base rows had no lookup match/null key; null labels filled", rel.id, unmatched_base));
    }
    Ok(DataTable { columns: out_cols, rows, source: Some(format!("lookup:{}", rel.id)) })
}

/// Apply `ProjectDataModel.exposed_columns` as a deterministic projection on the resolved table.
/// `None` is a no-op (expose all resolved columns). When some (possibly empty after
/// validation), the resolved table keeps only those columns in the requested order;
/// unknown names are a hard error so stale `PivotSpec` refs fail loudly instead of silently.
/// Resolution matches qualified, unqualified, original/display, and tail forms.
fn apply_exposed_columns(table: &mut DataTable, model: &ProjectDataModel, diag: &mut ResolveDiagnostics) -> Result<()> {
    let Some(exposed) = &model.exposed_columns else { return Ok(()); };
    if exposed.is_empty() {
        diag.warnings.push("exposed_columns is empty: resolved table has no exposed columns".to_string());
        table.columns.clear();
        for row in &mut table.rows { row.clear(); }
        return Ok(());
    }
    let mut keep_idx: Vec<usize> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for name in exposed {
        let n = name.trim();
        if n.is_empty() { continue; }
        match resolve_col_index(table, n) {
            Some(idx) => { if !keep_idx.contains(&idx) { keep_idx.push(idx); } }
            None => missing.push(n.to_string()),
        }
    }
    if !missing.is_empty() {
        return Err(DataError::Invalid(format!(
            "exposed_columns not found in resolved model (sample: {}): {}",
            missing.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
            missing.len()
        )));
    }
    if keep_idx.is_empty() {
        diag.warnings.push("exposed_columns produced no columns after validation".to_string());
        table.columns.clear();
        for row in &mut table.rows { row.clear(); }
        return Ok(());
    }
    let old_cols = table.columns.clone();
    for row in &mut table.rows {
        let old = std::mem::take(row);
        *row = keep_idx.iter().map(|&i| old.get(i).cloned().unwrap_or(Value::Null)).collect();
    }
    table.columns = keep_idx.iter().map(|&i| old_cols[i].clone()).collect();
    Ok(())
}

fn resolve_col_index(table: &DataTable, key: &str) -> Option<usize> {
    let k = key.trim();
    if k.is_empty() { return None; }
    table.columns.iter().position(|c| c.name == k)
        .or_else(|| table.columns.iter().position(|c| c.name.eq_ignore_ascii_case(k)))
        .or_else(|| table.columns.iter().position(|c| c.original_name.as_deref() == Some(k)))
        .or_else(|| table.columns.iter().position(|c| c.name.ends_with(&format!(".{k}")) || c.name == k))
}

fn join_key_display(v: &Value) -> String {
    // MVP coercion: compare by canonical display; nulls excluded from match index.
    match v {
        Value::Null => String::new(),
        Value::Integer(n) => n.to_string(),
        Value::Decimal(n) => {
            if n.fract() == 0.0 && n.abs() < (i64::MAX as f64) { format!("{}", *n as i64) } else { n.to_string() }
        }
        Value::Boolean(b) => if *b { "true".into() } else { "false".into() },
        Value::String(s) => s.trim().to_string(),
        Value::Date(s) | Value::DateTime(s) => s.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{Dataset, JoinType, Relation, WorkspaceSource};

    fn csv_ds(id: &str, alias: &str, path: &str) -> Dataset {
        Dataset::from_legacy_source(id, alias, WorkspaceSource::File { path: path.into(), file_kind: Some("csv".into()), sheet: None })
    }

    #[test]
    fn resolves_csv_xlsx_left_join() {
        let dir = tempfile::tempdir().unwrap();
        let orders = dir.path().join("orders.csv");
        std::fs::write(&orders, "Order ID,customer_id\n1,10\n2,20\n3,30\n").unwrap();
        let cust = dir.path().join("customers.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let sh = wb.add_worksheet(); sh.set_name("Customers").unwrap();
        sh.write_string(0, 0, "ID").unwrap(); sh.write_string(0, 1, "Customer Name").unwrap();
        sh.write_number(1, 0, 10.0).unwrap(); sh.write_string(1, 1, "Alice").unwrap();
        sh.write_number(2, 0, 20.0).unwrap(); sh.write_string(2, 1, "Bob").unwrap();
        wb.save(&cust).unwrap();

        let model = ProjectDataModel {
            datasets: vec![
                csv_ds("orders", "orders", orders.to_string_lossy().as_ref()),
                Dataset::from_legacy_source("customers", "customers", WorkspaceSource::File { path: cust.to_string_lossy().to_string(), file_kind: Some("xlsx".into()), sheet: Some("Customers".into()) }),
            ],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "orders".into(), left_column: "orders.customer_id".into(), right_dataset_id: "customers".into(), right_column: "customers.id".into(), join_type: JoinType::Left, label_columns: None, enabled: true }],
            base_dataset_id: Some("orders".into()),
            exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics.errors);
        assert_eq!(resolved.table.row_count(), 3); // left join keeps unmatched order 30
        let names: Vec<_> = resolved.table.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.iter().any(|n| *n == "orders.order_id"));
        assert!(names.iter().any(|n| n.contains("customer_name")));
    }

    #[test]
    fn inner_relation_rejected_by_lookup_rule() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id,v\n1,a\n2,b\n").unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,w\n2,x\n").unwrap();
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a","a",a.to_string_lossy().as_ref()), csv_ds("b","b",b.to_string_lossy().as_ref())],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "a".into(), left_column: "a.id".into(), right_dataset_id: "b".into(), right_column: "b.id".into(), join_type: JoinType::Inner, label_columns: None, enabled: true }],
            base_dataset_id: Some("a".into()), exposed_columns: None,
        };
        let err = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap_err().to_string();
        assert!(err.to_lowercase().contains("inner") || err.contains("lookup") || err.contains("m00581"), "{err}");
    }

    #[test]
    fn duplicate_lookup_keys_rejected_no_fanout() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id\n1\n2\n").unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,v\n1,x\n1,y\n").unwrap();
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a","a",a.to_string_lossy().as_ref()), csv_ds("b","b",b.to_string_lossy().as_ref())],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "a".into(), left_column: "a.id".into(), right_dataset_id: "b".into(), right_column: "b.id".into(), join_type: JoinType::Lookup, label_columns: None, enabled: true }],
            base_dataset_id: Some("a".into()), exposed_columns: None,
        };
        let err = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap_err().to_string();
        assert!(err.to_lowercase().contains("duplicate") || err.contains("fanout") || err.contains("m00581"), "{err}");
    }

    #[test]
    fn lookup_preserves_base_row_count_and_fills_labels() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id,amt\n1,10\n2,20\n3,30\n").unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,label\n2,B\n").unwrap();
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a","a",a.to_string_lossy().as_ref()), csv_ds("b","b",b.to_string_lossy().as_ref())],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "a".into(), left_column: "a.id".into(), right_dataset_id: "b".into(), right_column: "b.id".into(), join_type: JoinType::Lookup, label_columns: None, enabled: true }],
            base_dataset_id: Some("a".into()), exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics.errors);
        assert_eq!(resolved.table.row_count(), 3); // base row count preserved (id 1,3 unmatched -> null labels)
        assert!(resolved.diagnostics.warnings.iter().any(|w| w.contains("null labels") || w.contains("no lookup match")));
    }

    #[test]
    fn exposed_columns_projection_limits_resolved_fields() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id,amt\n1,10\n2,20\n").unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,label\n1,A\n2,B\n").unwrap();
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a","a",a.to_string_lossy().as_ref()), csv_ds("b","b",b.to_string_lossy().as_ref())],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "a".into(), left_column: "a.id".into(), right_dataset_id: "b".into(), right_column: "b.id".into(), join_type: JoinType::Lookup, label_columns: None, enabled: true }],
            base_dataset_id: Some("a".into()),
            exposed_columns: Some(vec!["a.amt".into(), "b.label".into()]),
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics);
        assert_eq!(resolved.table.column_count(), 2);
        let names: Vec<_> = resolved.table.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a.amt", "b.label"]);
        assert_eq!(resolved.table.row_count(), 2);
    }

    #[test]
    fn exposed_columns_unknown_names_error() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id,amt\n1,10\n").unwrap();
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a","a",a.to_string_lossy().as_ref())],
            relations: vec![],
            base_dataset_id: Some("a".into()),
            exposed_columns: Some(vec!["a.missing".into()]),
        };
        let err = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap_err().to_string();
        assert!(err.contains("exposed_columns") || err.contains("not found"), "{err}");
    }

    #[test]
    fn lookup_label_columns_selection_limits_appended_labels() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id,amt\n1,10\n2,20\n").unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,label,group\n1,A,G1\n2,B,G2\n").unwrap();
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a","a",a.to_string_lossy().as_ref()), csv_ds("b","b",b.to_string_lossy().as_ref())],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "a".into(), left_column: "a.id".into(), right_dataset_id: "b".into(), right_column: "b.id".into(), join_type: JoinType::Lookup, label_columns: Some(vec!["b.label".into()]), enabled: true }],
            base_dataset_id: Some("a".into()), exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics);
        let names: Vec<_> = resolved.table.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("label")), "{names:?}");
        assert!(!names.iter().any(|n| n.contains("group")), "group should not be appended when label_columns selected: {names:?}");
        assert_eq!(resolved.table.row_count(), 2);
    }

    /// Large-data safety: `Dataset.row_limit` bounds what the resolver materializes,
    /// the cap is visible in diagnostics, and lookup invariants still hold on capped data.
    #[test]
    fn row_limit_cap_is_bounded_and_visible_in_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv");
        let mut csv = String::from("id,amt\n");
        for i in 1..=500 { csv.push_str(&format!("{i},{}\n", i * 10)); }
        std::fs::write(&a, csv).unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,label\n1,one\n2,two\n").unwrap();

        let mut base = csv_ds("a", "a", a.to_string_lossy().as_ref());
        base.row_limit = Some(200);
        let model = ProjectDataModel {
            datasets: vec![base, csv_ds("b", "b", b.to_string_lossy().as_ref())],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "a".into(), left_column: "a.id".into(), right_dataset_id: "b".into(), right_column: "b.id".into(), join_type: JoinType::Lookup, label_columns: None, enabled: true }],
            base_dataset_id: Some("a".into()), exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics);
        assert_eq!(resolved.table.row_count(), 200, "row_limit must bound materialized base rows");
        assert_eq!(resolved.diagnostics.input_rows.get("a").copied(), Some(200));
        assert_eq!(resolved.diagnostics.output_rows, 200);
    }

    /// Cross-type lookup keys match through the documented MVP coercion
    /// (canonical display: trimmed, integral decimals rendered as integers).
    /// Unmatched keys still yield null labels and never drop base rows.
    #[test]
    fn cross_type_keys_match_via_canonical_display() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.csv");
        std::fs::write(&base, "id,amt\n42,10\n7,20\n").unwrap();
        let lookup = dir.path().join("lk.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let sh = wb.add_worksheet();
        sh.write_string(0, 0, "ID").unwrap();
        sh.write_string(0, 1, "Label").unwrap();
        sh.write_string(1, 0, " 42 ").unwrap(); // text key vs integer key
        sh.write_string(1, 1, "Answer").unwrap();
        wb.save(&lookup).unwrap();

        let model = ProjectDataModel {
            datasets: vec![
                csv_ds("base", "base", base.to_string_lossy().as_ref()),
                Dataset::from_legacy_source("lk", "lk", WorkspaceSource::File { path: lookup.to_string_lossy().to_string(), file_kind: Some("xlsx".into()), sheet: None }),
            ],
            relations: vec![Relation { id: "r1".into(), name: None, left_dataset_id: "base".into(), left_column: "base.id".into(), right_dataset_id: "lk".into(), right_column: "lk.id".into(), join_type: JoinType::Lookup, label_columns: Some(vec!["lk.label".into()]), enabled: true }],
            base_dataset_id: Some("base".into()),
            exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics);
        assert_eq!(resolved.table.row_count(), 2, "unmatched key must not drop base rows");
        let label_idx = resolved.table.columns.iter().position(|c| c.name.ends_with("label")).expect("label column appended");
        assert_eq!(resolved.table.rows[0][label_idx], pivot_core::Value::String("Answer".into()), "text ' 42 ' must match integer 42");
        assert_eq!(resolved.table.rows[1][label_idx], pivot_core::Value::Null, "unmatched key yields null label");
        assert!(resolved.diagnostics.warnings.iter().any(|w| w.contains("null labels") || w.contains("no lookup match")));
    }

    /// A cyclic pair of relations must terminate deterministically and must not
    /// re-append labels (the reverse relation is already satisfied once joined).
    #[test]
    fn relation_cycle_terminates_without_duplicate_labels() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.csv"); std::fs::write(&a, "id,x\n1,a1\n2,a2\n").unwrap();
        let b = dir.path().join("b.csv"); std::fs::write(&b, "id,y\n1,b1\n2,b2\n").unwrap();
        let rel = |id: &str, left: &str, right: &str| Relation {
            id: id.into(), name: None,
            left_dataset_id: left.into(), left_column: format!("{left}.id"),
            right_dataset_id: right.into(), right_column: format!("{right}.id"),
            join_type: JoinType::Lookup, label_columns: None, enabled: true,
        };
        let model = ProjectDataModel {
            datasets: vec![csv_ds("a", "a", a.to_string_lossy().as_ref()), csv_ds("b", "b", b.to_string_lossy().as_ref())],
            relations: vec![rel("r1", "a", "b"), rel("r2", "b", "a")],
            base_dataset_id: Some("a".into()),
            exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics);
        assert_eq!(resolved.table.row_count(), 2);
        let labels = resolved.table.columns.iter().filter(|c| c.name.ends_with(".y")).count();
        assert_eq!(labels, 1, "reverse relation must not append a second label set");
    }

    /// User scenario (m01660): main CSV key `kegiatan_id` relates to XLSX `id`,
    /// and several XLSX columns are exposed in the main dataset.
    #[test]
    fn multiple_label_columns_are_appended_in_lookup_schema_order() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.csv");
        std::fs::write(&main, "id,kegiatan_id,nilai\n1,10,100\n2,20,200\n3,99,300\n").unwrap();
        let lookup = dir.path().join("kegiatan.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let sh = wb.add_worksheet();
        for (c, h) in ["id", "tahun", "fungsi", "kode_kegiatan", "nama", "volume"].iter().enumerate() {
            sh.write_string(0, c as u16, *h).unwrap();
        }
        let rows = [["10", "2026", "Perencanaan", "K01", "Sensus Penduduk", "5"],
                    ["20", "2026", "Produksi", "K02", "Sensus Pertanian", "7"]];
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                sh.write_string(r as u32 + 1, c as u16, *v).unwrap();
            }
        }
        wb.save(&lookup).unwrap();

        let model = ProjectDataModel {
            datasets: vec![
                csv_ds("main", "main", main.to_string_lossy().as_ref()),
                Dataset::from_legacy_source("kg", "kg", WorkspaceSource::File { path: lookup.to_string_lossy().to_string(), file_kind: Some("xlsx".into()), sheet: None }),
            ],
            relations: vec![Relation {
                id: "r1".into(), name: None,
                left_dataset_id: "main".into(), left_column: "main.kegiatan_id".into(),
                right_dataset_id: "kg".into(), right_column: "kg.id".into(),
                join_type: JoinType::Lookup,
                label_columns: Some(vec!["kg.nama".into(), "kg.fungsi".into(), "kg.kode_kegiatan".into()]),
                enabled: true,
            }],
            base_dataset_id: Some("main".into()),
            exposed_columns: None,
        };
        let resolved = resolve_model(&model, &DatasetLoadOptions::default(), None).unwrap();
        assert!(resolved.diagnostics.ok(), "{:?}", resolved.diagnostics);
        assert_eq!(resolved.table.row_count(), 3, "lookup must not change main row count");
        let names: Vec<String> = resolved.table.columns.iter().map(|c| c.name.clone()).collect();
        let idx = |tail: &str| names.iter().position(|n| n.ends_with(tail)).unwrap_or_else(|| panic!("missing {tail}: {names:?}"));
        let (fungsi, kode, nama) = (idx(".fungsi"), idx(".kode_kegiatan"), idx(".nama"));
        assert!(fungsi < kode && kode < nama, "labels keep lookup schema order: {names:?}");
        assert_eq!(resolved.table.rows[0][nama], pivot_core::Value::String("Sensus Penduduk".into()));
        assert_eq!(resolved.table.rows[0][fungsi], pivot_core::Value::String("Perencanaan".into()));
        assert_eq!(resolved.table.rows[0][kode], pivot_core::Value::String("K01".into()));
        assert_eq!(resolved.table.rows[2][nama], pivot_core::Value::Null, "unmatched key keeps the row with an empty value");
        assert!(!names.iter().any(|n| n.ends_with(".tahun") || n.ends_with(".volume")), "unselected lookup columns stay out: {names:?}");
    }
}
