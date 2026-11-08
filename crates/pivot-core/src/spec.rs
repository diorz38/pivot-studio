//! Pivot specification: field assignments, aggregates, and filters (Phase 2).

use crate::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef {
    /// Column name in the source `DataTable`.
    pub name: String,
}

impl FieldRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregate {
    Sum,
    Count,
    CountDistinct,
    Average,
    Min,
    Max,
}

impl Aggregate {
    pub fn label(self) -> &'static str {
        match self {
            Aggregate::Sum => "Sum",
            Aggregate::Count => "Count",
            Aggregate::CountDistinct => "Count Distinct",
            Aggregate::Average => "Average",
            Aggregate::Min => "Min",
            Aggregate::Max => "Max",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueField {
    pub field: FieldRef,
    pub aggregate: Aggregate,
    /// Display name/label override; `None` => auto (e.g. "Sum of Sales").
    pub display_name: Option<String>,
}

impl ValueField {
    pub fn auto_name(&self) -> String {
        format!("{} of {}", self.aggregate.label(), self.field.name)
    }
    pub fn effective_name(&self) -> String {
        self.display_name.clone().unwrap_or_else(|| self.auto_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    #[default]
    Include,
    Exclude,
    Equals,
    NotEquals,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub field: FieldRef,
    pub op: FilterOp,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SubtotalConfig {
    pub row_subtotals: bool,
    pub grand_total: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PivotSpec {
    pub rows: Vec<FieldRef>,
    pub columns: Vec<FieldRef>,
    pub values: Vec<ValueField>,
    pub filters: Vec<Filter>,
    pub subtotals: SubtotalConfig,
    /// Polished: editable stub-head (header column 1 / corner cell). AC3.1.
    pub stub_head: Option<String>,
    /// Polished: user-pinned table title; `None` => auto-derived. AC3.3.
    pub title_override: Option<String>,
}
