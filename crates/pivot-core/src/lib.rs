//! Pivot spec, computation engine, and the canonical themed table model.
//! UI-agnostic; data model types are shared by ingest/UI/pivot phases.

pub mod model;
pub mod pivot;
pub mod spec;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use model::{Cell, CellRole, FooterBand, FooterBandKind, ThemedTableModel};
pub use spec::{Aggregate, FieldRef, Filter, FilterOp, FooterBandSpec, PivotSpec, SubtotalConfig, ValueField};

/// Inferred/declared column data type (Phase 1, AC1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    #[default]
    String,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
}

/// A single cell value (nullable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "t", content = "v", rename_all = "snake_case")]
pub enum Value {
    #[default]
    Null,
    String(String),
    Integer(i64),
    Decimal(f64),
    Boolean(bool),
    /// ISO-8601 date `YYYY-MM-DD` (no timezone).
    Date(String),
    /// ISO-8601 datetime string.
    DateTime(String),
}

impl Value {
    pub fn as_display(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            Value::Integer(n) => n.to_string(),
            Value::Decimal(n) => {
                if n.is_finite() {
                    let s = format!("{n}");
                    s
                } else {
                    String::new()
                }
            }
            Value::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Date(s) | Value::DateTime(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub inferred: DataType,
    /// User override; when `None`, `inferred` is the effective type.
    pub override_type: Option<DataType>,
    pub nullable: bool,
}

impl Column {
    pub fn effective_type(&self) -> DataType {
        self.override_type.unwrap_or(self.inferred)
    }
}

/// A simple columnar-ish row table (tens of thousands of rows target).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DataTable {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    /// Optional source label (path/sheet/table).
    pub source: Option<String>,
}

impl DataTable {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn get(&self, row: usize, col: usize) -> Option<&Value> {
        self.rows.get(row).and_then(|r| r.get(col))
    }

    /// Reinterpret all cells of `col_idx` according to that column's effective type.
    /// Parsing failures become `Null` for nullable columns or keep the display string
    /// as `String` when the target type is `String`; non-parseable typed values become `Null`.
    pub fn reinterpret_column(&mut self, col_idx: usize) {
        let Some(col) = self.columns.get(col_idx).cloned() else {
            return;
        };
        let target = col.effective_type();
        for row in &mut self.rows {
            if let Some(cell) = row.get_mut(col_idx) {
                *cell = coerce_value(cell, target);
            }
        }
        if let Some(col) = self.columns.get_mut(col_idx) {
            col.inferred = target;
            col.override_type = None;
        }
    }

    pub fn schema_summary(&self) -> BTreeMap<String, String> {
        self.columns
            .iter()
            .map(|c| (c.name.clone(), format!("{:?}", c.effective_type())))
            .collect()
    }
}

pub fn coerce_value(value: &Value, target: DataType) -> Value {
    if matches!(value, Value::Null) {
        return Value::Null;
    }
    let s = value.as_display();
    match target {
        DataType::String => Value::String(s),
        DataType::Integer => {
            if let Value::Integer(n) = value {
                Value::Integer(*n)
            } else if let Value::Decimal(n) = value {
                if n.is_finite() && n.fract() == 0.0 {
                    Value::Integer(*n as i64)
                } else {
                    Value::Null
                }
            } else {
                let t = s.trim().replace(',', "");
                t.parse::<i64>().map(Value::Integer).unwrap_or(Value::Null)
            }
        }
        DataType::Decimal => {
            if let Value::Decimal(n) = value {
                Value::Decimal(*n)
            } else if let Value::Integer(n) = value {
                Value::Decimal(*n as f64)
            } else {
                let t = s.trim().replace(',', "");
                t.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(Value::Decimal)
                    .unwrap_or(Value::Null)
            }
        }
        DataType::Boolean => {
            if let Value::Boolean(b) = value {
                Value::Boolean(*b)
            } else {
                match s.trim().to_ascii_lowercase().as_str() {
                    "true" | "t" | "1" | "yes" | "y" => Value::Boolean(true),
                    "false" | "f" | "0" | "no" | "n" => Value::Boolean(false),
                    _ => Value::Null,
                }
            }
        }
        DataType::Date => {
            let t = s.trim();
            if looks_like_date(t) {
                Value::Date(t.to_string())
            } else {
                Value::Null
            }
        }
        DataType::DateTime => {
            let t = s.trim();
            if looks_like_datetime(t) || looks_like_date(t) {
                Value::DateTime(t.to_string())
            } else {
                Value::Null
            }
        }
    }
}

fn looks_like_date(s: &str) -> bool {
    // Minimal deterministic check: YYYY-MM-DD or YYYY/MM/DD
    let b = s.as_bytes();
    (b.len() == 10)
        && b[4] == b'-' as u8
        && b[7] == b'-' as u8
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        || (b.len() == 10
            && b[4] == b'/' as u8
            && b[7] == b'/' as u8
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()))
}

fn looks_like_datetime(s: &str) -> bool {
    // Minimal deterministic check: has date prefix and time separator.
    s.len() >= 16 && (s.contains('T') || (s.len() >= 11 && s.as_bytes().get(10) == Some(&b' ')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_and_override_reinterpret() {
        let mut table = DataTable {
            columns: vec![Column {
                name: "n".to_string(),
                inferred: DataType::String,
                override_type: Some(DataType::Integer),
                nullable: true,
            }],
            rows: vec![
                vec![Value::String("10".to_string())],
                vec![Value::String("x".to_string())],
                vec![Value::Null],
            ],
            source: None,
        };
        table.reinterpret_column(0);
        assert_eq!(table.get(0, 0), Some(&Value::Integer(10)));
        assert_eq!(table.get(1, 0), Some(&Value::Null));
        assert_eq!(table.get(2, 0), Some(&Value::Null));
        assert_eq!(table.columns[0].effective_type(), DataType::Integer);
    }
}
