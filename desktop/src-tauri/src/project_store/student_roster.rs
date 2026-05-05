// SPDX-License-Identifier: AGPL-3.0-only
//! Project-scoped pseudonymous LMS roster.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::errors::HostResult;
use crate::models::{StudentIntakeState, StudentRosterRow};

use super::schema::{initialize_schema, project_db_path};

pub fn load_student_roster(project_path: &Path) -> HostResult<Vec<StudentRosterRow>> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    load_student_roster_from_connection(&connection)
}

pub fn load_student_ref_for_binding_token_hex(
    project_path: &Path,
    token_hex: &str,
) -> HostResult<Option<String>> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    connection
        .query_row(
            "SELECT student_ref
             FROM student_roster
             WHERE binding_token_hex = ?1",
            params![token_hex],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

pub fn sync_student_roster_tokens(
    project_path: &Path,
    ordered_binding_tokens: &[String],
) -> HostResult<Vec<StudentRosterRow>> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let existing_rows = load_student_roster_from_connection(&connection)?;
    let existing_by_token = existing_student_refs_by_token(&existing_rows);
    let mut allocator = StudentRefAllocator::from_existing(&existing_rows);
    let next_rows =
        build_student_roster_rows(ordered_binding_tokens, &existing_by_token, &mut allocator);

    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM student_roster", [])?;
    for row in &next_rows {
        transaction.execute(
            "INSERT INTO student_roster (
                student_ref,
                binding_token_hex,
                created_at,
                updated_at
            ) VALUES (?1, ?2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![row.student_ref, row.binding_token_hex],
        )?;
    }
    transaction.commit()?;
    Ok(next_rows)
}

pub(crate) fn attach_binding_tokens(
    project_path: &Path,
    state: &mut StudentIntakeState,
) -> HostResult<()> {
    if state.items.is_empty() {
        return Ok(());
    }
    let refs: Vec<String> = state
        .items
        .iter()
        .map(|item| item.student_ref.clone())
        .collect();
    let map = load_binding_tokens_by_refs(project_path, &refs)?;
    for item in &mut state.items {
        if let Some(token) = map.get(&item.student_ref) {
            item.binding_token_hex = Some(token.clone());
        }
    }
    Ok(())
}

fn load_student_roster_from_connection(
    connection: &Connection,
) -> HostResult<Vec<StudentRosterRow>> {
    let mut statement = connection.prepare(
        "SELECT student_ref, binding_token_hex
         FROM student_roster",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(StudentRosterRow {
            student_ref: row.get(0)?,
            binding_token_hex: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    out.sort_by(|left, right| {
        student_ref_sort_key(&left.student_ref).cmp(&student_ref_sort_key(&right.student_ref))
    });
    Ok(out)
}

fn load_binding_tokens_by_refs(
    project_path: &Path,
    student_refs: &[String],
) -> HostResult<HashMap<String, String>> {
    if student_refs.is_empty() {
        return Ok(HashMap::new());
    }
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let mut out = HashMap::new();
    for student_ref in student_refs {
        let token: Option<String> = connection
            .query_row(
                "SELECT binding_token_hex
                 FROM student_roster
                 WHERE student_ref = ?1",
                params![student_ref],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(token) = token {
            out.insert(student_ref.clone(), token);
        }
    }
    Ok(out)
}

fn existing_student_refs_by_token(rows: &[StudentRosterRow]) -> HashMap<String, String> {
    rows.iter()
        .map(|row| (row.binding_token_hex.clone(), row.student_ref.clone()))
        .collect()
}

fn build_student_roster_rows(
    ordered_binding_tokens: &[String],
    existing_by_token: &HashMap<String, String>,
    allocator: &mut StudentRefAllocator,
) -> Vec<StudentRosterRow> {
    let mut seen_tokens = HashSet::new();
    let mut rows = Vec::new();
    for token in ordered_binding_tokens {
        let Some(trimmed) = normalize_binding_token(token, &mut seen_tokens) else {
            continue;
        };
        let student_ref = existing_by_token
            .get(trimmed)
            .cloned()
            .unwrap_or_else(|| allocator.allocate());
        rows.push(StudentRosterRow {
            student_ref,
            binding_token_hex: trimmed.to_string(),
        });
    }
    rows
}

fn normalize_binding_token<'a>(
    token: &'a str,
    seen_tokens: &mut HashSet<String>,
) -> Option<&'a str> {
    let trimmed = token.trim();
    if trimmed.is_empty() || !seen_tokens.insert(trimmed.to_string()) {
        None
    } else {
        Some(trimmed)
    }
}

struct StudentRefAllocator {
    next_index: i64,
}

impl StudentRefAllocator {
    fn from_existing(rows: &[StudentRosterRow]) -> Self {
        let next_index = rows
            .iter()
            .filter_map(|row| parse_student_ref_index(&row.student_ref))
            .max()
            .unwrap_or(0)
            + 1;
        Self { next_index }
    }

    fn allocate(&mut self) -> String {
        let student_ref = format!("student_{}", self.next_index);
        self.next_index += 1;
        student_ref
    }
}

fn parse_student_ref_index(student_ref: &str) -> Option<i64> {
    student_ref
        .strip_prefix("student_")
        .and_then(|suffix| suffix.parse::<i64>().ok())
        .filter(|value| *value > 0)
}

fn student_ref_sort_key(student_ref: &str) -> (i64, String) {
    (
        parse_student_ref_index(student_ref).unwrap_or(i64::MAX),
        student_ref.to_string(),
    )
}
