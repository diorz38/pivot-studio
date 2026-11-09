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
    fn prefix(&self, level: usize) -> Vec<String> {
        let n = (level + 1).min(self.0.len());
        self.0[..n].to_vec()
    }
}

#[derive(Debug, Clone)]
enum ColSlot {
    Data(Key),
    /// Subtotal for a column-group prefix at level (0-based in column fields).
    Subtotal { level: usize, prefix: Vec<String>, label: String },
    /// Grand total across all pivot columns.
    GrandTotal,
}

fn col_subtotal_label(prefix: &str, lang: crate::spec::OutputLanguage) -> String {
    subtotal_label(prefix, lang)
}

fn build_col_slots(col_key_vec: &[Key], levels: usize, sub_flags: &[bool], lang: crate::spec::OutputLanguage, show_grand: bool) -> Vec<ColSlot> {
    if levels == 0 || col_key_vec.is_empty() {
        let mut slots = Vec::new();
        if show_grand { slots.push(ColSlot::GrandTotal); }
        return slots;
    }
    let mut slots = Vec::new();
    let mut emitted: BTreeSet<(usize, Vec<String>)> = BTreeSet::new();
    for i in 0..col_key_vec.len() {
        if i > 0 {
            for level in (0..levels).rev() {
                if !*sub_flags.get(level).unwrap_or(&false) { continue; }
                let prev_p = col_key_vec[i - 1].prefix(level);
                let cur_p = col_key_vec[i].prefix(level);
                if prev_p != cur_p {
                    let key = (level, prev_p.clone());
                    if !emitted.contains(&key) {
                        emitted.insert(key);
                        let plabel = prev_p.last().cloned().unwrap_or_default();
                        slots.push(ColSlot::Subtotal { level, prefix: prev_p, label: col_subtotal_label(&plabel, lang) });
                    }
                }
            }
        }
        slots.push(ColSlot::Data(col_key_vec[i].clone()));
    }
    let last = col_key_vec.len() - 1;
    for level in (0..levels).rev() {
        if !*sub_flags.get(level).unwrap_or(&false) { continue; }
        let p = col_key_vec[last].prefix(level);
        let key = (level, p.clone());
        if !emitted.contains(&key) {
            emitted.insert(key);
            let plabel = p.last().cloned().unwrap_or_default();
            slots.push(ColSlot::Subtotal { level, prefix: p, label: col_subtotal_label(&plabel, lang) });
        }
    }
    if show_grand {
        slots.push(ColSlot::GrandTotal);
    }
    slots
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
    fn merge(&mut self, other: &Accum) {
        self.sum += other.sum;
        self.count += other.count;
        self.non_null += other.non_null;
        self.min = match (self.min, other.min) { (Some(a), Some(b)) => Some(a.min(b)), (Some(a), None) => Some(a), (None, Some(b)) => Some(b), (None, None) => None };
        self.max = match (self.max, other.max) { (Some(a), Some(b)) => Some(a.max(b)), (Some(a), None) => Some(a), (None, Some(b)) => Some(b), (None, None) => None };
        self.distinct.extend(other.distinct.iter().cloned());
        self.saw_number |= other.saw_number;
    }

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

fn format_number(n: f64, decimals: Option<u8>) -> String {
    if !n.is_finite() {
        return String::new();
    }
    if let Some(d) = decimals {
        return format!("{:.*}", d as usize, n);
    }
    numeric_value(n).as_display()
}

fn cell_text(table: &DataTable, row: usize, col: usize) -> String {
    table.get(row, col).map(|v| v.as_display()).unwrap_or_default()
}

pub fn resolve_field_index(table: &DataTable, name: &str) -> Option<usize> {
    let k = name.trim();
    if k.is_empty() { return None; }
    table.columns.iter().position(|c| c.name == k)
        .or_else(|| table.columns.iter().position(|c| c.name.eq_ignore_ascii_case(k)))
        .or_else(|| table.columns.iter().position(|c| c.original_name.as_deref() == Some(k)))
        .or_else(|| table.columns.iter().position(|c| c.display_name.as_deref() == Some(k)))
        .or_else(|| {
            // Accept qualified refs (alias.col) and legacy unqualified refs against qualified columns.
            table.columns.iter().position(|c| {
                c.name.ends_with(&format!(".{k}")) || k.ends_with(&format!(".{}", c.name))
                    || c.original_name.as_deref().is_some_and(|o| format!(".{o}") == format!(".{k}") || c.name.ends_with(&format!(".{o}")) && o.eq_ignore_ascii_case(k.rsplit('.').next().unwrap_or(k)))
            })
        })
        .or_else(|| {
            let tail = k.rsplit('.').next().unwrap_or(k);
            table.columns.iter().position(|c| c.name == tail || c.name.ends_with(&format!(".{tail}")) || c.name.eq_ignore_ascii_case(tail))
        })
}

/// Canonical resolved column name for a (possibly legacy/qualified/original/display)
/// field reference. `None` when the reference cannot be resolved against the table.
/// Used to migrate persisted specs onto stable `alias.column` identity.
pub fn canonical_field_name(table: &DataTable, name: &str) -> Option<String> {
    resolve_field_index(table, name).map(|i| table.columns[i].name.clone())
}

fn passes_filters(table: &DataTable, row: usize, filters: &[Filter]) -> bool {
    for f in filters {
        let Some(ci) = resolve_field_index(table, &f.field.name) else { continue; };
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
    let row_fields: Vec<usize> = spec.rows.iter().filter_map(|f| resolve_field_index(table, &f.name)).collect();
    let col_fields: Vec<usize> = spec.columns.iter().filter_map(|f| resolve_field_index(table, &f.name)).collect();
    let value_fields: Vec<(usize, &ValueField)> = spec
        .values
        .iter()
        .filter_map(|vf| resolve_field_index(table, &vf.field.name).map(|i| (i, vf)))
        .collect();

    let mut col_keys: BTreeSet<Key> = BTreeSet::new();
    let mut row_keys: BTreeSet<Key> = BTreeSet::new();
    // (row_key, col_key, value_index) -> accumulator
    let mut cells: BTreeMap<(Key, Key, usize), Accum> = BTreeMap::new();
    let mut filtered_rows: usize = 0;

    for r in 0..table.row_count() {
        if !passes_filters(table, r, &spec.filters) {
            continue;
        }
        filtered_rows += 1;
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
    let lang = spec.output_language;
    let show_grand_col = spec.subtotals.column_totals && !spec.columns.is_empty() && !value_fields.is_empty();
    let col_sub_flags = spec.effective_column_subtotals();
    let col_slots: Vec<ColSlot> = if spec.columns.is_empty() {
        vec![ColSlot::Data(Key(vec![]))]
    } else {
        build_col_slots(&col_key_vec, spec.columns.len(), &col_sub_flags, lang, show_grand_col)
    };
    let grand_label = match lang {
        crate::spec::OutputLanguage::En => "Total".to_string(),
        crate::spec::OutputLanguage::Id => "Total".to_string(),
        crate::spec::OutputLanguage::IdEn => "Total".to_string(),
    };
    let total_cols = row_header_cols + col_slots.len() * value_count.max(1);

    let stub_head = spec
        .stub_head
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_stub_head(spec, lang));
    let mut header_rows: Vec<Vec<Cell>> = Vec::new();
    if spec.columns.is_empty() {
        // single header row
        let mut hr = Vec::new();
        hr.push(Cell::new(stub_head.clone(), CellRole::StubHead));
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
                hr.push(Cell::span(stub_head.clone(), CellRole::StubHead, row_header_cols, levels));
            }
            let mut i = 0;
            while i < col_slots.len() {
                match &col_slots[i] {
                    ColSlot::Data(k) => {
                        let label = k.display_part(level).to_string();
                        let mut span = 1usize;
                        while i + span < col_slots.len() {
                            if let (ColSlot::Data(a), ColSlot::Data(b)) = (&col_slots[i], &col_slots[i + span]) {
                                let same_prefix = (0..=level).all(|l| a.display_part(l) == b.display_part(l));
                                if same_prefix { span += 1; } else { break; }
                            } else { break; }
                        }
                        hr.push(Cell::span(label, CellRole::ColHeader, span * value_count.max(1), 1));
                        i += span;
                    }
                    ColSlot::Subtotal { level: sl, label, prefix } => {
                        let vspan = value_count.max(1);
                        if level == *sl {
                            hr.push(Cell::span(label.clone(), CellRole::ColHeader, vspan, 1));
                        } else if level < *sl {
                            // show group context above the subtotal slot
                            let ctx = prefix.get(level).cloned().unwrap_or_default();
                            hr.push(Cell::span(ctx, CellRole::ColHeader, vspan, 1));
                        } else {
                            hr.push(Cell::span("", CellRole::ColHeader, vspan, 1));
                        }
                        i += 1;
                    }
                    ColSlot::GrandTotal => {
                        let vspan = value_count.max(1);
                        if level == 0 {
                            hr.push(Cell::span(grand_label.clone(), CellRole::ColHeader, vspan, 1));
                        } else {
                            hr.push(Cell::span("", CellRole::ColHeader, vspan, 1));
                        }
                        i += 1;
                    }
                }
            }
            header_rows.push(hr);
        }
        if !effective_values.is_empty() && effective_values.len() > 1 {
            let mut hr = Vec::new();
            for _ in 0..row_header_cols { hr.push(Cell::new("", CellRole::RowHeader)); }
            for _slot in &col_slots {
                for name in &effective_values {
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
            if !passes_filters(table, r, &spec.filters) { continue; }
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
            if !passes_filters(table, r, &spec.filters) { continue; }
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
                    let label = subtotal_label(&prev_keys[level], lang);
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
                        for slot in &col_slots {
                            for (vi, (_, vf)) in value_fields.iter().enumerate() {
                                let text = match slot {
                                    ColSlot::Data(ck) => prefix_col_subs.get(&(prefix.clone(), ck.clone(), vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default(),
                                    ColSlot::Subtotal { prefix: cp, .. } => {
                                        let mut acc = Accum::default();
                                        for dck in &col_key_vec {
                                            if dck.0.len() >= cp.len() && dck.0[..cp.len()] == cp[..] {
                                                if let Some(a) = prefix_col_subs.get(&(prefix.clone(), dck.clone(), vi)) { acc.merge(a); }
                                            }
                                        }
                                        acc.finish(vf.aggregate).as_display()
                                    }
                                    ColSlot::GrandTotal => {
                                        let mut acc = Accum::default();
                                        for dck in &col_key_vec {
                                            if let Some(a) = prefix_col_subs.get(&(prefix.clone(), dck.clone(), vi)) { acc.merge(a); }
                                        }
                                        acc.finish(vf.aggregate).as_display()
                                    }
                                };
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
        } else if spec.columns.is_empty() {
            for slot in &col_slots {
                let _ = slot;
                for (vi, (_, vf)) in value_fields.iter().enumerate() {
                    let text = cells.get(&(rk.clone(), Key(vec![]), vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default();
                    row.push(Cell::new(text, CellRole::Body));
                }
            }
        } else {
            for slot in &col_slots {
                for (vi, (_, vf)) in value_fields.iter().enumerate() {
                    let text = match slot {
                        ColSlot::Data(ck) => cells.get(&(rk.clone(), ck.clone(), vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default(),
                        ColSlot::Subtotal { prefix: cp, .. } => {
                            let mut acc = Accum::default();
                            for dck in &col_key_vec {
                                if dck.0.len() >= cp.len() && dck.0[..cp.len()] == cp[..] {
                                    if let Some(a) = cells.get(&(rk.clone(), dck.clone(), vi)) { acc.merge(a); }
                                }
                            }
                            acc.finish(vf.aggregate).as_display()
                        }
                        ColSlot::GrandTotal => {
                            let mut acc = Accum::default();
                            for dck in &col_key_vec {
                                if let Some(a) = cells.get(&(rk.clone(), dck.clone(), vi)) { acc.merge(a); }
                            }
                            acc.finish(vf.aggregate).as_display()
                        }
                    };
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
            let label = subtotal_label(&prev_keys[level], lang);
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
                for slot in &col_slots {
                    for (vi, (_, vf)) in value_fields.iter().enumerate() {
                        let text = match slot {
                            ColSlot::Data(ck) => prefix_col_subs.get(&(prefix.clone(), ck.clone(), vi)).map(|a| a.finish(vf.aggregate).as_display()).unwrap_or_default(),
                            ColSlot::Subtotal { prefix: cp, .. } => {
                                let mut acc = Accum::default();
                                for dck in &col_key_vec {
                                    if dck.0.len() >= cp.len() && dck.0[..cp.len()] == cp[..] {
                                        if let Some(a) = prefix_col_subs.get(&(prefix.clone(), dck.clone(), vi)) { acc.merge(a); }
                                    }
                                }
                                acc.finish(vf.aggregate).as_display()
                            }
                            ColSlot::GrandTotal => {
                                let mut acc = Accum::default();
                                for dck in &col_key_vec {
                                    if let Some(a) = prefix_col_subs.get(&(prefix.clone(), dck.clone(), vi)) { acc.merge(a); }
                                }
                                acc.finish(vf.aggregate).as_display()
                            }
                        };
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
                let src = spec
                    .source_label
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| table.source.clone())
                    .unwrap_or_else(|| "(in-memory)".to_string());
                let filter_summary = if spec.filters.is_empty() {
                    String::new()
                } else {
                    spec.filters
                        .iter()
                        .map(|f| format_filter_readable(f, lang))
                        .collect::<Vec<_>>()
                        .join("; ")
                };
                for line in notes_lines(lang, &src, table.row_count(), filtered_rows, &filter_summary) {
                    footer_bands.push(FooterBand {
                        kind: FooterBandKind::Notes,
                        label: band_spec.effective_label_lang(lang),
                        cells: vec![Cell::span(line, CellRole::Footer, total_cols, 1)],
                        pinned: false,
                    });
                }
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
                frow.push(Cell::new(band_spec.effective_label_lang(lang), role));
                for _ in 1..row_header_cols { frow.push(Cell::new("", role)); }
                if spec.columns.is_empty() {
                    for (vi, (ci, vf)) in value_fields.iter().enumerate() {
                        if matches!(band_spec.kind, FooterBandKind::Average) && band_spec.field.is_none() {
                            let mut sum = 0.0_f64; let mut n = 0_i64;
                            let empty_ck = Key(vec![]);
                            for rk in &row_key_vec {
                                if let Some(acc) = cells.get(&(rk.clone(), empty_ck.clone(), vi)) {
                                    let v = acc.finish(vf.aggregate);
                                    let num = numeric(&v).or_else(|| v.as_display().trim().replace(',', "").parse::<f64>().ok());
                                    if let Some(num) = num { sum += num; n += 1; }
                                }
                            }
                            let text = if n > 0 { format_number(sum / n as f64, band_spec.decimals) } else { String::new() };
                            frow.push(Cell::new(text, role));
                        } else {
                            let a = if use_band_agg && band_spec.field.as_deref().map(|n| n == vf.field.name).unwrap_or(band_spec.field.is_none()) { agg } else { vf.aggregate };
                            let mut acc = Accum::default();
                            for r in 0..table.row_count() {
                                if !passes_filters(table, r, &spec.filters) { continue; }
                                let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
                                acc.push(&v, a);
                            }
                            let text = if let Some(d) = band_spec.decimals {
                                numeric(&acc.finish(a)).map(|n| format_number(n, Some(d))).unwrap_or_default()
                            } else {
                                acc.finish(a).as_display()
                            };
                            frow.push(Cell::new(text, role));
                        }
                    }
                } else {
                    // per column slot (data / per-level subtotal / grand total)
                    let data_col_matches = |dck: &Key, slot: &ColSlot| -> bool {
                        match slot {
                            ColSlot::Data(ck) => dck == ck,
                            ColSlot::Subtotal { prefix, .. } => dck.0.len() >= prefix.len() && dck.0[..prefix.len()] == prefix[..],
                            ColSlot::GrandTotal => true,
                        }
                    };
                    for slot in &col_slots {
                        for (vi, (ci, vf)) in value_fields.iter().enumerate() {
                            if matches!(band_spec.kind, FooterBandKind::Average) && band_spec.field.is_none() {
                                // Mean of displayed/calculated pivot values in this slot (detail body rows).
                                let mut sum = 0.0_f64; let mut n = 0_i64;
                                for rk in &row_key_vec {
                                    for dck in &col_key_vec {
                                        if !data_col_matches(dck, slot) { continue; }
                                        if let Some(acc) = cells.get(&(rk.clone(), dck.clone(), vi)) {
                                            let v = acc.finish(vf.aggregate);
                                            let num = numeric(&v).or_else(|| v.as_display().trim().replace(',', "").parse::<f64>().ok());
                                            if let Some(num) = num { sum += num; n += 1; }
                                        }
                                    }
                                }
                                let text = if n > 0 { format_number(sum / n as f64, band_spec.decimals) } else { String::new() };
                                frow.push(Cell::new(text, role));
                            } else {
                                let a = if use_band_agg && band_spec.field.as_deref().map(|n| n == vf.field.name).unwrap_or(band_spec.field.is_none()) { agg } else { vf.aggregate };
                                let mut acc = Accum::default();
                                for r in 0..table.row_count() {
                                    if !passes_filters(table, r, &spec.filters) { continue; }
                                    let rck = Key(if col_fields.is_empty() { vec![] } else { col_fields.iter().map(|&cci| cell_text(table, r, cci)).collect() });
                                    if !data_col_matches(&rck, slot) { continue; }
                                    let v = table.get(r, *ci).cloned().unwrap_or(Value::Null);
                                    acc.push(&v, a);
                                }
                                let text = if let Some(d) = band_spec.decimals {
                                    numeric(&acc.finish(a)).map(|n| format_number(n, Some(d))).unwrap_or_default()
                                } else {
                                    acc.finish(a).as_display()
                                };
                                frow.push(Cell::new(text, role));
                            }
                        }
                    }
                }
                footer_bands.push(FooterBand { kind: band_spec.kind, label: band_spec.effective_label_lang(lang), cells: frow, pinned: false });
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

fn humanize_name(name: &str) -> String {
    name.replace('_', " ").trim().to_string()
}

fn join_lang(lang: crate::spec::OutputLanguage, items: &[String]) -> String {
    use crate::spec::OutputLanguage::*;
    let and = match lang { Id => " dan ", _ => " and " };
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{}{}{}", items[0], and, items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, {}{}", rest.join(", "), and.trim_start(), last)
        }
    }
}

fn bi(lang: crate::spec::OutputLanguage, en: &str, id: &str) -> String {
    use crate::spec::OutputLanguage::*;
    match lang {
        En => en.to_string(),
        Id => id.to_string(),
        IdEn => format!("{en} / {id}"),
    }
}

fn value_phrase(v: &ValueField, lang: crate::spec::OutputLanguage) -> String {
    use crate::spec::OutputLanguage::*;
    let field = humanize_name(&v.field.name);
    match (v.aggregate, lang) {
        (Aggregate::Count | Aggregate::CountDistinct, En) => format!("Number of {field}"),
        (Aggregate::Count | Aggregate::CountDistinct, Id) => format!("Jumlah {field}"),
        (Aggregate::Count | Aggregate::CountDistinct, IdEn) => format!("Number of {field} / Jumlah {field}"),
        (Aggregate::Sum, En) => format!("Sum of {field}"),
        (Aggregate::Sum, Id) => format!("Total {field}"),
        (Aggregate::Sum, IdEn) => format!("Sum of {field} / Total {field}"),
        (Aggregate::Average, En) => format!("Average of {field}"),
        (Aggregate::Average, Id) => format!("Rata-rata {field}"),
        (Aggregate::Average, IdEn) => format!("Average of {field} / Rata-rata {field}"),
        (Aggregate::Min, En) => format!("Minimum of {field}"),
        (Aggregate::Min, Id) => format!("Minimum {field}"),
        (Aggregate::Min, IdEn) => format!("Minimum of {field} / Minimum {field}"),
        (Aggregate::Max, En) => format!("Maximum of {field}"),
        (Aggregate::Max, Id) => format!("Maksimum {field}"),
        (Aggregate::Max, IdEn) => format!("Maximum of {field} / Maksimum {field}"),
    }
}

fn auto_title(spec: &PivotSpec) -> String {
    let lang = spec.output_language;
    let vals = if spec.values.is_empty() {
        bi(lang, "Values", "Nilai")
    } else {
        join_lang(lang, &spec.values.iter().map(|v| value_phrase(v, lang)).collect::<Vec<_>>())
    };
    let cols = spec
        .columns
        .iter()
        .map(|c| humanize_name(&c.name))
        .collect::<Vec<_>>();
    let rows = spec
        .rows
        .iter()
        .map(|r| humanize_name(&r.name))
        .collect::<Vec<_>>();

    let mut out = vals;
    if !cols.is_empty() {
        out.push_str(&match lang {
            crate::spec::OutputLanguage::En => " according to ".to_string(),
            crate::spec::OutputLanguage::Id => " berdasarkan ".to_string(),
            crate::spec::OutputLanguage::IdEn => " according to / berdasarkan ".to_string(),
        });
        out.push_str(&join_lang(lang, &cols));
    }
    if !rows.is_empty() {
        // m01681: "menurut" -> "by" (EN); keep "menurut" for ID; bilingual shows both.
        out.push_str(&match lang {
            crate::spec::OutputLanguage::En => " by ".to_string(),
            crate::spec::OutputLanguage::Id => " menurut ".to_string(),
            crate::spec::OutputLanguage::IdEn => " by / menurut ".to_string(),
        });
        out.push_str(&join_lang(lang, &rows));
    } else if cols.is_empty() {
        out.push_str(&match lang {
            crate::spec::OutputLanguage::En => " (all data)".to_string(),
            crate::spec::OutputLanguage::Id => " (semua data)".to_string(),
            crate::spec::OutputLanguage::IdEn => " (all data / semua data)".to_string(),
        });
    }
    out
}

fn subtotal_label(prefix: &str, lang: crate::spec::OutputLanguage) -> String {
    use crate::spec::OutputLanguage::*;
    match lang {
        En => format!("{prefix} Subtotal"),
        Id => format!("Subtotal {prefix}"),
        IdEn => format!("{prefix} Subtotal"),
    }
}

fn default_stub_head(spec: &PivotSpec, lang: crate::spec::OutputLanguage) -> String {
    let names = spec
        .rows
        .iter()
        .map(|r| humanize_name(&r.name))
        .collect::<Vec<_>>();
    if names.is_empty() {
        bi(lang, "Row", "Baris")
    } else {
        join_lang(lang, &names)
    }
}

fn format_filter_readable(f: &crate::spec::Filter, lang: crate::spec::OutputLanguage) -> String {
    use crate::spec::{FilterOp, OutputLanguage::*};
    let vals = f.values.iter().map(|v| v.as_display()).collect::<Vec<_>>().join(", ");
    let (eq, neq, inc, exc) = match lang {
        En => ("=", "â‰ ", "in", "not in"),
        Id => ("=", "â‰ ", "termasuk", "tidak termasuk"),
        IdEn => ("=", "â‰ ", "in/termasuk", "not in/tidak termasuk"),
    };
    match f.op {
        FilterOp::Equals => format!("{} {} {}", f.field.name, eq, vals),
        FilterOp::NotEquals => format!("{} {} {}", f.field.name, neq, vals),
        FilterOp::Include => format!("{} {} [{}]", f.field.name, inc, vals),
        FilterOp::Exclude => format!("{} {} [{}]", f.field.name, exc, vals),
    }
}

fn notes_lines(
    lang: crate::spec::OutputLanguage,
    source: &str,
    rows: usize,
    filtered_rows: usize,
    filter_summary: &str,
) -> Vec<String> {
    use crate::spec::OutputLanguage::*;
    let none = match lang {
        En => "none",
        Id => "tidak ada",
        IdEn => "none / tidak ada",
    };
    let filters = if filter_summary.trim().is_empty() { none } else { filter_summary };
    match lang {
        En => vec![
            format!("Data source: {source}"),
            format!("Rows: {rows}"),
            format!("Filtered rows: {filtered_rows}"),
            format!("Filters: {filters}"),
        ],
        Id => vec![
            format!("Sumber data: {source}"),
            format!("Baris Data: {rows}"),
            format!("Baris Data terfilter: {filtered_rows}"),
            format!("Filter: {filters}"),
        ],
        IdEn => vec![
            format!("Data source / Sumber data: {source}"),
            format!("Rows / Baris Data: {rows}"),
            format!("Filtered rows / Baris Data terfilter: {filtered_rows}"),
            format!("Filters / Filter: {filters}"),
        ],
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Aggregate, FieldRef, Filter, FilterOp, PivotSpec, SubtotalConfig, ValueField};
    use crate::{Column, DataTable, DataType, Value};

    #[test]
    fn canonical_field_name_maps_legacy_refs_to_qualified_identity() {
        let table = DataTable {
            columns: vec![
                Column { name: "orders.order_id".into(), original_name: Some("Order ID".into()), display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
                Column { name: "customers.customer_name".into(), original_name: Some("Customer Name".into()), display_name: None, inferred: DataType::String, override_type: None, nullable: true },
            ],
            rows: vec![vec![Value::Integer(1), Value::String("Ann".into())]],
            source: None,
        };
        assert_eq!(canonical_field_name(&table, "order_id").as_deref(), Some("orders.order_id"));
        assert_eq!(canonical_field_name(&table, "Order ID").as_deref(), Some("orders.order_id"));
        assert_eq!(canonical_field_name(&table, "orders.order_id").as_deref(), Some("orders.order_id"));
        assert_eq!(canonical_field_name(&table, "customer_name").as_deref(), Some("customers.customer_name"));
        assert_eq!(canonical_field_name(&table, "nope"), None);
    }

    fn sample_table() -> DataTable {
        DataTable {
            columns: vec![
                Column { name: "Region".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Product".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Sales".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
                Column { name: "Qty".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: true },
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
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: true, column_totals: false },
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
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: false, column_totals: false },
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
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: false, column_totals: false },
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
                Column { name: "Region".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Product".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Sales".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
                Column { name: "Qty".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
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
            subtotals: SubtotalConfig { row_subtotals: true, grand_total: true, column_totals: false },
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
                Column { name: "Region".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "City".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "Sales".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
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
            subtotals: SubtotalConfig { row_subtotals: true, grand_total: true, column_totals: false },
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
            subtotals: SubtotalConfig { grand_total: true, row_subtotals: false, column_totals: false },
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
            subtotals: SubtotalConfig { grand_total: false, row_subtotals: false, column_totals: false },
            footer_bands: vec![
                crate::spec::FooterBandSpec::grand_total(),
                crate::spec::FooterBandSpec::average(),
                crate::spec::FooterBandSpec::notes(),
            ],
            output_language: crate::spec::OutputLanguage::En,
            ..Default::default()
        };
        let m = compute(&t, &spec);
        assert!(m.footer_bands.len() >= 6, "notes expands to 4 rows: {:?}", m.footer_bands.len());
        assert!(matches!(m.footer_bands[0].kind, FooterBandKind::GrandTotal));
        assert!(matches!(m.footer_bands[1].kind, FooterBandKind::Average));
        assert!(m.footer_bands.iter().skip(2).all(|b| matches!(b.kind, FooterBandKind::Notes)));
        assert!(m.footer_bands[0].cells.iter().any(|c| c.text == "500"));
        // Average of displayed pivot values per column (detail rows): (East 250 + West 250) / 2 = 125? no, East=100+150=250, West=200+50=250, mean=250
        assert!(m.footer_bands[1].cells.iter().any(|c| c.text == "250"));
        let notes: Vec<String> = m.footer_bands.iter().skip(2).map(|b| b.cells[0].text.clone()).collect();
        assert!(notes.iter().any(|s| s == "Rows: 4"), "notes: {:?}", notes);
        assert!(notes.iter().any(|s| s == "Filtered rows: 4"), "notes: {:?}", notes);
        assert!(notes.iter().any(|s| s == "Filters: none"), "notes: {:?}", notes);
        assert!(notes.iter().any(|s| s.starts_with("Data source:")), "notes: {:?}", notes);
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
            output_language: crate::spec::OutputLanguage::En,
            ..Default::default()
        };
        let m = compute(&t, &spec);
        assert!(matches!(m.footer_bands[0].kind, FooterBandKind::Notes));
        let gt_idx = m.footer_bands.iter().position(|b| matches!(b.kind, FooterBandKind::GrandTotal)).expect("grand total band");
        assert_eq!(gt_idx, 4, "notes spec expands to 4 rows before grand total");
        assert!(m.footer_bands[gt_idx].cells.iter().any(|c| c.text == "300"));
        let notes: String = m.footer_bands.iter().take(gt_idx).map(|b| b.cells[0].text.clone()).collect::<Vec<_>>().join("\n");
        assert!(notes.contains("Rows: 4"), "note: {notes}");
        assert!(notes.contains("Filtered rows: 2"), "note: {notes}");
        assert!(notes.contains("Filters:"), "note: {notes}");
        assert!(notes.contains("Product"), "note: {notes}");
        assert!(notes.contains("A"), "note: {notes}");
    }

    #[test]
    fn notes_respect_source_label_and_id_format() {
        let t = sample_table();
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig::default(),
            output_language: crate::spec::OutputLanguage::Id,
            source_label: Some(r"D:\Documents\Downloads\mitra.csv".to_string()),
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let notes: Vec<String> = m
            .footer_bands
            .iter()
            .filter(|b| matches!(b.kind, FooterBandKind::Notes))
            .map(|b| b.cells[0].text.clone())
            .collect();
        assert_eq!(notes[0], r"Sumber data: D:\Documents\Downloads\mitra.csv");
        assert_eq!(notes[1], "Baris Data: 4");
        assert_eq!(notes[2], "Baris Data terfilter: 4");
        assert_eq!(notes[3], "Filter: tidak ada");
    }
    #[test]
    fn ac3_2_average_on_column_count_values() {
        // Average footer should show mean of displayed/calculated pivot values per column.
        let t = DataTable {
            columns: vec![
                Column { name: "keca".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "jk".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
            ],
            rows: vec![
                vec![Value::String("A".into()), Value::String("L".into())],
                vec![Value::String("A".into()), Value::String("L".into())],
                vec![Value::String("A".into()), Value::String("P".into())],
                vec![Value::String("B".into()), Value::String("L".into())],
                vec![Value::String("B".into()), Value::String("P".into())],
                vec![Value::String("B".into()), Value::String("P".into())],
            ],
            source: Some("test".into()),
        };
        let spec = PivotSpec {
            rows: vec![FieldRef::new("keca")],
            columns: vec![FieldRef::new("jk")],
            values: vec![ValueField { field: FieldRef::new("keca"), aggregate: Aggregate::Count, display_name: None }],
            footer_bands: vec![crate::spec::FooterBandSpec::grand_total(), crate::spec::FooterBandSpec::average()],
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let avg = m.footer_bands.iter().find(|f| matches!(f.kind, FooterBandKind::Average)).unwrap();
        let texts: Vec<_> = avg.cells.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.iter().any(|t| *t == "1.5" || *t == "1.50" || *t == "1,5"), "avg row: {:?}", texts);
    }

    #[test]
    fn row_subtotals_can_be_hidden() {
        let t = DataTable {
            columns: vec![
                Column { name: "region".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "dept".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "amt".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
            ],
            rows: vec![
                vec![Value::String("East".into()), Value::String("A".into()), Value::Integer(100)],
                vec![Value::String("East".into()), Value::String("B".into()), Value::Integer(120)],
                vec![Value::String("West".into()), Value::String("A".into()), Value::Integer(200)],
            ],
            source: None,
        };
        let spec = PivotSpec {
            rows: vec![FieldRef { name: "region".into() }, FieldRef { name: "dept".into() }],
            values: vec![ValueField { field: FieldRef { name: "amt".into() }, aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: false, column_totals: false },
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let b: Vec<Vec<String>> = m.body_rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect();
        assert!(!b.iter().any(|r| r.iter().any(|c| c.contains("Subtotal"))), "subtotal rows should be hidden: {:?}", b);
        assert!(m.footer_bands.iter().all(|b| b.kind != FooterBandKind::GrandTotal));
    }

    #[test]
    fn column_totals_add_total_column_across_pivot_columns() {
        let t = DataTable {
            columns: vec![
                Column { name: "region".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "year".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
                Column { name: "sales".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
            ],
            rows: vec![
                vec![Value::String("East".into()), Value::Integer(2024), Value::Integer(100)],
                vec![Value::String("East".into()), Value::Integer(2025), Value::Integer(120)],
                vec![Value::String("West".into()), Value::Integer(2024), Value::Integer(200)],
                vec![Value::String("West".into()), Value::Integer(2025), Value::Integer(300)],
            ],
            source: None,
        };
        let spec = PivotSpec {
            rows: vec![FieldRef { name: "region".into() }],
            columns: vec![FieldRef { name: "year".into() }],
            values: vec![ValueField { field: FieldRef { name: "sales".into() }, aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { row_subtotals: true, grand_total: true, column_totals: true },
            footer_bands: vec![crate::spec::FooterBandSpec::grand_total()],
            ..Default::default()
        };
        let m = compute(&t, &spec);
        let header_texts: Vec<String> = m.header_rows.first().map(|r| r.iter().map(|c| c.text.clone()).collect()).unwrap_or_default();
        assert!(header_texts.iter().any(|t| t == "Total"), "missing Total column header: {:?}", header_texts);
        let b: Vec<Vec<String>> = m.body_rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect();
        let east = b.iter().find(|r| r.first().map(|s| s == "East").unwrap_or(false)).expect("East row");
        assert_eq!(east.last().map(|s| s.as_str()), Some("220"), "East total wrong: {:?}", east);
        let west = b.iter().find(|r| r.first().map(|s| s == "West").unwrap_or(false)).expect("West row");
        assert_eq!(west.last().map(|s| s.as_str()), Some("500"), "West total wrong: {:?}", west);
        let gt = m.footer_bands.iter().find(|b| b.kind == FooterBandKind::GrandTotal).expect("grand total");
        let gt_vals: Vec<String> = gt.cells.iter().map(|c| c.text.clone()).collect();
        assert_eq!(gt_vals.last().map(|s| s.as_str()), Some("720"), "grand total column wrong: {:?}", gt_vals);
    }
    #[test]
    fn column_level_subtotals_add_subtotal_columns_at_selected_level() {
        // 2 column levels (region -> quarter), 1 row, subtotal enabled at level 0 (region).
        let t = DataTable {
            columns: vec![
                Column { name: "product".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "region".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "quarter".into(), original_name: None, display_name: None, inferred: DataType::String, override_type: None, nullable: false },
                Column { name: "sales".into(), original_name: None, display_name: None, inferred: DataType::Integer, override_type: None, nullable: false },
            ],
            rows: vec![
                vec![Value::String("A".into()), Value::String("East".into()), Value::String("Q1".into()), Value::Integer(10)],
                vec![Value::String("A".into()), Value::String("East".into()), Value::String("Q2".into()), Value::Integer(20)],
                vec![Value::String("A".into()), Value::String("West".into()), Value::String("Q1".into()), Value::Integer(30)],
                vec![Value::String("A".into()), Value::String("West".into()), Value::String("Q2".into()), Value::Integer(40)],
                vec![Value::String("B".into()), Value::String("East".into()), Value::String("Q1".into()), Value::Integer(5)],
            ],
            source: None,
        };
        let spec = PivotSpec {
            rows: vec![FieldRef::new("product")],
            columns: vec![FieldRef::new("region"), FieldRef::new("quarter")],
            values: vec![ValueField { field: FieldRef::new("sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { row_subtotals: false, grand_total: false, column_totals: false },
            column_subtotals: vec![true, false], // subtotal per region
            output_language: crate::spec::OutputLanguage::En,
            ..Default::default()
        };
        let m = compute(&t, &spec);
        // Subtotal labels for the region level (level 0) appear in header row 0.
        let h0: Vec<String> = m.header_rows.first().map(|r| r.iter().map(|c| c.text.clone()).collect()).unwrap_or_default();
        assert!(h0.iter().any(|t| t == "East Subtotal"), "missing East Subtotal in header: {:?}", h0);
        assert!(h0.iter().any(|t| t == "West Subtotal"), "missing West Subtotal in header: {:?}", h0);
        // Quarter-level header (row 1) is empty above subtotal slots.
        let h1: Vec<String> = m.header_rows.get(1).map(|r| r.iter().map(|c| c.text.clone()).collect()).unwrap_or_default();
        assert!(h1.contains(&"Q1".to_string()) && h1.contains(&"Q2".to_string()), "missing quarter headers: {:?}", h1);
        // Product A: East Q1=10,Q2=20, East subtotal=30, West Q1=30,Q2=40, West subtotal=70.
        let b: Vec<Vec<String>> = m.body_rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect();
        let a = b.iter().find(|r| r.first().map(|s| s == "A").unwrap_or(false)).expect("row A");
        assert_eq!(a, &vec!["A".to_string(), "10".to_string(), "20".to_string(), "30".to_string(), "30".to_string(), "40".to_string(), "70".to_string()], "row A: {:?}", a);
        // B has only East data; East subtotal=5.
        let pb = b.iter().find(|r| r.first().map(|s| s == "B").unwrap_or(false)).expect("row B");
        assert_eq!(pb, &vec!["B".to_string(), "5".to_string(), "".to_string(), "5".to_string(), "".to_string(), "".to_string(), "".to_string()], "row B: {:?}", pb);
    }
}

