//! Pivot specification: field assignments, aggregates, and filters (Phase 2).

use crate::Value;
use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

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
    /// Legacy flag; when `PivotSpec::footer_bands` is empty, a Grand Total band is emitted if true.
    pub grand_total: bool,
}

/// Configures one footer band in the pivot output (AC3.2). Bands render in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FooterBandSpec {
    pub kind: crate::model::FooterBandKind,
    /// Label shown in the row-header/stub area (and used as band identity in UI).
    #[serde(default)]
    pub label: Option<String>,
    /// Aggregate used for GrandTotal/Average/Count/Sum/Min/Max/CountDistinct bands.
    /// Ignored for `Notes`. `None` => sensible default per kind.
    #[serde(default)]
    pub aggregate: Option<Aggregate>,
    /// Field to aggregate for value-style bands; `None` => first value field / all-values behavior.
    #[serde(default)]
    pub field: Option<String>,
    /// If true, this band is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl FooterBandSpec {
    pub fn grand_total() -> Self {
        Self { kind: crate::model::FooterBandKind::GrandTotal, label: None, aggregate: None, field: None, enabled: true }
    }
    pub fn notes() -> Self {
        Self { kind: crate::model::FooterBandKind::Notes, label: None, aggregate: None, field: None, enabled: true }
    }
    pub fn average() -> Self {
        Self { kind: crate::model::FooterBandKind::Average, label: None, aggregate: Some(Aggregate::Average), field: None, enabled: true }
    }
    pub fn count() -> Self {
        Self { kind: crate::model::FooterBandKind::Count, label: None, aggregate: Some(Aggregate::Count), field: None, enabled: true }
    }
    pub fn effective_label(&self) -> String {
        if let Some(l) = &self.label {
            if !l.trim().is_empty() { return l.clone(); }
        }
        match self.kind {
            crate::model::FooterBandKind::GrandTotal => "Grand Total".to_string(),
            crate::model::FooterBandKind::Notes => "Notes".to_string(),
            crate::model::FooterBandKind::Average => "Average".to_string(),
            crate::model::FooterBandKind::Count => "Count".to_string(),
            crate::model::FooterBandKind::Subtotal => "Subtotal".to_string(),
            crate::model::FooterBandKind::Custom => "Custom".to_string(),
        }
    }
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
    /// Ordered, configurable footer bands (AC3.2). Empty => legacy behavior:
    /// Grand Total when `subtotals.grand_total` is true, plus Notes.
    #[serde(default)]
    pub footer_bands: Vec<FooterBandSpec>,
}

impl PivotSpec {
    /// Resolve effective footer band specs, preserving legacy default behavior.
    pub fn effective_footer_bands(&self) -> Vec<FooterBandSpec> {
        if !self.footer_bands.is_empty() {
            return self.footer_bands.iter().filter(|b| b.enabled).cloned().collect();
        }
        let mut out = Vec::new();
        if self.subtotals.grand_total {
            out.push(FooterBandSpec::grand_total());
        }
        out.push(FooterBandSpec::notes());
        out
    }
}
