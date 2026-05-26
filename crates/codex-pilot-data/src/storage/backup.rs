use crate::storage::{
    OwnedSqlValue, SQLiteStorageAdapter, decode_hex, encode_hex, has_table, json_to_sql_value,
    quote_identifier, sanitize_token_part, table_columns,
};
use anyhow::anyhow;
use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct BackupPayload {
    pub(super) session_id: String,
    pub(super) db_path: PathBuf,
    pub(super) schema: String,
    pub(super) tables: Map<String, Value>,
}

impl SQLiteStorageAdapter {
    pub(super) fn write_backup(
        &self,
        session_id: &str,
        schema: &str,
        tables: Map<String, Value>,
    ) -> anyhow::Result<String> {
        fs::create_dir_all(&self.backup_dir)?;
        let token = format!(
            "{}-{}",
            sanitize_token_part(session_id),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let payload = BackupPayload {
            session_id: session_id.to_string(),
            db_path: self.db_path.clone(),
            schema: schema.to_string(),
            tables,
        };
        let backup_path = self.backup_path(&token)?;
        fs::write(&backup_path, serde_json::to_vec_pretty(&payload)?)?;
        Ok(token)
    }

    pub(super) fn backup_path(&self, token: &str) -> anyhow::Result<PathBuf> {
        if token.is_empty() || token.contains('/') || token.contains('\\') || token.contains("..") {
            return Err(anyhow!("invalid undo token"));
        }
        Ok(self.backup_dir.join(format!("{token}.json")))
    }
}

pub(super) fn restore_tables(
    db: &mut Connection,
    tables: &Map<String, Value>,
) -> anyhow::Result<()> {
    validate_restore_tables(db, tables)?;
    let tx = db.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    for table in restore_table_order(tables) {
        let Some(rows) = tables.get(&table) else {
            continue;
        };
        if table.starts_with("__") {
            continue;
        }
        let Some(rows) = rows.as_array() else {
            continue;
        };
        for row in rows {
            let Some(row) = row.as_object() else {
                continue;
            };
            insert_row(&tx, &table, row)?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn restore_table_order(tables: &Map<String, Value>) -> Vec<String> {
    let preferred = [
        "sessions",
        "threads",
        "messages",
        "thread_dynamic_tools",
        "thread_goals",
        "thread_spawn_edges",
        "stage1_outputs",
        "agent_job_items",
    ];
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for table in preferred {
        if tables.contains_key(table) && seen.insert(table.to_string()) {
            ordered.push(table.to_string());
        }
    }
    let mut rest = tables
        .keys()
        .filter(|table| !table.starts_with("__"))
        .filter(|table| seen.insert((*table).clone()))
        .cloned()
        .collect::<Vec<_>>();
    rest.sort();
    ordered.extend(rest);
    ordered
}

fn validate_restore_tables(db: &Connection, tables: &Map<String, Value>) -> anyhow::Result<()> {
    for (table, rows) in tables {
        if table.starts_with("__") {
            continue;
        }
        if !has_table(db, table)? {
            return Err(anyhow!("cannot restore missing table {table}"));
        }
        let existing: HashSet<String> = table_columns(db, table)?.into_iter().collect();
        let Some(rows) = rows.as_array() else {
            continue;
        };
        for row in rows {
            let Some(row) = row.as_object() else {
                continue;
            };
            for column in row.keys() {
                if !existing.contains(column) {
                    return Err(anyhow!("cannot restore missing column {table}.{column}"));
                }
            }
        }
    }
    Ok(())
}

fn insert_row(db: &Connection, table: &str, row: &Map<String, Value>) -> anyhow::Result<()> {
    if row.is_empty() {
        return Ok(());
    }
    if table == "agent_job_items" && update_existing_agent_job_item(db, row)? {
        return Ok(());
    }
    let columns = row.keys().cloned().collect::<Vec<_>>();
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>();
    let values = columns
        .iter()
        .map(|column| OwnedSqlValue(json_to_sql_value(&row[column])))
        .collect::<Vec<_>>();
    let params = values
        .iter()
        .map(|value| value as &dyn ToSql)
        .collect::<Vec<_>>();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_identifier(table),
        columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", "),
        placeholders.join(", ")
    );
    db.execute(&sql, params.as_slice())?;
    Ok(())
}

fn update_existing_agent_job_item(
    db: &Connection,
    row: &Map<String, Value>,
) -> anyhow::Result<bool> {
    let Some(id) = row.get("id") else {
        return Ok(false);
    };
    if !row.contains_key("assigned_thread_id") || !has_table(db, "agent_job_items")? {
        return Ok(false);
    }
    let id_value = OwnedSqlValue(json_to_sql_value(id));
    let current = db.query_row(
        "SELECT assigned_thread_id FROM agent_job_items WHERE id = ?1 LIMIT 1",
        [&id_value as &dyn ToSql],
        |row| row.get::<_, Option<String>>(0),
    );
    let current = match current {
        Ok(value) => value,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if current.is_some() {
        return Err(anyhow!("restore conflict: agent job item already assigned"));
    }
    let assigned = OwnedSqlValue(json_to_sql_value(&row["assigned_thread_id"]));
    db.execute(
        "UPDATE agent_job_items SET assigned_thread_id = ?1 WHERE id = ?2 AND assigned_thread_id IS NULL",
        [&assigned as &dyn ToSql, &id_value as &dyn ToSql],
    )?;
    Ok(true)
}

pub(super) fn rollout_file_backups(thread_rows: &[Value]) -> Vec<Value> {
    thread_rows
        .iter()
        .filter_map(|row| row.get("rollout_path").and_then(Value::as_str))
        .filter(|path| !path.trim().is_empty())
        .filter_map(|path| {
            let bytes = fs::read(path).ok()?;
            Some(json!({
                "path": path,
                "content_hex": encode_hex(&bytes),
            }))
        })
        .collect()
}

pub(super) fn remove_rollout_files(files: &[Value]) -> Vec<String> {
    let mut errors = Vec::new();
    for file in files {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        if let Err(error) = fs::remove_file(path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!("{path}: {error}"));
            }
        }
    }
    errors
}

pub(super) fn restore_files(tables: &Map<String, Value>) -> anyhow::Result<()> {
    let Some(files) = tables.get("__files").and_then(Value::as_array) else {
        return Ok(());
    };
    for file in files {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some(content) = file.get("content_hex").and_then(Value::as_str) else {
            continue;
        };
        let path = Path::new(path);
        if path.exists() {
            return Err(anyhow!(
                "restore conflict: file already exists: {}",
                path.display()
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, decode_hex(content)?)?;
    }
    Ok(())
}

pub(super) fn session_index_backups(db_path: &Path, thread_id: &str) -> Vec<Value> {
    let Some(index_path) = session_index_path(db_path) else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(&index_path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| session_index_line_id(line).as_deref() == Some(thread_id))
        .map(|line| {
            json!({
                "path": index_path,
                "line": line,
            })
        })
        .collect()
}

pub(super) fn remove_session_index_entries(entries: &[Value], thread_id: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let paths = entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    for path in paths {
        let path = Path::new(path);
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let mut changed = false;
        let kept = text
            .lines()
            .filter(|line| {
                let should_remove = session_index_line_id(line).as_deref() == Some(thread_id);
                changed |= should_remove;
                !should_remove
            })
            .collect::<Vec<_>>();
        if changed {
            let next = if kept.is_empty() {
                String::new()
            } else {
                format!("{}\n", kept.join("\n"))
            };
            if let Err(error) = fs::write(path, next) {
                errors.push(format!("{}: {error}", path.display()));
            }
        }
    }
    errors
}

pub(super) fn restore_session_index_entries(tables: &Map<String, Value>) -> anyhow::Result<()> {
    let Some(entries) = tables.get("__session_index").and_then(Value::as_array) else {
        return Ok(());
    };
    let mut by_path = HashSet::new();
    for path in entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
    {
        by_path.insert(path.to_string());
    }
    for path in by_path {
        let path = PathBuf::from(path);
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let restore_lines = entries
            .iter()
            .filter(|entry| {
                entry.get("path").and_then(Value::as_str) == Some(path.to_string_lossy().as_ref())
            })
            .filter_map(|entry| entry.get("line").and_then(Value::as_str))
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        if restore_lines.is_empty() {
            continue;
        }
        let restore_ids = restore_lines
            .iter()
            .filter_map(|line| session_index_line_id(line))
            .collect::<HashSet<_>>();
        let mut lines = existing
            .lines()
            .filter(|line| {
                session_index_line_id(line)
                    .map(|id| !restore_ids.contains(&id))
                    .unwrap_or(true)
            })
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        lines.extend(restore_lines.into_iter().map(ToString::to_string));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &path,
            if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            },
        )?;
    }
    Ok(())
}

fn session_index_path(db_path: &Path) -> Option<PathBuf> {
    if db_path.file_name().and_then(|name| name.to_str()) != Some("state_5.sqlite") {
        return None;
    }
    Some(db_path.parent()?.join("session_index.jsonl"))
}

fn session_index_line_id(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(super) fn backup_title(tables: &Map<String, Value>) -> Option<String> {
    backup_first_row(tables, "sessions")
        .and_then(|row| row.get("title"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            backup_first_row(tables, "threads")
                .and_then(|row| row.get("title"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

pub(super) fn backup_project_cwd(tables: &Map<String, Value>) -> Option<String> {
    backup_first_row(tables, "threads")
        .and_then(|row| row.get("cwd"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(ToString::to_string)
}

pub(super) fn backup_last_active_at(tables: &Map<String, Value>) -> Option<u64> {
    let row = backup_first_row(tables, "threads")?;
    ["updated_at_ms", "updated_at", "created_at_ms"]
        .iter()
        .find_map(|column| timestamp_seconds(row.get(*column)?))
}

fn backup_first_row<'a>(
    tables: &'a Map<String, Value>,
    table: &str,
) -> Option<&'a Map<String, Value>> {
    tables
        .get(table)
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(Value::as_object)
}

fn timestamp_seconds(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64().map(|value| {
            if value > 10_000_000_000 {
                value / 1000
            } else {
                value
            }
        }),
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else if let Ok(number) = trimmed.parse::<u64>() {
                Some(if number > 10_000_000_000 {
                    number / 1000
                } else {
                    number
                })
            } else {
                parse_rfc3339_seconds(trimmed)
            }
        }
        _ => None,
    }
}

fn parse_rfc3339_seconds(value: &str) -> Option<u64> {
    let date_time = value.strip_suffix('Z').unwrap_or(value);
    let (date, time) = date_time.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i64>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    let time = time.split(['+', '-']).next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u64>().ok()?;
    let minute = time_parts.next()?.parse::<u64>().ok()?;
    let second = time_parts
        .next()
        .and_then(|part| part.split('.').next())
        .unwrap_or("0")
        .parse::<u64>()
        .ok()?;
    let days = days_from_civil(year, month, day)?;
    u64::try_from(days)
        .ok()?
        .checked_mul(86_400)?
        .checked_add(hour.checked_mul(3_600)?)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}
