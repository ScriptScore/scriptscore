// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::BTreeSet;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::errors::HostResult;

const STUDENT_REF_KEYS: [&str; 4] = ["student_ref", "studentRef", "student_refs", "studentRefs"];

pub(crate) fn upsert_job_student_refs(
    connection: &Connection,
    job_id: &str,
    values: &[Value],
) -> HostResult<()> {
    let refs = student_refs_from_values(values);
    for student_ref in refs {
        connection.execute(
            "INSERT OR IGNORE INTO job_trace_student_ref (job_id, student_ref)
             VALUES (?1, ?2)",
            params![job_id, student_ref],
        )?;
    }
    Ok(())
}

pub(crate) fn refresh_job_student_ref_index(connection: &Connection) -> HostResult<()> {
    refresh_request_student_refs(connection)?;
    refresh_event_student_refs(connection)?;
    Ok(())
}

fn refresh_request_student_refs(connection: &Connection) -> HostResult<()> {
    let mut job_statement = connection.prepare("SELECT job_id, request_json FROM job_run")?;
    let job_rows = job_statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in job_rows {
        let (job_id, request_json) = row?;
        let request = parse_json(&request_json);
        upsert_job_student_refs(connection, &job_id, &[request])?;
    }
    Ok(())
}

fn refresh_event_student_refs(connection: &Connection) -> HostResult<()> {
    let mut event_statement =
        connection.prepare("SELECT job_id, progress_json, scope_json, data_json FROM job_event")?;
    let event_rows = event_statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in event_rows {
        let (job_id, progress_json, scope_json, data_json) = row?;
        let mut values = vec![parse_json(&data_json)];
        if let Some(progress_json) = progress_json {
            values.push(parse_json(&progress_json));
        }
        if let Some(scope_json) = scope_json {
            values.push(parse_json(&scope_json));
        }
        upsert_job_student_refs(connection, &job_id, &values)?;
    }
    Ok(())
}

pub(crate) fn student_refs_for_job(
    connection: &Connection,
    job_id: &str,
) -> HostResult<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT student_ref
         FROM job_trace_student_ref
         WHERE job_id = ?1
         ORDER BY student_ref ASC",
    )?;
    let rows = statement.query_map([job_id], |row| row.get::<_, String>(0))?;
    let mut refs = Vec::new();
    for row in rows {
        refs.push(row?);
    }
    Ok(refs)
}

fn student_refs_from_values(values: &[Value]) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for value in values {
        collect_student_refs(value, &mut refs);
    }
    refs
}

fn collect_student_refs(value: &Value, refs: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => collect_student_refs_from_object(object, refs),
        Value::Array(items) => collect_student_refs_from_array(items, refs),
        _ => {}
    }
}

fn collect_student_refs_from_object(
    object: &serde_json::Map<String, Value>,
    refs: &mut BTreeSet<String>,
) {
    for (key, nested) in object {
        if is_student_ref_key(key) {
            collect_student_ref_value(nested, refs);
        }
        collect_student_refs(nested, refs);
    }
}

fn collect_student_refs_from_array(items: &[Value], refs: &mut BTreeSet<String>) {
    for item in items {
        collect_student_refs(item, refs);
    }
}

fn is_student_ref_key(key: &str) -> bool {
    STUDENT_REF_KEYS.contains(&key)
}

fn collect_student_ref_value(value: &Value, refs: &mut BTreeSet<String>) {
    match value {
        Value::String(item) => {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                refs.insert(trimmed.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_student_ref_value(item, refs);
            }
        }
        _ => {}
    }
}

fn parse_json(source: &str) -> Value {
    serde_json::from_str(source).unwrap_or(Value::Null)
}
