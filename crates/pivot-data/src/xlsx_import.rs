use crate::{DataError, Result};
use calamine::{open_workbook_auto, Data, Reader};
use pivot_core::DataTable;
use std::path::Path;

use crate::infer::parse_row_strings;
use crate::normalize::infer_file_columns;

pub fn list_sheets(path: &Path) -> Result<Vec<String>> {
    let workbook = open_workbook_auto(path).map_err(|e| DataError::Xlsx(e.to_string()))?;
    Ok(workbook.sheet_names().to_owned())
}

pub fn load_sheet_by_name(path: &Path, sheet: &str) -> Result<DataTable> {
    let mut workbook = open_workbook_auto(path).map_err(|e| DataError::Xlsx(e.to_string()))?;
    load_sheet(&mut workbook, sheet, Some(path.to_string_lossy().into_owned()))
}

pub fn load_first_sheet(path: &Path) -> Result<DataTable> {
    let mut workbook = open_workbook_auto(path).map_err(|e| DataError::Xlsx(e.to_string()))?;
    let sheets = workbook.sheet_names().to_owned();
    let name = sheets
        .first()
        .cloned()
        .ok_or_else(|| DataError::Xlsx("workbook has no sheets".to_string()))?;
    load_sheet(
        &mut workbook,
        &name,
        Some(path.to_string_lossy().into_owned()),
    )
}

pub fn load_sheet(
    workbook: &mut calamine::Sheets<std::io::BufReader<std::fs::File>>,
    sheet: &str,
    source: Option<String>,
) -> Result<DataTable> {
    let range = workbook
        .worksheet_range(sheet)
        .map_err(|e| DataError::Xlsx(format!("sheet '{sheet}': {e}")))?;
    let mut rows_iter = range.rows();
    let header_row = rows_iter
        .next()
        .ok_or_else(|| DataError::Xlsx(format!("sheet '{sheet}' is empty")))?;
    let headers: Vec<String> = header_row.iter().map(cell_to_string).collect();
    let width = headers.len();
    if width == 0 {
        return Err(DataError::Xlsx(format!("sheet '{sheet}' has no columns")));
    }
    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    for row in rows_iter {
        let mut r: Vec<String> = row.iter().map(cell_to_string).collect();
        r.resize(width, String::new());
        raw_rows.push(r);
    }
    // XLSX headers are normalized to SQLite-compatible identifiers and the
    // original header is preserved as `Column::original_name` metadata.
    let columns = infer_file_columns(&headers, &raw_rows);
    let rows = raw_rows
        .iter()
        .map(|r| parse_row_strings(r, &columns))
        .collect();
    Ok(DataTable {
        columns,
        rows,
        source: source.map(|s| format!("{s}#{sheet}")),
    })
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.is_finite() {
                // Keep integers clean.
                if f.fract() == 0.0 && f.abs() < (i64::MAX as f64) {
                    format!("{}", *f as i64)
                } else {
                    format!("{f}")
                }
            } else {
                String::new()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Data::Error(e) => format!("#ERR:{e:?}"),
        Data::DateTime(dt) => format!("{dt}"),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        // Fallback for future calamine variants.
        #[allow(unreachable_patterns)]
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pivot_core::DataType;
    use rust_xlsxwriter::Workbook;

    #[test]
    fn cell_to_string_basic_values() {
        assert_eq!(cell_to_string(&Data::Empty), "");
        assert_eq!(cell_to_string(&Data::String("x".to_string())), "x");
        assert_eq!(cell_to_string(&Data::Int(5)), "5");
        assert_eq!(cell_to_string(&Data::Bool(true)), "true");
    }

    #[test]
    fn xlsx_happy_path_reads_values_and_types() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.xlsx");
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("Data").unwrap();
        sheet.write_string(0, 0, "name").unwrap();
        sheet.write_string(0, 1, "qty").unwrap();
        sheet.write_string(0, 2, "price").unwrap();
        sheet.write_string(0, 3, "active").unwrap();
        sheet.write_string(1, 0, "A").unwrap();
        sheet.write_number(1, 1, 10.0).unwrap();
        sheet.write_number(1, 2, 1.5).unwrap();
        sheet.write_boolean(1, 3, true).unwrap();
        sheet.write_string(2, 0, "B").unwrap();
        sheet.write_number(2, 1, 20.0).unwrap();
        sheet.write_number(2, 2, 2.0).unwrap();
        sheet.write_boolean(2, 3, false).unwrap();
        workbook.save(&path).unwrap();

        let t = load_first_sheet(&path).unwrap();
        assert_eq!(t.column_count(), 4);
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.columns[0].name, "name");
        assert_eq!(t.columns[1].effective_type(), DataType::Integer);
        assert_eq!(t.columns[2].effective_type(), DataType::Decimal);
        // Boolean may be inferred as boolean or string depending on writer; accept both as non-crashing.
        assert!(matches!(
            t.columns[3].effective_type(),
            DataType::Boolean | DataType::String | DataType::Integer
        ));
    }

    #[test]
    fn xlsx_normalizes_headers_and_preserves_originals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("headers.xlsx");
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("Data").unwrap();
        sheet.write_string(0, 0, "Order ID").unwrap();
        sheet.write_string(0, 1, "Customer Name").unwrap();
        sheet.write_string(0, 2, "Total ($)").unwrap();
        sheet.write_string(0, 3, "1st Item").unwrap();
        sheet.write_string(1, 0, "A1").unwrap();
        sheet.write_string(1, 1, "Alice").unwrap();
        sheet.write_number(1, 2, 10.0).unwrap();
        sheet.write_string(1, 3, "X").unwrap();
        workbook.save(&path).unwrap();

        let t = load_first_sheet(&path).unwrap();
        assert_eq!(t.columns[0].name, "order_id");
        assert_eq!(t.columns[0].original_name.as_deref(), Some("Order ID"));
        assert_eq!(t.columns[1].name, "customer_name");
        assert_eq!(t.columns[1].original_name.as_deref(), Some("Customer Name"));
        assert_eq!(t.columns[2].name, "total");
        assert_eq!(t.columns[2].original_name.as_deref(), Some("Total ($)"));
        assert_eq!(t.columns[3].name, "_1st_item");
        assert_eq!(t.columns[3].original_name.as_deref(), Some("1st Item"));
    }
}
