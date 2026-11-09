use pivot_core::{Column, DataType, Value};

pub fn infer_columns(headers: &[String], rows: &[Vec<String>]) -> Vec<Column> {
    let col_count = headers.len();
    let mut inferred = vec![DataType::String; col_count];
    let mut nullable = vec![false; col_count];
    let mut decided = vec![false; col_count];

    for row in rows {
        for (i, cell) in row.iter().enumerate().take(col_count) {
            let s = cell.trim();
            if s.is_empty() {
                nullable[i] = true;
                continue;
            }
            let t = infer_cell(s);
            if !decided[i] {
                inferred[i] = t;
                decided[i] = true;
            } else if inferred[i] != t {
                inferred[i] = promote(inferred[i], t);
            }
        }
    }

    headers
        .iter()
        .enumerate()
        .map(|(i, name)| Column {
            name: if name.trim().is_empty() {
                format!("Column{}", i + 1)
            } else {
                name.clone()
            },
            inferred: inferred[i],
            override_type: None,
            nullable: nullable[i],
        })
        .collect()
}

pub fn parse_row_strings(row: &[String], columns: &[Column]) -> Vec<Value> {
    columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let s = row.get(i).map(|s| s.as_str()).unwrap_or("");
            parse_cell(s, col.effective_type())
        })
        .collect()
}

pub fn parse_cell(s: &str, target: DataType) -> Value {
    let t = s.trim();
    if t.is_empty() {
        return Value::Null;
    }
    match target {
        DataType::String => Value::String(t.to_string()),
        DataType::Integer => t
            .replace(',', "")
            .parse::<i64>()
            .map(Value::Integer)
            .unwrap_or_else(|_| {
                t.replace(',', "")
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && n.fract() == 0.0)
                    .map(|n| Value::Integer(n as i64))
                    .unwrap_or(Value::Null)
            }),
        DataType::Decimal => t
            .replace(',', "")
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Value::Decimal)
            .unwrap_or(Value::Null),
        DataType::Boolean => match t.to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" | "y" => Value::Boolean(true),
            "false" | "f" | "0" | "no" | "n" => Value::Boolean(false),
            _ => Value::Null,
        },
        DataType::Date => Value::Date(t.to_string()),
        DataType::DateTime => Value::DateTime(t.to_string()),
    }
}

/// True for numeric-looking identifiers whose leading zero(s) are meaningful
/// (e.g. postal codes, account numbers, NIK fragments: "00123", "0123", "00").
/// Plain "0" (and "+0"/"-0") remains numeric.
fn has_meaningful_leading_zero(n: &str) -> bool {
    let digits = n.strip_prefix(['+', '-'].as_ref()).unwrap_or(n);
    digits.len() > 1 && digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit())
}

fn infer_cell(s: &str) -> DataType {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    if matches!(lower.as_str(), "true" | "false" | "yes" | "no") {
        return DataType::Boolean;
    }
    if is_datetime(t) {
        return DataType::DateTime;
    }
    if is_date(t) {
        return DataType::Date;
    }
    let n = t.replace(',', "");
    if has_meaningful_leading_zero(&n) {
        // Preserve leading zeros (IDs/codes). Treat as String even if numeric.
        return DataType::String;
    }
    if n.parse::<i64>().is_ok() {
        return DataType::Integer;
    }
    if n.parse::<f64>().is_ok() {
        return DataType::Decimal;
    }
    DataType::String
}

fn promote(a: DataType, b: DataType) -> DataType {
    use DataType::*;
    if a == b {
        return a;
    }
    match (a, b) {
        (String, _) | (_, String) => String,
        (DateTime, Date) | (Date, DateTime) | (DateTime, _) | (_, DateTime) => DateTime,
        (Date, _) | (_, Date) => String,
        (Decimal, Integer) | (Integer, Decimal) | (Decimal, Decimal) => Decimal,
        (Boolean, _) | (_, Boolean) => String,
        (Integer, Integer) => Integer,
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() == 10)
        && ((b[4] == b'-' && b[7] == b'-') || (b[4] == b'/' && b[7] == b'/'))
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn is_datetime(s: &str) -> bool {
    s.len() >= 16
        && (s.contains('T') || s.as_bytes().get(10) == Some(&b' '))
        && s.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_basic_types() {
        let headers = vec![
            "name".to_string(),
            "qty".to_string(),
            "price".to_string(),
            "ok".to_string(),
            "d".to_string(),
        ];
        let rows = vec![vec![
            "A".to_string(),
            "10".to_string(),
            "1.5".to_string(),
            "true".to_string(),
            "2024-01-31".to_string(),
        ]];
        let cols = infer_columns(&headers, &rows);
        assert_eq!(cols[0].effective_type(), DataType::String);
        assert_eq!(cols[1].effective_type(), DataType::Integer);
        assert_eq!(cols[2].effective_type(), DataType::Decimal);
        assert_eq!(cols[3].effective_type(), DataType::Boolean);
        assert_eq!(cols[4].effective_type(), DataType::Date);
    }

    #[test]
    fn promotes_integer_to_decimal() {
        let headers = vec!["n".to_string()];
        let rows = vec![vec!["1".to_string()], vec!["2.5".to_string()]];
        let cols = infer_columns(&headers, &rows);
        assert_eq!(cols[0].effective_type(), DataType::Decimal);
    }

    #[test]
    fn preserves_leading_zero_numeric_codes_as_string() {
        // Numeric-looking identifiers/codes with meaningful leading zeros must stay String
        // (importing as Integer would drop leading zeros: "00123" -> 123).
        let headers = vec!["kode".to_string(), "plain".to_string(), "zero".to_string()];
        let rows = vec![
            vec!["00123".to_string(), "123".to_string(), "0".to_string()],
            vec!["0456".to_string(), "456".to_string(), "0".to_string()],
            vec!["0007".to_string(), "7".to_string(), "0".to_string()],
        ];
        let cols = infer_columns(&headers, &rows);
        assert_eq!(cols[0].effective_type(), DataType::String, "leading-zero codes must infer as String");
        assert_eq!(cols[1].effective_type(), DataType::Integer, "plain integers remain Integer");
        assert_eq!(cols[2].effective_type(), DataType::Integer, "plain zero remains Integer");
        let parsed = parse_row_strings(&rows[0], &cols);
        assert_eq!(parsed[0], Value::String("00123".to_string()));
        assert_eq!(parsed[1], Value::Integer(123));
    }
}
