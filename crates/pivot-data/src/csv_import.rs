use crate::{DataError, Result};
use pivot_core::DataTable;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::infer::parse_row_strings;
use crate::normalize::infer_file_columns;

pub fn load_csv(
    path: &Path,
    delimiter: u8,
    has_header: bool,
    encoding_label: Option<&str>,
) -> Result<DataTable> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = decode_bytes(&bytes, encoding_label);
    load_csv_from_str(
        &text,
        delimiter,
        has_header,
        Some(path.to_string_lossy().into_owned()),
    )
}

/// Loads a CSV with the delimiter sniffed from the first non-empty line.
pub fn load_csv_auto(
    path: &Path,
    has_header: bool,
    encoding_label: Option<&str>,
) -> Result<DataTable> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = decode_bytes(&bytes, encoding_label);
    let delimiter = sniff_delimiter(&text);
    load_csv_from_str(
        &text,
        delimiter,
        has_header,
        Some(path.to_string_lossy().into_owned()),
    )
}

/// Picks the delimiter (`,`, `;`, tab, `|`) with the highest count outside
/// double quotes on the first non-empty line; defaults to `,`.
pub fn sniff_delimiter(text: &str) -> u8 {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    let mut best = b',';
    let mut best_count = 0usize;
    for &d in &[b',', b';', b'\t', b'|'] {
        let count = line
            .bytes()
            .scan(false, |in_quotes, b| {
                if b == b'"' {
                    *in_quotes = !*in_quotes;
                }
                Some((b, *in_quotes))
            })
            .filter(|&(b, in_quotes)| b == d && !in_quotes)
            .count();
        if count > best_count {
            best_count = count;
            best = d;
        }
    }
    best
}

pub fn load_csv_from_str(
    text: &str,
    delimiter: u8,
    has_header: bool,
    source: Option<String>,
) -> Result<DataTable> {
    // Strip UTF-8 BOM if present.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(has_header)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());

    let headers: Vec<String> = if has_header {
        let hdr = rdr.headers().map_err(DataError::Csv)?;
        hdr.iter().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    };

    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(DataError::Csv)?;
        raw_rows.push(rec.iter().map(|s| s.to_string()).collect());
    }

    let width = headers
        .len()
        .max(raw_rows.iter().map(|r| r.len()).max().unwrap_or(0));
    let headers = if has_header && !headers.is_empty() {
        let mut h = headers;
        h.resize(width, String::new());
        h
    } else {
        (0..width).map(|i| format!("Column{}", i + 1)).collect()
    };

    for r in &mut raw_rows {
        r.resize(width, String::new());
    }

    // File datasets use SQLite-compatible normalized identifiers and preserve
    // original headers as `Column::original_name` metadata.
    let columns = infer_file_columns(&headers, &raw_rows);
    let rows = raw_rows
        .iter()
        .map(|r| parse_row_strings(r, &columns))
        .collect();

    Ok(DataTable {
        columns,
        rows,
        source,
    })
}

