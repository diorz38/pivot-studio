use crate::{DataError, Result};
use dbase::FieldValue;
use pivot_core::{Column, DataTable, DataType, Value};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn load_dbf(path: &Path) -> Result<DataTable> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut dbf = dbase::Reader::new(reader).map_err(|e| DataError::Dbf(e.to_string()))?;
    let fields = dbf.fields().to_vec();

    let columns: Vec<Column> = fields
        .iter()
        .map(|f| Column {
            name: f.name().to_string(),
            inferred: map_field_type(f.field_type()),
            override_type: None,
            nullable: true,
        })
        .collect();

    let mut rows: Vec<Vec<Value>> = Vec::new();
    for record_res in dbf.iter_records() {
        let record = record_res.map_err(|e| DataError::Dbf(e.to_string()))?;
        let mut row = Vec::with_capacity(columns.len());
        for field in &fields {
            let v = match record.get(field.name()) {
                Some(fv) => map_value(fv, map_field_type(field.field_type())),
                None => Value::Null,
            };
            row.push(v);
        }
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        source: Some(path.to_string_lossy().into_owned()),
    })
}

fn map_field_type(ft: dbase::FieldType) -> DataType {
    use dbase::FieldType;
    match ft {
        FieldType::Numeric | FieldType::Float | FieldType::Double => DataType::Decimal,
        FieldType::Integer | FieldType::Currency => DataType::Integer,
        FieldType::Logical => DataType::Boolean,
        FieldType::Date => DataType::Date,
        FieldType::DateTime => DataType::DateTime,
        _ => DataType::String,
    }
}

fn map_value(v: &FieldValue, target: DataType) -> Value {
    match v {
        FieldValue::Character(Some(s)) => match target {
            DataType::String => Value::String(s.clone()),
            _ => crate::infer::parse_cell(s, target),
        },
        FieldValue::Character(None) => Value::Null,
        FieldValue::Memo(s) => match target {
            DataType::String => Value::String(s.clone()),
            _ => crate::infer::parse_cell(s, target),
        },
        FieldValue::Numeric(Some(n)) => Value::Decimal(*n),
        FieldValue::Numeric(None) => Value::Null,
        FieldValue::Float(Some(n)) => Value::Decimal(f64::from(*n)),
        FieldValue::Float(None) => Value::Null,
        FieldValue::Logical(Some(b)) => Value::Boolean(*b),
        FieldValue::Logical(None) => Value::Null,
        FieldValue::Date(Some(d)) => {
            Value::Date(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()))
        }
        FieldValue::Date(None) => Value::Null,
        FieldValue::Integer(n) => Value::Integer(i64::from(*n)),
        FieldValue::Double(n) => Value::Decimal(*n),
        FieldValue::Currency(c) => Value::Decimal(*c),
        FieldValue::DateTime(dt) => Value::DateTime(format!("{dt:?}")),
        // Fallback for future dbase variants; suppress unreachable in current exhaustive match.
        #[allow(unreachable_patterns)]
        other => Value::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbase::{FieldName, FieldValue, Record, TableWriterBuilder};
    use std::convert::TryFrom;

    #[test]
    fn dbf_happy_path_reads_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.dbf");
        let writer = TableWriterBuilder::new()
            .add_character_field(FieldName::try_from("NAME").unwrap(), 20)
            .add_numeric_field(FieldName::try_from("QTY").unwrap(), 10, 0)
            .build_with_file_dest(&path)
            .unwrap();

        let mut r1 = Record::default();
        r1.insert("NAME".to_string(), FieldValue::Character(Some("Alpha".to_string())));
        r1.insert("QTY".to_string(), FieldValue::Numeric(Some(10.0)));
        let mut r2 = Record::default();
        r2.insert("NAME".to_string(), FieldValue::Character(Some("Beta".to_string())));
        r2.insert("QTY".to_string(), FieldValue::Numeric(Some(20.0)));
        writer.write_records(&[r1, r2]).unwrap();

        let t = load_dbf(&path).unwrap();
        assert_eq!(t.row_count(), 2);
        assert!(t.column_count() >= 2);
        assert!(t.columns.iter().any(|c| c.name.eq_ignore_ascii_case("NAME")));
    }
}
