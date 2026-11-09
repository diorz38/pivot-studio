//! Deterministic, SQLite-compatible column identifier normalization.
//!
//! Used for file-dataset ingestion (XLSX required; recommended for CSV/DBF)
//! so column names are stable, relation-safe identifiers. The original header
//! is preserved separately as metadata (`Column::original_name`), not as a
//! SQLite `COMMENT ON` (SQLite has no native column/table comments).

use std::collections::HashSet;

/// Conservative SQLite/common-SQL keyword set for collision avoidance.
/// This list is intentionally small and can be expanded; collisions are
/// resolved deterministically with a numeric suffix.
const SQLITE_KEYWORDS: &[&str] = &[
    "abort", "action", "add", "after", "all", "alter", "always", "analyze", "and", "as", "asc",
    "attach", "autoincrement", "before", "begin", "between", "by", "cascade", "case", "cast",
    "check", "collate", "column", "commit", "conflict", "constraint", "create", "cross",
    "current", "current_date", "current_time", "current_timestamp", "database", "default",
    "deferrable", "deferred", "delete", "desc", "detach", "distinct", "do", "drop", "each",
    "else", "end", "escape", "except", "exclude", "exclusive", "exists", "explain", "fail",
    "filter", "first", "following", "for", "foreign", "from", "full", "generated", "glob",
    "group", "groups", "having", "if", "ignore", "immediate", "in", "index", "indexed",
    "initially", "inner", "insert", "instead", "intersect", "into", "is", "isnull", "join",
    "key", "last", "left", "like", "limit", "match", "materialized", "natural", "no", "not",
    "nothing", "notnull", "null", "nulls", "of", "offset", "on", "or", "order", "others",
    "outer", "over", "partition", "plan", "pragma", "preceding", "primary", "query", "raise",
    "range", "recursive", "references", "regexp", "reindex", "release", "rename", "replace",
    "restrict", "returning", "right", "rollback", "row", "rows", "savepoint", "select", "set",
    "table", "temp", "temporary", "then", "ties", "to", "transaction", "trigger", "unbounded",
    "union", "unique", "update", "using", "vacuum", "values", "view", "virtual", "when",
    "where", "window", "with", "without",
];

