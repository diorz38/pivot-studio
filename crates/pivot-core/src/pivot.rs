//! In-memory pivot computation (Phase 2, AC2.1-AC2.3).

use crate::model::{Cell, CellRole, FooterBand, FooterBandKind, ThemedTableModel};
use crate::spec::{Aggregate, Filter, FilterOp, PivotSpec, ValueField};
use crate::{DataTable, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Key(Vec<String>);

impl Key {
    fn display_part(&self, idx: usize) -> &str {
        self.0.get(idx).map(|s| s.as_str()).unwrap_or("")
    }
}

#[derive(Debug, Default)]
struct Accum {
    sum: f64,
    count: i64,
    non_null: i64,
    min: Option<f64>,
    max: Option<f64>,
    distinct: BTreeSet<String>,
    saw_number: bool,
}

impl Accum {
    fn push(&mut self, v: &Value, agg: Aggregate) {
        match agg {
            Aggregate::Count => {
                self.count += 1;
                if !matches!(v, Value::Null) {
                    self.non_null += 1;
                }
                return;
            }
            Aggregate::CountDistinct => {
                if !matches!(v, Value::Null) {
                    self.distinct.insert(v.as_display());
                }
                return;
            }
            _ => {}
        }
        let num = numeric(v);
        let Some(n) = num else {
            return;
        };
        self.saw_number = true;
        self.non_null += 1;
        self.count += 1;
        self.sum += n;
        self.min = Some(self.min.map_or(n, |m| m.min(n)));
        self.max = Some(self.max.map_or(n, |m| m.max(n)));
    }

    fn finish(&self, agg: Aggregate) -> Value {
        match agg {
            Aggregate::Count => Value::Integer(self.count),
            Aggregate::CountDistinct => Value::Integer(self.distinct.len() as i64),
            Aggregate::Sum => {
                if self.saw_number || self.non_null > 0 {
                    numeric_value(self.sum)
                } else {
                    Value::Null
                }
            }
            Aggregate::Average => {
                if self.non_null > 0 {
                    numeric_value(self.sum / self.non_null as f64)
                } else {
                    Value::Null
                }
            }
            Aggregate::Min => self.min.map(numeric_value).unwrap_or(Value::Null),
            Aggregate::Max => self.max.map(numeric_value).unwrap_or(Value::Null),
        }
    }
}

fn numeric(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(*n as f64),
        Value::Decimal(n) if n.is_finite() => Some(*n),
        Value::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => {
            let t = s.trim().replace(',', "");
            if t.is_empty() { return None; }
            t.parse::<f64>().ok().filter(|n| n.is_finite())
        }
        _ => None,
    }
}

fn numeric_value(n: f64) -> Value {
    if n.is_finite() && n.fract() == 0.0 && n.abs() <= (i64::MAX as f64) {
        Value::Integer(n as i64)
    } else if n.is_finite() {
        Value::Decimal(n)
    } else {
        Value::Null
    }
}

fn cell_text(table: &DataTable, row: usize, col: usize) -> String {
    table.get(row, col).map(|v| v.as_display()).unwrap_or_default()
}