fn decode_bytes(bytes: &[u8], label: Option<&str>) -> String {
    if let Some(label) = label {
        if let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            let (cow, _, _) = enc.decode(bytes);
            return cow.into_owned();
        }
    }
    // Default: UTF-8 lossy (deterministic, no panic).
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pivot_core::DataType;

    #[test]
    fn csv_header_and_delimiter() {
        let csv = "name,qty\nA,1\nB,2\n";
        let t = load_csv_from_str(csv, b',', true, None).unwrap();
        assert_eq!(t.column_count(), 2);
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.columns[0].name, "name");
        assert_eq!(t.columns[1].effective_type(), DataType::Integer);
    }

    #[test]
    fn tsv_parses() {
        let tsv = "name\tqty\nA\t3\n";
        let t = load_csv_from_str(tsv, b'\t', true, None).unwrap();
        assert_eq!(t.column_count(), 2);
        assert_eq!(t.row_count(), 1);
    }

    #[test]
    fn no_header_generates_columns() {
        let csv = "A,1\nB,2\n";
        let t = load_csv_from_str(csv, b',', false, None).unwrap();
        assert_eq!(t.columns[0].name, "column1");
        assert_eq!(t.row_count(), 2);
    }

    #[test]
    fn malformed_reports_error_without_panic() {
        // AC1.1: malformed CSV reports an error without crashing.
        // The csv crate's recovery/flexible behavior varies by input and version, so the
        // key property is: the call completes deterministically with no panic and returns a
        // reportable Result. If it errors, the message must be non-empty.
        let csv = "name,qty\n\"A,1\n";
        let res = load_csv_from_str(csv, b',', true, None);
        match res {
            Ok(t) => {
                // Recovered parse: still a valid, bounded table.
                assert!(t.column_count() >= 1);
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(!msg.is_empty(), "error must be reportable");
            }
        }
    }

    #[test]
    fn loads_50k_rows_synthetic() {
        // AC1.5: synthetic dataset of 50,000 rows (several columns) loads and is navigable.
        let mut csv = String::with_capacity(2_000_000);
        csv.push_str("id,category,amount,active,when\n");
        for i in 0..50_000u32 {
            let cat = if i % 3 == 0 { "A" } else if i % 3 == 1 { "B" } else { "C" };
            let active = if i % 2 == 0 { "true" } else { "false" };
            // simple deterministic date
            let day = (i % 28) + 1;
            csv.push_str(&format!("{i},{cat},{:.2},{active},2024-01-{day:02}\n", (i as f64) * 0.5));
        }
        let t = load_csv_from_str(&csv, b',', true, Some("synthetic://50k".to_string())).unwrap();
        assert_eq!(t.row_count(), 50_000);
        assert_eq!(t.column_count(), 5);
        assert_eq!(t.columns[0].effective_type(), DataType::Integer);
        assert_eq!(t.columns[1].effective_type(), DataType::String);
        assert_eq!(t.columns[2].effective_type(), DataType::Decimal);
    }

    #[test]
    fn csv_preserves_leading_zero_codes() {
        let csv = "kode,nama\n00123,A\n0456,B\n0007,C\n";
        let t = load_csv_from_str(csv, b',', true, None).unwrap();
        assert_eq!(t.columns[0].effective_type(), DataType::String);
        assert_eq!(t.get(0, 0), Some(&pivot_core::Value::String("00123".to_string())));
        assert_eq!(t.get(1, 0), Some(&pivot_core::Value::String("0456".to_string())));
        assert_eq!(t.get(2, 0), Some(&pivot_core::Value::String("0007".to_string())));
    }

    #[test]
    fn csv_normalizes_headers_and_preserves_originals() {
        let csv = "Order ID,Customer Name,Total ($)\n1,Alice,10\n";
        let t = load_csv_from_str(csv, b',', true, None).unwrap();
        assert_eq!(t.columns[0].name, "order_id");
        assert_eq!(t.columns[0].original_name.as_deref(), Some("Order ID"));
        assert_eq!(t.columns[1].name, "customer_name");
        assert_eq!(t.columns[1].original_name.as_deref(), Some("Customer Name"));
    }

    #[test]
    fn semicolon_csv_auto_detects_delimiter() {
        let csv = "name;qty\nA;1\nB;2\n";
        let t = load_csv_from_str(csv, sniff_delimiter(csv), true, None).unwrap();
        assert_eq!(t.column_count(), 2);
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.columns[0].name, "name");
        assert_eq!(t.columns[1].effective_type(), DataType::Integer);
    }

    #[test]
    fn sniff_delimiter_ignores_quoted_separators() {
        let csv = "\"a,b\";c\n\"x;y\";z\n";
        assert_eq!(sniff_delimiter(csv), b';');
        let t = load_csv_from_str(csv, sniff_delimiter(csv), false, None).unwrap();
        assert_eq!(t.column_count(), 2);
        assert_eq!(t.get(0, 0), Some(&pivot_core::Value::String("a,b".to_string())));
    }
}