/// Normalize a single header into a SQLite-compatible identifier.
///
/// The algorithm is deterministic:
/// 1. Trim; collapse whitespace runs to a single `_`.
/// 2. Replace chars outside `[A-Za-z0-9_]` with `_` (conservative/portable).
/// 3. Collapse consecutive `_`; trim leading/trailing `_`.
/// 4. Prefix `_` if it starts with a digit.
/// 5. Fallback to `column_<1-based index>` if empty.
/// 6. Lowercase the canonical form for stable case-insensitive behavior.
pub fn normalize_identifier(header: &str, one_based_index: usize) -> String {
    let trimmed = header.trim();
    let mut out = String::with_capacity(trimmed.len().max(8));
    let mut prev_us = false;
    for ch in trimmed.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if ch.is_whitespace() || ch == '_' {
            '_'
        } else {
            // Conservative MVP: non-ASCII and symbols become `_`.
            '_'
        };
        if mapped == '_' {
            if !prev_us {
                out.push('_');
                prev_us = true;
            }
        } else {
            out.push(mapped);
            prev_us = false;
        }
    }
    // Trim leading/trailing underscores.
    let mut s = out.trim_matches('_').to_string();
    if s.is_empty() {
        s = format!("column_{one_based_index}");
    } else if s.as_bytes().first().is_some_and(|b| b.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn is_keyword(ident: &str) -> bool {
    SQLITE_KEYWORDS.contains(&ident)
}

/// Normalize a list of headers, resolving per-dataset collisions and keywords.
///
/// Returns `(normalized_name, original_name)` pairs. `original_name` is `None`
/// only when it would be identical to `normalized_name` AND no disambiguation
/// occurred; in practice we keep the original header when it differs or when
/// normalization changed it, so UI can show the user's label.
pub fn normalize_headers(headers: &[String]) -> Vec<(String, Option<String>)> {
    let mut used: HashSet<String> = HashSet::new();
    let mut result = Vec::with_capacity(headers.len());
    for (i, header) in headers.iter().enumerate() {
        let base = normalize_identifier(header, i + 1);
        let mut candidate = base.clone();
        if is_keyword(&candidate) {
            candidate = format!("{candidate}_col");
        }
        let mut suffix = 2usize;
        while used.contains(&candidate) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        used.insert(candidate.clone());
        let original = header.clone();
        let original_meta = if original == candidate {
            None
        } else {
            Some(original)
        };
        result.push((candidate, original_meta));
    }
    result
}

/// Build `Column`s for file datasets (CSV/XLSX/DBF): infer types from raw
/// string rows, normalize headers to SQLite-compatible identifiers, and keep
/// original headers as metadata.
pub fn infer_file_columns(headers: &[String], rows: &[Vec<String>]) -> Vec<pivot_core::Column> {
    let normalized = normalize_headers(headers);
    let inferred = crate::infer::infer_column_types(headers.len(), rows);
    normalized
        .into_iter()
        .zip(inferred)
        .map(|((name, original_name), (dtype, nullable))| {
            pivot_core::Column::with_metadata(name, original_name, dtype, nullable)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_spaces_and_symbols() {
        assert_eq!(normalize_identifier("Order ID", 1), "order_id");
        assert_eq!(normalize_identifier("Customer Name", 1), "customer_name");
        assert_eq!(normalize_identifier("Total ($)", 1), "total");
        assert_eq!(normalize_identifier("A/B-C.D", 1), "a_b_c_d");
        assert_eq!(normalize_identifier("  lots   of   space  ", 1), "lots_of_space");
    }

    #[test]
    fn handles_digit_prefix_and_empty() {
        assert_eq!(normalize_identifier("1st Item", 1), "_1st_item");
        assert_eq!(normalize_identifier("", 3), "column_3");
        assert_eq!(normalize_identifier("   ", 7), "column_7");
        assert_eq!(normalize_identifier("$$$", 2), "column_2");
    }

    #[test]
    fn normalizes_non_ascii_conservatively() {
        // MVP conservative policy: non-ASCII becomes underscores/fallback.
        assert_eq!(normalize_identifier("名前", 1), "column_1");
        assert_eq!(normalize_identifier("Mã KH", 1), "m_kh");
    }

    #[test]
    fn resolves_collisions_and_keywords_deterministically() {
        let headers = vec![
            "Order ID".to_string(),
            "Order-ID".to_string(),
            "select".to_string(),
            "Select".to_string(),
        ];
        let out = normalize_headers(&headers);
        assert_eq!(out[0].0, "order_id");
        assert_eq!(out[1].0, "order_id_2");
        assert_eq!(out[2].0, "select_col");
        // "Select" normalizes to select, collides with keyword-adjusted select_col? no: base select -> select_col; next base select -> select_col exists -> select_2
        assert_eq!(out[3].0, "select_2");
        assert_eq!(out[0].1.as_deref(), Some("Order ID"));
    }

    #[test]
    fn infer_file_columns_preserves_original_name() {
        let headers = vec!["Order ID".to_string(), "qty".to_string()];
        let rows = vec![vec!["A".to_string(), "10".to_string()]];
        let cols = infer_file_columns(&headers, &rows);
        assert_eq!(cols[0].name, "order_id");
        assert_eq!(cols[0].original_name.as_deref(), Some("Order ID"));
        assert_eq!(
            cols[0].effective_display_name(),
            "Order ID"
        );
        assert_eq!(cols[1].name, "qty");
        assert_eq!(cols[1].original_name, None);
        assert_eq!(cols[1].inferred, pivot_core::DataType::Integer);
    }
}