fn passes_filters(table: &DataTable, row: usize, filters: &[Filter], idx: &BTreeMap<String, usize>) -> bool {
    for f in filters {
        let Some(&ci) = idx.get(&f.field.name) else { continue; };
        let v = table.get(row, ci).cloned().unwrap_or(Value::Null);
        let in_set = f.values.iter().any(|x| x == &v || x.as_display() == v.as_display());
        let ok = match f.op {
            FilterOp::Include => f.values.is_empty() || in_set,
            FilterOp::Exclude => !in_set,
            FilterOp::Equals => f.values.first().map(|x| x == &v || x.as_display() == v.as_display()).unwrap_or(false),
            FilterOp::NotEquals => f.values.first().map(|x| x != &v && x.as_display() != v.as_display()).unwrap_or(true),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Compute a pivot and produce a canonical `ThemedTableModel` (AC2.x/AC3 foundations).
pub fn compute(table: &DataTable, spec: &PivotSpec) -> ThemedTableModel {
    let idx: BTreeMap<String, usize> = table
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name.clone(), i))
        .collect();

    let row_fields: Vec<usize> = spec.rows.iter().filter_map(|f| idx.get(&f.name).copied()).collect();
    let col_fields: Vec<usize> = spec.columns.iter().filter_map(|f| idx.get(&f.name).copied()).collect();
    let value_fields: Vec<(usize, &ValueField)> = spec
        .values
        .iter()
        .filter_map(|vf| idx.get(&vf.field.name).copied().map(|i| (i, vf)))
        .collect();

    let mut col_keys: BTreeSet<Key> = BTreeSet::new();
    let mut row_keys: BTreeSet<Key> = BTreeSet::new();
    // (row_key, col_key, value_index) -> accumulator
    let mut cells: BTreeMap<(Key, Key, usize), Accum> = BTreeMap::new();

    for r in 0..table.row_count() {
        if !passes_filters(table, r, &spec.filters, &idx) {
            continue;
        }
        let rk = Key(if row_fields.is_empty() {
            vec!["(All)".to_string()]
        } else {
            row_fields.iter().map(|&ci| cell_text(table, r, ci)).collect()
        });
        let ck = Key(if col_fields.is_empty() {
            vec![]
        } else {
            col_fields.iter().map(|&ci| cell_text(table, r, ci)).collect()
        });
        row_keys.insert(rk.clone());
        col_keys.insert(ck.clone());
        for (vi, (ci, vf)) in value_fields.iter().enumerate() {
            let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
            cells.entry((rk.clone(), ck.clone(), vi)).or_default().push(&v, vf.aggregate);
        }
    }
    if row_keys.is_empty() && !table.rows.is_empty() {
        // all filtered out -> keep empty
    } else if row_keys.is_empty() && table.rows.is_empty() && !value_fields.is_empty() {
        row_keys.insert(Key(vec![]));
    }

    // Build column headers
    let row_header_cols = spec.rows.len().max(1);
    let col_key_vec: Vec<Key> = if spec.columns.is_empty() { vec![Key(vec![])] } else { col_keys.into_iter().collect() };
    let value_count = value_fields.len().max(if spec.values.is_empty() { 0 } else { 1 });
    let effective_values: Vec<String> = if value_fields.is_empty() {
        vec![]
    } else {
        value_fields.iter().map(|(_, vf)| vf.effective_name()).collect()
    };

    let mut header_rows: Vec<Vec<Cell>> = Vec::new();
    if spec.columns.is_empty() {
        // single header row
        let mut hr = Vec::new();
        hr.push(Cell::new(spec.stub_head.clone().unwrap_or_default(), CellRole::StubHead));
        for _ in 1..row_header_cols {
            hr.push(Cell::new("", CellRole::RowHeader));
        }
        if effective_values.is_empty() {
            hr.push(Cell::new("Value", CellRole::ColHeader));
        } else {
            for name in &effective_values {
                hr.push(Cell::new(name, CellRole::ColHeader));
            }
        }
        header_rows.push(hr);
    } else {
        let levels = spec.columns.len();
        for level in 0..levels {
            let mut hr = Vec::new();
            if level == 0 {
                hr.push(Cell::span(spec.stub_head.clone().unwrap_or_default(), CellRole::StubHead, row_header_cols, levels));
            }
            // group consecutive same labels at this level given same prefix
            let mut i = 0;
            while i < col_key_vec.len() {
                let label = col_key_vec[i].display_part(level).to_string();
                let mut span = 1usize;
                while i + span < col_key_vec.len() {
                    let same_prefix = (0..=level).all(|l| col_key_vec[i].display_part(l) == col_key_vec[i + span].display_part(l));
                    if same_prefix {
                        span += 1;
                    } else {
                        break;
                    }
                }
                hr.push(Cell::span(label, CellRole::ColHeader, span * value_count.max(1), 1));
                i += span;
            }
            header_rows.push(hr);
        }
        if !effective_values.is_empty() && levels > 0 && effective_values.len() > 1 {
            let mut hr = Vec::new();
            for _ in 0..row_header_cols {
                hr.push(Cell::new("", CellRole::RowHeader));
            }
            for ck in &col_key_vec {
                for name in &effective_values {
                    let _ = ck;
                    hr.push(Cell::new(name, CellRole::ColHeader));
                }
            }
            header_rows.push(hr);
        }
    }

    // Body rows
    let mut body_rows: Vec<Vec<Cell>> = Vec::new();
    let row_key_vec: Vec<Key> = row_keys.into_iter().collect();
    let mut prev_keys: Vec<String> = vec![];

    // Build prefix+col accumulators for correct subtotals when column fields exist
    let mut prefix_col_subs: BTreeMap<(Vec<String>, Key, usize), Accum> = BTreeMap::new();
    if spec.subtotals.row_subtotals && !spec.columns.is_empty() && row_header_cols > 1 {
        for r in 0..table.row_count() {
            if !passes_filters(table, r, &spec.filters, &idx) { continue; }
            let rk_vals: Vec<String> = if row_fields.is_empty() { vec!["(All)".to_string()] } else { row_fields.iter().map(|&ci| cell_text(table, r, ci)).collect() };
            let ck = Key(if col_fields.is_empty() { vec![] } else { col_fields.iter().map(|&ci| cell_text(table, r, ci)).collect() });
            for level in 0..row_header_cols.saturating_sub(1) {
                if level >= rk_vals.len() { continue; }
                let prefix = rk_vals[..=level].to_vec();
                for (vi, (ci, vf)) in value_fields.iter().enumerate() {
                    let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
                    prefix_col_subs.entry((prefix.clone(), ck.clone(), vi)).or_default().push(&v, vf.aggregate);
                }
            }
        }
    }

    // Track per-prefix overall (no col split) for non-column case
    let mut prefix_subs: BTreeMap<Vec<String>, Vec<Accum>> = BTreeMap::new();
    if spec.subtotals.row_subtotals && row_header_cols > 1 {
        for r in 0..table.row_count() {
            if !passes_filters(table, r, &spec.filters, &idx) { continue; }
            let rk_vals: Vec<String> = if row_fields.is_empty() { vec!["(All)".to_string()] } else { row_fields.iter().map(|&ci| cell_text(table, r, ci)).collect() };
            for level in 0..row_header_cols.saturating_sub(1) {
                if level >= rk_vals.len() { continue; }
                let prefix = rk_vals[..=level].to_vec();
                let accs = prefix_subs.entry(prefix).or_insert_with(|| (0..value_fields.len()).map(|_| Accum::default()).collect());
                for (vi, (ci, vf)) in value_fields.iter().enumerate() {
                    let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
                    accs[vi].push(&v, vf.aggregate);
                }
            }
        }
    }

    for (ri, rk) in row_key_vec.iter().enumerate() {
        if spec.subtotals.row_subtotals && row_header_cols > 1 && ri > 0 {
            // find deepest changed level
            let cur = &rk.0;
            let mut change_level = None;
            for level in 0..row_header_cols.saturating_sub(1).min(prev_keys.len()).min(cur.len()) {
                if prev_keys.get(level) != cur.get(level) {
                    change_level = Some(level);
                    break;
                }
            }
            if let Some(cl) = change_level {
                // emit subtotals from deepest to cl
                for level in (cl..row_header_cols.saturating_sub(1)).rev() {
                    if level >= prev_keys.len() { continue; }
                    let prefix = prev_keys[..=level].to_vec();
                    let label = format!("{} Subtotal", prev_keys[level]);
                    let mut row = Vec::new();
                    for i in 0..row_header_cols {
                        if i == 0 { row.push(Cell::new(label.clone(), CellRole::Subtotal).indented(level)); }
                        else { row.push(Cell::new("", CellRole::Subtotal)); }
                    }
                    if value_fields.is_empty() {
                        row.push(Cell::new("", CellRole::Subtotal));
                    } else if spec.columns.is_empty() {
                        let accs = prefix_subs.get(&prefix);
                        for (vi, (_, vf)) in value_fields.iter().enumerate() {
                            let text = accs.and_then(|a| a.get(vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default();
                            row.push(Cell::new(text, CellRole::Subtotal));
                        }
                    } else {
                        for ck in &col_key_vec {
                            for (vi, (_, vf)) in value_fields.iter().enumerate() {
                                let text = prefix_col_subs.get(&(prefix.clone(), ck.clone(), vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default();
                                row.push(Cell::new(text, CellRole::Subtotal));
                            }
                        }
                    }
                    body_rows.push(row);
                }
            }
        }

        let mut row = Vec::new();
        for i in 0..row_header_cols {
            let val = rk.display_part(i).to_string();
            // suppress repeated labels
            let show = prev_keys.get(i).map(|p| p != &val).unwrap_or(true) || i == row_header_cols - 1;
            row.push(Cell::new(if show { val } else { String::new() }, CellRole::RowHeader).indented(i));
        }
        if value_fields.is_empty() {
            row.push(Cell::new("", CellRole::Body));
        } else {
            for ck in &col_key_vec {
                for (vi, (_, vf)) in value_fields.iter().enumerate() {
                    let text = cells
                        .get(&(rk.clone(), ck.clone(), vi))
                        .map(|a| a.finish(vf.aggregate).as_display())
                        .unwrap_or_default();
                    row.push(Cell::new(text, CellRole::Body));
                }
            }
        }
        body_rows.push(row);
        prev_keys = rk.0.clone();
    }
    // trailing subtotals
    if spec.subtotals.row_subtotals && row_header_cols > 1 && !prev_keys.is_empty() {
        for level in (0..row_header_cols.saturating_sub(1)).rev() {
            if level >= prev_keys.len() { continue; }
            let prefix = prev_keys[..=level].to_vec();
            let label = format!("{} Subtotal", prev_keys[level]);
            let mut row = Vec::new();
            for i in 0..row_header_cols {
                if i == 0 { row.push(Cell::new(label.clone(), CellRole::Subtotal).indented(level)); }
                else { row.push(Cell::new("", CellRole::Subtotal)); }
            }
            if value_fields.is_empty() {
                row.push(Cell::new("", CellRole::Subtotal));
            } else if spec.columns.is_empty() {
                let accs = prefix_subs.get(&prefix);
                for (vi, (_, vf)) in value_fields.iter().enumerate() {
                    let text = accs.and_then(|a| a.get(vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default();
                    row.push(Cell::new(text, CellRole::Subtotal));
                }
            } else {
                for ck in &col_key_vec {
                    for (vi, (_, vf)) in value_fields.iter().enumerate() {
                        let text = prefix_col_subs.get(&(prefix.clone(), ck.clone(), vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default();
                        row.push(Cell::new(text, CellRole::Subtotal));
                    }
                }
            }
            body_rows.push(row);
        }
    }

    // Footer bands (AC3.2): ordered, configurable; legacy default = Grand Total? + Notes.
    let mut footer_bands: Vec<FooterBand> = Vec::new();
    for band_spec in spec.effective_footer_bands() {
        match band_spec.kind {
            FooterBandKind::Notes => {
                let mut frow = Vec::new();
                let src = table.source.clone().unwrap_or_else(|| "(in-memory)".to_string());
                let filter_summary = if spec.filters.is_empty() {
                    "none".to_string()
                } else {
                    spec.filters.iter().map(|f| format!("{}:{:?}", f.field.name, f.op)).collect::<Vec<_>>().join(", ")
                };
                let note = format!("Source: {} | Rows: {} | Filters: {}", src, table.row_count(), filter_summary);
                frow.push(Cell::new(note, CellRole::Footer));
                footer_bands.push(FooterBand { kind: FooterBandKind::Notes, label: band_spec.effective_label(), cells: frow, pinned: false });
            }
            FooterBandKind::GrandTotal | FooterBandKind::Average | FooterBandKind::Count | FooterBandKind::Custom | FooterBandKind::Subtotal => {
                if value_fields.is_empty() { continue; }
                let agg = band_spec.aggregate.unwrap_or(match band_spec.kind {
                    FooterBandKind::Average => Aggregate::Average,
                    FooterBandKind::Count => Aggregate::Count,
                    _ => Aggregate::Sum,
                });
                // choose which value indices to compute with this aggregate: explicit field or all values (using band agg for non-grand/custom if spec'd)
                let use_band_agg = matches!(band_spec.kind, FooterBandKind::Average | FooterBandKind::Count | FooterBandKind::Custom) || band_spec.aggregate.is_some();
                let mut frow = Vec::new();
                let role = match band_spec.kind {
                    FooterBandKind::GrandTotal => CellRole::GrandTotal,
                    _ => CellRole::Footer,
                };
                frow.push(Cell::new(band_spec.effective_label(), role.clone()));
                for _ in 1..row_header_cols { frow.push(Cell::new("", role.clone())); }
                if spec.columns.is_empty() {
                    for (vi, (ci, vf)) in value_fields.iter().enumerate() {
                        let a = if use_band_agg && band_spec.field.as_deref().map(|n| n == vf.field.name).unwrap_or(band_spec.field.is_none()) { agg } else { vf.aggregate };
                        if a == Aggregate::Average && use_band_agg && band_spec.aggregate.is_none() && band_spec.field.is_none() && matches!(band_spec.kind, FooterBandKind::Average) {
                            let mut sum = 0.0_f64; let mut n = 0_i64;
                            let empty_ck = Key(vec![]);
                            for rk in &row_key_vec {
                                if let Some(acc) = cells.get(&(rk.clone(), empty_ck.clone(), vi)) {
                                    let v = acc.finish(vf.aggregate);
                                    if let Some(num) = numeric(&v) { sum += num; n += 1; }
                                }
                            }
                            let text = if n > 0 { numeric_value(sum / n as f64).as_display() } else { String::new() };
                            frow.push(Cell::new(text, role.clone()));
                        } else {
                            let mut acc = Accum::default();
                            for r in 0..table.row_count() {
                                if !passes_filters(table, r, &spec.filters, &idx) { continue; }
                                let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
                                acc.push(&v, a);
                            }
                            frow.push(Cell::new(acc.finish(a).as_display(), role.clone()));
                        }
                    }
                } else {
                    // per column key
                    for ck in &col_key_vec {
                        for (vi, (ci, vf)) in value_fields.iter().enumerate() {
                            let a = if use_band_agg && band_spec.field.as_deref().map(|n| n == vf.field.name).unwrap_or(band_spec.field.is_none()) { agg } else { vf.aggregate };
                            if a == Aggregate::Average && use_band_agg && band_spec.aggregate.is_none() && band_spec.field.is_none() && matches!(band_spec.kind, FooterBandKind::Average) {
                                // Average of displayed/calculated pivot values in this column (body rows), excluding subtotals/grand totals.
                                let mut sum = 0.0_f64; let mut n = 0_i64;
                                for rk in &row_key_vec {
                                    if let Some(acc) = cells.get(&(rk.clone(), ck.clone(), vi)) {
                                        let v = acc.finish(vf.aggregate);
                                        if let Some(num) = numeric(&v) { sum += num; n += 1; }
                                    }
                                }
                                let text = if n > 0 { numeric_value(sum / n as f64).as_display() } else { String::new() };
                                frow.push(Cell::new(text, role.clone()));
                            } else {
                                let mut acc = Accum::default();
                                for r in 0..table.row_count() {
                                    if !passes_filters(table, r, &spec.filters, &idx) { continue; }
                                    let rck = Key(if col_fields.is_empty() { vec![] } else { col_fields.iter().map(|&cci| cell_text(table, r, cci)).collect() });
                                    if &rck != ck { continue; }
                                    let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
                                    acc.push(&v, a);
                                }
                                frow.push(Cell::new(acc.finish(a).as_display(), role.clone()));
                            }
                        }
                    }
                }
                footer_bands.push(FooterBand { kind: band_spec.kind, label: band_spec.effective_label(), cells: frow, pinned: false });
            }
        }
    }

    let auto_title = auto_title(spec);
    let title = spec.title_override.clone().unwrap_or(auto_title);
    let title_pinned = spec.title_override.is_some();

    ThemedTableModel {
        title,
        title_pinned,
        row_header_cols,
        header_rows,
        body_rows,
        footer_bands,
    }
}

fn auto_title(spec: &PivotSpec) -> String {
    let vals = if spec.values.is_empty() {
        "Values".to_string()
    } else {
        spec.values.iter().map(|v| v.effective_name()).collect::<Vec<_>>().join(", ")
    };
    let rows = if spec.rows.is_empty() { "All".to_string() } else { spec.rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", ") };
    let cols = if spec.columns.is_empty() { String::new() } else { format!(" by {}", spec.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", ")) };
    format!("{vals} by {rows}{cols}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Aggregate, FieldRef, Filter, FilterOp, PivotSpec, SubtotalConfig, ValueField};
    use crate::{Column, DataTable, DataType, Value};

    fn sample_table() -> DataTable {
        DataTable {
            columns: vec![
                Column { name: "Region".into(), inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Product".into(), inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Sales".into(), inferred: DataType::Integer, override_type: None, nullable: false },
                Column { name: "Qty".into(), inferred: DataType::Integer, override_type: None, nullable: true },
            ],
            rows: vec![
                vec![Value::String("East".into()), Value::String("A".into()), Value::Integer(100), Value::Integer(1)],
                vec![Value::String("East".into()), Value::String("B".into()), Value::Integer(150), Value::Integer(2)],
                vec![Value::String("West".into()), Value::String("A".into()), Value::Integer(200), Value::Null],
                vec![Value::String("West".into()), Value::String("B".into()), Value::Integer(50), Value::Integer(5)],
            ],
            source: Some("test".into()),
        }
    }

    fn body_text(model: &ThemedTableModel) -> Vec<Vec<String>> {
        model.body_rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect()
    }

    #[test]
    fn ac2_1a_single_row_sum() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: true },
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        assert!(b.iter().any(|r| r.contains(&"East".to_string()) && r.contains(&"250".to_string())));
        assert!(b.iter().any(|r| r.contains(&"West".to_string()) && r.contains(&"250".to_string())));
        let gt = m.footer_bands.iter().find(|f| matches!(f.kind, FooterBandKind::GrandTotal)).unwrap();
        assert!(gt.cells.iter().any(|c| c.text == "500"));
    }

    #[test]
    fn ac2_1b_row_column_pivot() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            columns: vec![FieldRef::new("Product")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: false },
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        let east = b.iter().find(|r| r.first().map(|s| s.as_str()) == Some("East")).unwrap();
        assert_eq!(east, &vec!["East".to_string(), "100".to_string(), "150".to_string()]);
        let west = b.iter().find(|r| r.first().map(|s| s.as_str()) == Some("West")).unwrap();
        assert_eq!(west, &vec!["West".to_string(), "200".to_string(), "50".to_string()]);
    }

    #[test]
    fn ac2_1c_multiple_values() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![
                ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None },
                ValueField { field: FieldRef::new("Qty"), aggregate: Aggregate::Count, display_name: None },
            ],
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: false },
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        let east = b.iter().find(|r| r.first().map(|s| s.as_str()) == Some("East")).unwrap();
        assert_eq!(east, &vec!["East".to_string(), "250".to_string(), "2".to_string()]);
        let west = b.iter().find(|r| r.first().map(|s| s.as_str()) == Some("West")).unwrap();
        assert_eq!(west, &vec!["West".to_string(), "250".to_string(), "2".to_string()]);
    }

    #[test]
    fn ac2_1d_filter() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            filters: vec![Filter { field: FieldRef::new("Product"), op: FilterOp::Equals, values: vec![Value::String("A".into())] }],
            subtotals: SubtotalConfig::default(),
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        assert!(b.iter().any(|r| r.contains(&"East".to_string()) && r.contains(&"100".to_string())));
        assert!(b.iter().any(|r| r.contains(&"West".to_string()) && r.contains(&"200".to_string())));
    }

    #[test]
    fn ac2_1e_aggregates_and_nulls() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![
                ValueField { field: FieldRef::new("Qty"), aggregate: Aggregate::Count, display_name: None },
                ValueField { field: FieldRef::new("Qty"), aggregate: Aggregate::Average, display_name: None },
                ValueField { field: FieldRef::new("Qty"), aggregate: Aggregate::Min, display_name: None },
                ValueField { field: FieldRef::new("Qty"), aggregate: Aggregate::Max, display_name: None },
            ],
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        // West has Qty 5 and Null: Count=2 (rows), Avg=5 (ignores null), Min=5, Max=5
        let west = b.iter().find(|r| r.first().map(|s| s.as_str()) == Some("West")).unwrap();
        assert_eq!(west[1], "2");
        assert_eq!(west[2], "5");
        assert_eq!(west[3], "5");
        assert_eq!(west[4], "5");
    }

    #[test]
    fn ac2_1e_count_distinct() {
        let mut t = sample_table();
        t.rows.push(vec![Value::String("East".into()), Value::String("A".into()), Value::Integer(10), Value::Integer(1)]);
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Product"), aggregate: Aggregate::CountDistinct, display_name: None }],
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        let east = b.iter().find(|r| r.first().map(|s| s.as_str()) == Some("East")).unwrap();
        assert_eq!(east[1], "2");
    }

    #[test]
    fn ac3_1_stub_head_and_3_3_title() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            stub_head: Some("Region / Metric".into()),
            ..Default::default()
        };
        let m = compute(&t, &spec);
        assert!(m.title.contains("Sum of Sales"));
        assert!(!m.title_pinned);
        assert_eq!(m.header_rows[0][0].text, "Region / Metric");
        assert_eq!(m.header_rows[0][0].role, CellRole::StubHead);

        let spec2 = PivotSpec { title_override: Some("Pinned".into()), ..spec };
        let m2 = compute(&t, &spec2);
        assert_eq!(m2.title, "Pinned");
        assert!(m2.title_pinned);
    }

    #[test]
    fn ac2_4_50k_recompute_budget() {
        use std::time::Instant;
        let mut rows = Vec::with_capacity(50_000);
        let regions = ["East", "West", "North", "South"];
        let products = ["A", "B", "C", "D", "E"];
        for i in 0..50_000i64 {
            rows.push(vec![
                Value::String(regions[(i as usize) % regions.len()].to_string()),
                Value::String(products[(i as usize) % products.len()].to_string()),
                Value::Integer((i % 1000) + 1),
                Value::Integer((i % 10) + 1),
            ]);
        }
        let t = DataTable {
            columns: vec![
                Column { name: "Region".into(), inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Product".into(), inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Sales".into(), inferred: DataType::Integer, override_type: None, nullable: false },
                Column { name: "Qty".into(), inferred: DataType::Integer, override_type: None, nullable: false },
            ],
            rows,
            source: Some("synthetic-50k".into()),
        };
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            columns: vec![FieldRef::new("Product")],
            values: vec![
                ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None },
                ValueField { field: FieldRef::new("Qty"), aggregate: Aggregate::Count, display_name: None },
            ],
            subtotals: SubtotalConfig { row_subtotals: true, grand_total: true },
            ..Default::default()
        };
        let start = Instant::now();
        let m = compute(&t, &spec);
        let elapsed = start.elapsed();
        eprintln!("AC2.4 50k recompute: {:?} (body rows={}, footer bands={})", elapsed, m.body_rows.len(), m.footer_bands.len());
        assert!(!m.body_rows.is_empty());
        assert!(m.footer_bands.iter().any(|f| matches!(f.kind, FooterBandKind::GrandTotal)));
        // Debug budget: generous 10s to avoid flakiness; record in plan. Release target < few seconds.
        assert!(elapsed.as_secs() < 10, "50k pivot recompute took too long: {:?}", elapsed);
    }

    #[test]
    fn ac2_2_multi_level_subtotals() {
        let t = DataTable {
            columns: vec![
                Column { name: "Region".into(), inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "City".into(), inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Sales".into(), inferred: DataType::Integer, override_type: None, nullable: false },
            ],
            rows: vec![
                vec![Value::String("East".into()), Value::String("NY".into()), Value::Integer(100)],
                vec![Value::String("East".into()), Value::String("NY".into()), Value::Integer(50)],
                vec![Value::String("East".into()), Value::String("Boston".into()), Value::Integer(70)],
                vec![Value::String("West".into()), Value::String("LA".into()), Value::Integer(200)],
            ],
            source: Some("test".into()),
        };
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region"), FieldRef::new("City")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { row_subtotals: true, grand_total: true },
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b = body_text(&m);
        // Detail rows (row-header suppression applies; row-key order is engine-defined/sorted)
        assert!(b.iter().any(|r| r[1] == "NY" && r[2] == "150"), "NY row missing: {:?}", b);
        assert!(b.iter().any(|r| r[1] == "Boston" && r[2] == "70"));
        assert!(b.iter().any(|r| r[1] == "LA" && r[2] == "200"));
        // Level-1 subtotal rows label "<prefix> Subtotal" in first row-header col
        assert!(b.iter().any(|r| r[0] == "East Subtotal" && r[2] == "220"), "East subtotal missing: {:?}", b);
        assert!(b.iter().any(|r| r[0] == "West Subtotal" && r[2] == "200"), "West subtotal missing: {:?}", b);
        // Grand total = 420
        let gt = m.footer_bands.iter().find(|f| matches!(f.kind, FooterBandKind::GrandTotal)).unwrap();
        assert!(gt.cells.iter().any(|c| c.text == "420"));
    }

    #[test]
    fn ac3_2_footer_bands() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { grand_total: true, row_subtotals: false },
            ..Default::default()
        };
        let m = compute(&t, &spec);
        assert!(m.footer_bands.iter().any(|f| matches!(f.kind, FooterBandKind::GrandTotal)));
        assert!(m.footer_bands.iter().any(|f| matches!(f.kind, FooterBandKind::Notes)));
        assert!(m.footer_bands.len() >= 2);
    }

    #[test]
    fn ac3_2_configurable_multi_footer() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { grand_total: false, row_subtotals: false },
            footer_bands: vec![
                crate::spec::FooterBandSpec::grand_total(),
                crate::spec::FooterBandSpec::average(),
                crate::spec::FooterBandSpec::notes(),
            ],
            ..Default::default()
        };
        let m = compute(&t, &spec);
        assert_eq!(m.footer_bands.len(), 3);
        assert!(matches!(m.footer_bands[0].kind, FooterBandKind::GrandTotal));
        assert!(matches!(m.footer_bands[1].kind, FooterBandKind::Average));
        assert!(matches!(m.footer_bands[2].kind, FooterBandKind::Notes));
        assert!(m.footer_bands[0].cells.iter().any(|c| c.text == "500"));
        // Average of all filtered Sales rows: (100+150+200+50)/4 = 125
        assert!(m.footer_bands[1].cells.iter().any(|c| c.text == "125"));
        let notes = &m.footer_bands[2].cells[0].text;
        assert!(notes.contains("Rows: 4"));
        assert!(notes.contains("Filters: none"));
    }

    #[test]
    fn ac3_2_footer_reorder_and_filter_affects_values() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            filters: vec![Filter { field: FieldRef::new("Product"), op: FilterOp::Equals, values: vec![Value::String("A".into())] }],
            subtotals: SubtotalConfig::default(),
            footer_bands: vec![
                crate::spec::FooterBandSpec::notes(),
                crate::spec::FooterBandSpec::grand_total(),
            ],
            ..Default::default()
        };
        let m = compute(&t, &spec);
        assert!(matches!(m.footer_bands[0].kind, FooterBandKind::Notes));
        assert!(matches!(m.footer_bands[1].kind, FooterBandKind::GrandTotal));
        assert!(m.footer_bands[1].cells.iter().any(|c| c.text == "300"));
        assert!(m.footer_bands[0].cells[0].text.contains("Filters: Product:Equals"));
    }
}
