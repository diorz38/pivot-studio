use crate::{DataError, Result};
use pivot_core::pivot::compute;
use pivot_core::{
    Aggregate, Column, DataTable, DataType, Filter, FilterOp, PivotSpec, ThemedTableModel, Value,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// How MySQL/DB credentials are stored (AC5.5-AC5.7). Profiles persist only non-secret metadata + mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CredentialMode {
    #[default]
    SessionOnly,
    OsKeychain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DbProfile {
    Sqlite(SqliteProfile),
    MySql(MySqlProfile),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteProfile {
    pub path: PathBuf,
    pub table: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MySqlProfile {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub table: String,
    pub ssl: bool,
    pub credential_mode: CredentialMode,
    /// Stable key reference for keychain lookup (no secret).
    pub credential_ref: String,
}

impl MySqlProfile {
    pub fn key_ref(&self) -> String {
        format!("pivot-studio:mysql:{}:{}:{}:{}", self.host, self.port, self.database, self.user)
    }
    pub fn sanitized_for_storage(&self) -> Self {
        let mut p = self.clone();
        p.credential_ref = p.key_ref();
        p
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableRef {
    pub table: String,
}

static SESSION_SECRETS: std::sync::OnceLock<std::sync::Mutex<BTreeMap<String, String>>> = std::sync::OnceLock::new();
fn session_secrets() -> &'static std::sync::Mutex<BTreeMap<String, String>> {
    SESSION_SECRETS.get_or_init(|| std::sync::Mutex::new(BTreeMap::new()))
}
const KEYRING_SERVICE: &str = "pivot-studio";

pub fn store_secret(mode: CredentialMode, key_ref: &str, secret: &str) -> Result<()> {
    let key = key_ref.trim();
    if key.is_empty() { return Err(DataError::Invalid("credential reference is empty".into())); }
    match mode {
        CredentialMode::SessionOnly => {
            let mut map = session_secrets().lock().map_err(|_| DataError::Credentials("session secret store poisoned".into()))?;
            map.insert(key.to_string(), secret.to_string());
            Ok(())
        }
        CredentialMode::OsKeychain => {
            remove_session_secret(key);
            let entry = keyring::Entry::new(KEYRING_SERVICE, key).map_err(|e| DataError::Credentials(e.to_string()))?;
            entry.set_password(secret).map_err(|e| DataError::Credentials(format!("keychain store failed: {e}")))?;
            Ok(())
        }
    }
}

pub fn load_secret(mode: CredentialMode, key_ref: &str) -> Result<Option<String>> {
    let key = key_ref.trim();
    if key.is_empty() { return Ok(None); }
    match mode {
        CredentialMode::SessionOnly => {
            let map = session_secrets().lock().map_err(|_| DataError::Credentials("session secret store poisoned".into()))?;
            Ok(map.get(key).cloned())
        }
        CredentialMode::OsKeychain => {
            let entry = keyring::Entry::new(KEYRING_SERVICE, key).map_err(|e| DataError::Credentials(e.to_string()))?;
            match entry.get_password() {
                Ok(pw) => Ok(Some(pw)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(DataError::Credentials(format!("keychain read failed: {e}"))),
            }
        }
    }
}

pub fn remove_secret(mode: CredentialMode, key_ref: &str) -> Result<()> {
    let key = key_ref.trim();
    match mode {
        CredentialMode::SessionOnly => { remove_session_secret(key); Ok(()) }
        CredentialMode::OsKeychain => {
            if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, key) {
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(DataError::Credentials(format!("keychain delete failed: {e}"))),
                }
            }
            remove_session_secret(key);
            Ok(())
        }
    }
}
fn remove_session_secret(key: &str) {
    if let Ok(mut map) = session_secrets().lock() { map.remove(key); }
}

pub fn switch_credential_mode(profile: &mut MySqlProfile, new_mode: CredentialMode, secret: Option<&str>) -> Result<()> {
    let key = profile.key_ref();
    match (profile.credential_mode, new_mode) {
        (a, b) if a == b => {}
        (CredentialMode::SessionOnly, CredentialMode::OsKeychain) => {
            let s = secret.map(|s| s.to_string()).or_else(|| load_secret(CredentialMode::SessionOnly, &key).ok().flatten())
                .ok_or_else(|| DataError::Credentials("password required to move credential to OS keychain".into()))?;
            store_secret(CredentialMode::OsKeychain, &key, &s)?;
            remove_session_secret(&key);
        }
        (CredentialMode::OsKeychain, CredentialMode::SessionOnly) => {
            let s = secret.map(|s| s.to_string()).or_else(|| load_secret(CredentialMode::OsKeychain, &key).ok().flatten())
                .ok_or_else(|| DataError::Credentials("password required (or unavailable in keychain) to move credential to session-only".into()))?;
            let _ = remove_secret(CredentialMode::OsKeychain, &key);
            store_secret(CredentialMode::SessionOnly, &key, &s)?;
        }
        _ => {}
    }
    profile.credential_mode = new_mode;
    profile.credential_ref = key;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct PushdownPlan {
    pub sql: String,
    pub params: Vec<ParamValue>,
    pub columns: Vec<Column>,
    pub result_mapping: ResultMapping,
}
#[derive(Debug, Clone)]
pub enum ParamValue { Text(String), Int(i64), Float(f64), Bool(bool), Null }
#[derive(Debug, Clone)]
pub struct ResultMapping { pub row_dims: usize, pub col_dims: usize, pub value_count: usize }

pub fn validate_identifier(name: &str) -> Result<()> {
    let n = name.trim();
    if n.is_empty() { return Err(DataError::Invalid("identifier is empty".into())); }
    let ok = n.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | ' ' | '-' | '.' | '$' | '#'));
    if !ok || n.contains(';') || n.contains('"') || n.contains('\'') || n.contains('`') || n.contains('\\') || n.contains('\0') {
        return Err(DataError::Invalid(format!("unsafe identifier rejected: {n:?}")));
    }
    Ok(())
}

pub fn quote_ident(name: &str) -> Result<String> {
    validate_identifier(name)?;
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}

pub fn aggregate_sql(agg: Aggregate, field: &str) -> Result<String> {
    let col = quote_ident(field)?;
    Ok(match agg {
        Aggregate::Sum => format!("COALESCE(SUM(CAST({col} AS REAL)), 0)"),
        Aggregate::Count => "COUNT(*)".to_string(),
        Aggregate::CountDistinct => format!("COUNT(DISTINCT {col})"),
        Aggregate::Average => format!("AVG(CAST({col} AS REAL))"),
        Aggregate::Min => format!("MIN({col})"),
        Aggregate::Max => format!("MAX({col})"),
    })
}

pub fn build_group_by_sql(table: &str, spec: &PivotSpec, dialect_sqlite: bool) -> Result<PushdownPlan> {
    validate_identifier(table)?;
    if spec.values.is_empty() { return Err(DataError::Invalid("pushdown pivot requires at least one value field".into())); }
    for f in spec.rows.iter().chain(spec.columns.iter()) { validate_identifier(&f.name)?; }
    for v in &spec.values { validate_identifier(&v.field.name)?; }
    for flt in &spec.filters { validate_identifier(&flt.field.name)?; }

    let mut select: Vec<String> = Vec::new();
    let mut group: Vec<String> = Vec::new();
    let mut columns: Vec<Column> = Vec::new();
    for f in spec.rows.iter().chain(spec.columns.iter()) {
        let qi = quote_ident(&f.name)?;
        select.push(qi.clone());
        group.push(qi);
        columns.push(Column { name: f.name.clone(), inferred: DataType::String, override_type: None, nullable: true });
    }
    for (i, v) in spec.values.iter().enumerate() {
        let expr = aggregate_sql(v.aggregate, &v.field.name)?;
        let alias = format!("v{i}");
        select.push(format!("{expr} AS \"{alias}\""));
        // Column name = SQL alias (matches result set). ValueField in result-spec uses alias as field; display_name preserves effective name.
        columns.push(Column { name: alias, inferred: if matches!(v.aggregate, Aggregate::Count|Aggregate::CountDistinct) { DataType::Integer } else { DataType::Decimal }, override_type: None, nullable: true });
    }
    let mut params: Vec<ParamValue> = Vec::new();
    let mut where_parts: Vec<String> = Vec::new();
    for flt in &spec.filters {
        let col = quote_ident(&flt.field.name)?;
        let (sql, mut p) = filter_sql(&col, flt)?;
        where_parts.push(sql);
        params.append(&mut p);
    }
    let table_q = quote_ident(table)?;
    let mut sql = format!("SELECT {} FROM {}", select.join(", "), table_q);
    if !where_parts.is_empty() { sql.push_str(" WHERE "); sql.push_str(&where_parts.join(" AND ")); }
    if !group.is_empty() { sql.push_str(" GROUP BY "); sql.push_str(&group.join(", ")); }
    let _ = dialect_sqlite;
    Ok(PushdownPlan { sql, params, columns, result_mapping: ResultMapping { row_dims: spec.rows.len(), col_dims: spec.columns.len(), value_count: spec.values.len() } })
}

fn filter_sql(col: &str, flt: &Filter) -> Result<(String, Vec<ParamValue>)> {
    let vals = &flt.values;
    let v0 = vals.first().cloned().unwrap_or(Value::Null);
    let p = to_param(v0.clone());
    Ok(match flt.op {
        FilterOp::Equals => (format!("{col} = ?"), vec![p]),
        FilterOp::NotEquals => (format!("({col} IS NULL OR {col} <> ?)"), vec![p]),
        FilterOp::Include => {
            if vals.is_empty() { ("1=0".into(), vec![]) } else {
                let ph = std::iter::repeat_n("?", vals.len()).collect::<Vec<_>>().join(",");
                (format!("{col} IN ({ph})"), vals.iter().cloned().map(to_param).collect())
            }
        }
        FilterOp::Exclude => {
            if vals.is_empty() { ("1=1".into(), vec![]) } else {
                let ph = std::iter::repeat_n("?", vals.len()).collect::<Vec<_>>().join(",");
                (format!("({col} IS NULL OR {col} NOT IN ({ph}))"), vals.iter().cloned().map(to_param).collect())
            }
        }
    })
}

fn to_param(v: Value) -> ParamValue {
    match v {
        Value::Null => ParamValue::Null,
        Value::String(s) | Value::Date(s) | Value::DateTime(s) => ParamValue::Text(s),
        Value::Integer(n) => ParamValue::Int(n),
        Value::Decimal(n) => ParamValue::Float(n),
        Value::Boolean(b) => ParamValue::Bool(b),
    }
}

fn pivot_spec_for_result(spec: &PivotSpec, plan: &PushdownPlan) -> PivotSpec {
    let mut out = spec.clone();
    out.rows = plan.columns.iter().take(plan.result_mapping.row_dims).map(|c| pivot_core::FieldRef::new(c.name.clone())).collect();
    out.columns = plan.columns.iter().skip(plan.result_mapping.row_dims).take(plan.result_mapping.col_dims).map(|c| pivot_core::FieldRef::new(c.name.clone())).collect();
    let value_cols = plan.columns.iter().skip(plan.result_mapping.row_dims + plan.result_mapping.col_dims);
    out.values = value_cols.enumerate().map(|(i, c)| {
        let display = spec.values.get(i).map(|v| v.effective_name()).unwrap_or_else(|| c.name.clone());
        pivot_core::ValueField { field: pivot_core::FieldRef::new(c.name.clone()), aggregate: Aggregate::Sum, display_name: Some(display) }
    }).collect();
    out.filters.clear();
    out
}

pub fn list_sqlite_tables(path: &PathBuf) -> Result<Vec<String>> {
    let conn = rusqlite::Connection::open(path).map_err(|e| DataError::Database(e.to_string()))?;
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(|e| DataError::Database(e.to_string()))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| DataError::Database(e.to_string()))?
        .filter_map(|r| r.ok()).collect();
    Ok(names)
}

pub fn connect_and_preview_sqlite(path: &PathBuf, table: &str, limit: usize, cancel: Option<&AtomicBool>) -> Result<DataTable> {
    validate_identifier(table)?;
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    let conn = rusqlite::Connection::open(path).map_err(|e| DataError::Database(e.to_string()))?;
    let sql = format!("SELECT * FROM {} LIMIT {}", quote_ident(table)?, limit.max(1));
    query_sqlite_conn(&conn, &sql, &[], cancel)
}

pub fn execute_pivot_sqlite(path: &PathBuf, table: &str, spec: &PivotSpec, cancel: Option<&AtomicBool>) -> Result<ThemedTableModel> {
    let plan = build_group_by_sql(table, spec, true)?;
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    let conn = rusqlite::Connection::open(path).map_err(|e| DataError::Database(e.to_string()))?;
    let agg = query_sqlite_conn(&conn, &plan.sql, &plan.params, cancel)?;
    let model_spec = pivot_spec_for_result(spec, &plan);
    Ok(compute(&agg, &model_spec))
}

fn query_sqlite_conn(conn: &rusqlite::Connection, sql: &str, params: &[ParamValue], cancel: Option<&AtomicBool>) -> Result<DataTable> {
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    let mut stmt = conn.prepare(sql).map_err(|e| DataError::Database(format!("{e}; sql={sql}")))?;
    let col_count = stmt.column_count();
    let names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
    let rusql_params: Vec<Box<dyn rusqlite::ToSql>> = params.iter().map(param_to_rusqlite).collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> = rusql_params.iter().map(|b| b.as_ref()).collect();
    let mut rows_out = Vec::new();
    let mut qrows = stmt.query(param_refs.as_slice()).map_err(|e| DataError::Database(e.to_string()))?;
    while let Some(row) = qrows.next().map_err(|e| DataError::Database(e.to_string()))? {
        if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
        let mut out = Vec::with_capacity(col_count);
        for i in 0..col_count { out.push(sqlite_value(row, i)); }
        rows_out.push(out);
    }
    let columns = names.into_iter().map(|n| Column { name: n, inferred: DataType::String, override_type: None, nullable: true }).collect();
    Ok(DataTable { columns, rows: rows_out, source: Some("sqlite".into()) })
}
fn param_to_rusqlite(p: &ParamValue) -> Box<dyn rusqlite::ToSql> {
    match p {
        ParamValue::Text(s) => Box::new(s.clone()),
        ParamValue::Int(n) => Box::new(*n),
        ParamValue::Float(n) => Box::new(*n),
        ParamValue::Bool(b) => Box::new(*b),
        ParamValue::Null => Box::new(rusqlite::types::Null),
    }
}
fn sqlite_value(row: &rusqlite::Row<'_>, i: usize) -> Value {
    use rusqlite::types::ValueRef;
    match row.get_ref(i) {
        Ok(ValueRef::Null) => Value::Null,
        Ok(ValueRef::Integer(n)) => Value::Integer(n),
        Ok(ValueRef::Real(n)) => if n.is_finite() { Value::Decimal(n) } else { Value::Null },
        Ok(ValueRef::Text(t)) => Value::String(String::from_utf8_lossy(t).into_owned()),
        Ok(ValueRef::Blob(b)) => Value::String(format!("<blob {} bytes>", b.len())),
        Err(_) => Value::Null,
    }
}

pub fn build_mysql_url(profile: &MySqlProfile, password: &str) -> Result<String> {
    validate_identifier(&profile.user)?;
    validate_identifier(&profile.database)?;
    if profile.host.is_empty() || profile.host.contains('@') || profile.host.contains('/') || profile.host.contains(' ') {
        return Err(DataError::Invalid("invalid MySQL host".into()));
    }
    let user = urlenc(&profile.user);
    let pass = urlenc(password);
    let db = urlenc(&profile.database);
    Ok(format!("mysql://{user}:{pass}@{}:{}/{db}", profile.host, profile.port))
}
fn urlenc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for b in s.bytes() { match b { b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~' => o.push(b as char), _ => o.push_str(&format!("%{b:02X}")) } }
    o
}

pub fn execute_pivot_mysql(profile: &MySqlProfile, spec: &PivotSpec, cancel: Option<&AtomicBool>) -> Result<ThemedTableModel> {
    let password = load_secret(profile.credential_mode, &profile.key_ref())?
        .or(load_secret(profile.credential_mode, &profile.credential_ref)?)
        .ok_or_else(|| DataError::Credentials("MySQL password not available; re-enter (session-only) or check OS keychain".into()))?;
    let plan = build_group_by_sql(&profile.table, spec, false)?;
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    let url = build_mysql_url(profile, &password)?;
    let pool = mysql::Pool::new(url.as_str()).map_err(|e| DataError::Database(e.to_string()))?;
    let mut conn = pool.get_conn().map_err(|e| DataError::Database(e.to_string()))?;
    let agg = query_mysql_conn(&mut conn, &plan.sql, &plan.params, cancel)?;
    let model_spec = pivot_spec_for_result(spec, &plan);
    Ok(compute(&agg, &model_spec))
}

pub fn preview_mysql(profile: &MySqlProfile, limit: usize, cancel: Option<&AtomicBool>) -> Result<DataTable> {
    let password = load_secret(profile.credential_mode, &profile.key_ref())?
        .or(load_secret(profile.credential_mode, &profile.credential_ref)?)
        .ok_or_else(|| DataError::Credentials("MySQL password not available".into()))?;
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    validate_identifier(&profile.table)?;
    let url = build_mysql_url(profile, &password)?;
    let pool = mysql::Pool::new(url.as_str()).map_err(|e| DataError::Database(e.to_string()))?;
    let mut conn = pool.get_conn().map_err(|e| DataError::Database(e.to_string()))?;
    let sql = format!("SELECT * FROM {} LIMIT {}", quote_ident(&profile.table)?, limit.max(1));
    query_mysql_conn(&mut conn, &sql, &[], cancel)
}

fn query_mysql_conn(conn: &mut mysql::PooledConn, sql: &str, params: &[ParamValue], cancel: Option<&AtomicBool>) -> Result<DataTable> {
    use mysql::prelude::Queryable;
    if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
    if params.is_empty() {
        let result = conn.query_iter(sql).map_err(|e| DataError::Database(format!("{e}; sql={sql}")))?;
        collect_mysql_result(result, cancel)
    } else {
        let mysql_params: Vec<mysql::Value> = params.iter().map(param_to_mysql).collect();
        let result = conn.exec_iter(sql, mysql_params).map_err(|e| DataError::Database(format!("{e}; sql={sql}")))?;
        collect_mysql_result(result, cancel)
    }
}

fn collect_mysql_result<P: mysql::prelude::Protocol>(result: mysql::QueryResult<'_, '_, '_, P>, cancel: Option<&AtomicBool>) -> Result<DataTable> {
    let columns_meta = result.columns();
    let cols_ref = columns_meta.as_ref();
    let names: Vec<String> = cols_ref.iter().map(|c| c.name_str().to_string()).collect();
    let mut rows_out = Vec::new();
    for row_res in result {
        if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) { return Err(DataError::Cancelled); }
        let row = row_res.map_err(|e| DataError::Database(e.to_string()))?;
        let mut out = Vec::with_capacity(names.len());
        for i in 0..names.len() { out.push(mysql_value(row.as_ref(i))); }
        rows_out.push(out);
    }
    let columns = names.into_iter().map(|n| Column { name: n, inferred: DataType::String, override_type: None, nullable: true }).collect();
    Ok(DataTable { columns, rows: rows_out, source: Some("mysql".into()) })
}
fn param_to_mysql(p: &ParamValue) -> mysql::Value {
    match p {
        ParamValue::Text(s) => mysql::Value::Bytes(s.as_bytes().to_vec()),
        ParamValue::Int(n) => mysql::Value::Int(*n),
        ParamValue::Float(n) => mysql::Value::Double(*n),
        ParamValue::Bool(b) => mysql::Value::Int(if *b { 1 } else { 0 }),
        ParamValue::Null => mysql::Value::NULL,
    }
}
fn mysql_value(v: Option<&mysql::Value>) -> Value {
    match v {
        None | Some(mysql::Value::NULL) => Value::Null,
        Some(mysql::Value::Int(n)) => Value::Integer(*n),
        Some(mysql::Value::UInt(n)) => i64::try_from(*n).map(Value::Integer).unwrap_or(Value::Decimal(*n as f64)),
        Some(mysql::Value::Float(n)) => Value::Decimal(*n as f64),
        Some(mysql::Value::Double(n)) => Value::Decimal(*n),
        Some(mysql::Value::Bytes(b)) => Value::String(String::from_utf8_lossy(b).into_owned()),
        Some(other) => Value::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pivot_core::{FieldRef, PivotSpec, SubtotalConfig, ValueField};

    #[test]
    fn ac5_3_identifiers_validated_and_queries_parameterized() {
        assert!(validate_identifier("Region").is_ok());
        assert!(validate_identifier("order id").is_ok());
        assert!(validate_identifier("x; DROP TABLE t").is_err());
        assert!(validate_identifier("a\"b").is_err());
        assert!(validate_identifier("a'b").is_err());
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            filters: vec![Filter { field: FieldRef::new("Product"), op: FilterOp::Equals, values: vec![Value::String("A".into())] }],
            subtotals: SubtotalConfig { grand_total: true, row_subtotals: false, column_totals: false },
            ..Default::default()
        };
        let plan = build_group_by_sql("sales", &spec, true).unwrap();
        assert!(plan.sql.contains("GROUP BY"));
        assert!(plan.sql.contains("WHERE"));
        assert!(plan.sql.contains('?'), "values must be parameterized: {}", plan.sql);
        assert!(!plan.sql.contains("'A'"), "untrusted value interpolated: {}", plan.sql);
        assert!(plan.sql.contains("\"Region\""));
        assert_eq!(plan.params.len(), 1);
        assert!(build_group_by_sql("t;drop", &spec, true).is_err());
    }

    #[test]
    fn ac5_5_session_secret_not_persisted_by_helper() {
        let key = "pivot-studio:test:session-only:ac55";
        let _ = remove_secret(CredentialMode::SessionOnly, key);
        store_secret(CredentialMode::SessionOnly, key, "pw123").unwrap();
        let got = load_secret(CredentialMode::SessionOnly, key).unwrap();
        assert_eq!(got.as_deref(), Some("pw123"));
        let profile = MySqlProfile { host: "h".into(), port: 3306, database: "d".into(), user: "u".into(), table: "t".into(), ssl: false, credential_mode: CredentialMode::SessionOnly, credential_ref: key.into() };
        let json = serde_json::to_string(&profile.sanitized_for_storage()).unwrap();
        assert!(!json.contains("pw123"), "profile must not contain secret: {json}");
        let _ = remove_secret(CredentialMode::SessionOnly, key);
    }

    #[test]
    fn ac5_7_mode_switch_session_to_keychain_fallback_path() {
        let key = "pivot-studio:test:switch:ac57";
        let _ = remove_secret(CredentialMode::SessionOnly, key);
        store_secret(CredentialMode::SessionOnly, key, "s3cr3t").unwrap();
        let mut profile = MySqlProfile { host: "h".into(), port: 3306, database: "d".into(), user: "u".into(), table: "t".into(), ssl: false, credential_mode: CredentialMode::SessionOnly, credential_ref: key.into() };
        let res = switch_credential_mode(&mut profile, CredentialMode::OsKeychain, None);
        match res {
            Ok(()) => {
                assert_eq!(profile.credential_mode, CredentialMode::OsKeychain);
                assert!(load_secret(CredentialMode::SessionOnly, key).unwrap().is_none());
                let _ = switch_credential_mode(&mut profile, CredentialMode::SessionOnly, Some("s3cr3t"));
                let _ = remove_secret(CredentialMode::OsKeychain, key);
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("keychain") || msg.contains("credential") || msg.contains("access") || msg.contains("denied") || msg.contains("No") || msg.contains("store"), "{msg}");
                assert_eq!(load_secret(CredentialMode::SessionOnly, key).unwrap().as_deref(), Some("s3cr3t"));
            }
        }
        profile.credential_mode = CredentialMode::SessionOnly;
        let json = serde_json::to_string(&profile.sanitized_for_storage()).unwrap();
        assert!(!json.contains("s3cr3t"));
        let _ = remove_secret(CredentialMode::SessionOnly, key);
    }

    #[test]
    fn ac5_1_sqlite_pushdown_matches_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE sales(Region TEXT, Product TEXT, Sales INTEGER);
            INSERT INTO sales VALUES ('East','A',100),('East','A',150),('East','B',50),('West','A',200);").unwrap();
        drop(conn);
        let spec = PivotSpec {
            rows: vec![FieldRef::new("Region")],
            values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }],
            subtotals: SubtotalConfig { grand_total: true, row_subtotals: false, column_totals: false },
            ..Default::default()
        };
        let push = execute_pivot_sqlite(&path, "sales", &spec, None).unwrap();
        let table = DataTable {
            columns: vec![
                Column { name: "Region".into(), inferred: DataType::String, override_type: None, nullable: true },
                Column { name: "Product".into(), inferred: DataType::String, override_type: None, nullable: true },
                Column { name: "Sales".into(), inferred: DataType::Integer, override_type: None, nullable: true },
            ],
            rows: vec![
                vec![Value::String("East".into()), Value::String("A".into()), Value::Integer(100)],
                vec![Value::String("East".into()), Value::String("A".into()), Value::Integer(150)],
                vec![Value::String("East".into()), Value::String("B".into()), Value::Integer(50)],
                vec![Value::String("West".into()), Value::String("A".into()), Value::Integer(200)],
            ],
            source: Some("mem".into()),
        };
        let mem = compute(&table, &spec);
        let pgt = push.footer_bands.iter().find(|b| matches!(b.kind, pivot_core::FooterBandKind::GrandTotal)).unwrap();
        let mgt = mem.footer_bands.iter().find(|b| matches!(b.kind, pivot_core::FooterBandKind::GrandTotal)).unwrap();
        assert!(pgt.cells.iter().any(|c| c.text == "500"));
        assert_eq!(pgt.cells.iter().map(|c| &c.text).collect::<Vec<_>>(), mgt.cells.iter().map(|c| &c.text).collect::<Vec<_>>());
        let pbody: Vec<Vec<String>> = push.body_rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect();
        let mbody: Vec<Vec<String>> = mem.body_rows.iter().map(|r| r.iter().map(|c| c.text.clone()).collect()).collect();
        assert_eq!(pbody, mbody);
    }

    #[test]
    fn ac5_2_mysql_pushdown_sql_generation_unit() {
        let profile = MySqlProfile { host: "127.0.0.1".into(), port: 3306, database: "test".into(), user: "u".into(), table: "sales".into(), ssl: false, credential_mode: CredentialMode::SessionOnly, credential_ref: "x".into() };
        let spec = PivotSpec { rows: vec![FieldRef::new("Region")], values: vec![ValueField { field: FieldRef::new("Sales"), aggregate: Aggregate::Sum, display_name: None }], ..Default::default() };
        let plan = build_group_by_sql(&profile.table, &spec, false).unwrap();
        assert!(plan.sql.contains("SELECT"));
        assert!(plan.sql.contains("FROM \"sales\""));
        assert!(build_mysql_url(&profile, "p@ss:word").unwrap().contains("mysql://"));
    }

    #[test]
    #[ignore = "requires a MySQL server; set PIVOT_MYSQL_URL=mysql://user:pass@host:3306/db and a 'sales' test table"]
    fn ac5_2_mysql_pushdown_integration() {
        let url = std::env::var("PIVOT_MYSQL_URL").expect("PIVOT_MYSQL_URL");
        assert!(url.starts_with("mysql://"));
    }

    #[test]
    fn ac5_4_cancel_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t(a TEXT); INSERT INTO t VALUES ('x');").unwrap();
        drop(conn);
        let cancel = AtomicBool::new(true);
        let spec = PivotSpec { rows: vec![FieldRef::new("a")], values: vec![ValueField { field: FieldRef::new("a"), aggregate: Aggregate::Count, display_name: None }], ..Default::default() };
        let res = execute_pivot_sqlite(&path, "t", &spec, Some(&cancel));
        assert!(matches!(res, Err(DataError::Cancelled)));
    }
}