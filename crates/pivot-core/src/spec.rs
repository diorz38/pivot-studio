//! Pivot specification: field assignments, aggregates, and filters (Phase 2).

use crate::Value;
use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

fn default_output_language() -> OutputLanguage { OutputLanguage::IdEn }

/// Language for auto-generated pivot output labels (title, subtotals, footer bands, notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputLanguage {
    En,
    Id,
    #[default]
    IdEn,
}

impl OutputLanguage {
    pub fn label(self) -> &'static str {
        match self {
            OutputLanguage::En => "English",
            OutputLanguage::Id => "Bahasa Indonesia",
            OutputLanguage::IdEn => "Bilingual (ID/EN)",
        }
    }
    pub const ALL: [OutputLanguage; 3] = [OutputLanguage::En, OutputLanguage::Id, OutputLanguage::IdEn];
}

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
        self.field.name.clone()
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
    /// Show a Total column across all pivot column values (when column fields are configured).
    #[serde(default)]
    pub column_totals: bool,
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
    /// Decimal places for numeric output (e.g. Average). `None` => auto formatting.
    #[serde(default)]
    pub decimals: Option<u8>,
}

impl FooterBandSpec {
    pub fn grand_total() -> Self {
        Self { kind: crate::model::FooterBandKind::GrandTotal, label: None, aggregate: None, field: None, enabled: true, decimals: None }
    }
    pub fn notes() -> Self {
        Self { kind: crate::model::FooterBandKind::Notes, label: None, aggregate: None, field: None, enabled: true, decimals: None }
    }
    pub fn average() -> Self {
        Self { kind: crate::model::FooterBandKind::Average, label: None, aggregate: Some(Aggregate::Average), field: None, enabled: true, decimals: None }
    }
    pub fn count() -> Self {
        Self { kind: crate::model::FooterBandKind::Count, label: None, aggregate: Some(Aggregate::Count), field: None, enabled: true, decimals: None }
    }
    pub fn effective_label(&self) -> String {
        self.effective_label_lang(OutputLanguage::En)
    }
    pub fn effective_label_lang(&self, lang: OutputLanguage) -> String {
        if let Some(l) = &self.label {
            if !l.trim().is_empty() { return l.clone(); }
        }
        use crate::model::FooterBandKind::*;
        match (self.kind, lang) {
            (GrandTotal, OutputLanguage::En) => "Grand Total".to_string(),
            (GrandTotal, OutputLanguage::Id) => "Total Keseluruhan".to_string(),
            (GrandTotal, OutputLanguage::IdEn) => "Grand Total (Total Keseluruhan)".to_string(),
            (Notes, OutputLanguage::En) => "Notes".to_string(),
            (Notes, OutputLanguage::Id) => "Catatan".to_string(),
            (Notes, OutputLanguage::IdEn) => "Notes (Catatan)".to_string(),
            (Average, OutputLanguage::En) => "Average".to_string(),
            (Average, OutputLanguage::Id) => "Rata-rata".to_string(),
            (Average, OutputLanguage::IdEn) => "Average (Rata-rata)".to_string(),
            (Count, OutputLanguage::En) => "Count".to_string(),
            (Count, OutputLanguage::Id) => "Jumlah".to_string(),
            (Count, OutputLanguage::IdEn) => "Count (Jumlah)".to_string(),
            (Subtotal, OutputLanguage::En) => "Subtotal".to_string(),
            (Subtotal, OutputLanguage::Id) => "Subtotal".to_string(),
            (Subtotal, OutputLanguage::IdEn) => "Subtotal".to_string(),
            (Custom, OutputLanguage::En) => "Custom".to_string(),
            (Custom, OutputLanguage::Id) => "Kustom".to_string(),
            (Custom, OutputLanguage::IdEn) => "Custom (Kustom)".to_string(),
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
    /// Language for auto-generated pivot output labels (title, subtotals, footer bands, notes).
    #[serde(default = "default_output_language")]
    pub output_language: OutputLanguage,
    /// Optional editable data source label shown in Notes (per-worksheet, persisted). `None` => use table source path/name.
    #[serde(default)]
    pub source_label: Option<String>,
    /// Per column-field level: show subtotal/total columns for that grouping level when it has 2+ values
    /// or as configured (parallel to `columns`). `true` inserts subtotal columns at group boundaries for
    /// that level. Missing/short entries default to `false` for backward compatibility.
    #[serde(default)]
    pub column_subtotals: Vec<bool>,
}

impl PivotSpec {
    /// Normalized per-column-level subtotal flags aligned to `columns.len()`.
    pub fn effective_column_subtotals(&self) -> Vec<bool> {
        let mut v = self.column_subtotals.clone();
        v.resize(self.columns.len(), false);
        v
    }

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
