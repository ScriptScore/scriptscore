// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::errors::{HostError, HostResult};
use crate::models::{
    JobTraceEvent, JobTraceState, JobTraceSummary, ModerationState, QuestionAnalysisState,
    ResultsLmsState, RubricCriterion, RubricState, StudentIntakeState, StudentIntakeSummary,
    StudentWorkflowAnswer, StudentWorkflowState, StudentWorkflowSubmission,
};

use super::projects::{project_id, touch_project};
use super::schema::{initialize_schema, project_db_path, ARTIFACTS_DIR_NAME};
use super::trace_index::{refresh_job_student_ref_index, student_refs_for_job};

pub(crate) const QUESTION_ANALYSIS_STATE_KEY: &str = "question_analysis";
pub(crate) const QUESTION_RUBRIC_STATE_KEY: &str = "question_rubric";
pub(crate) const STUDENT_INTAKE_STATE_KEY: &str = "student_intake";
pub(crate) const STUDENT_WORKFLOW_STATE_KEY: &str = "student_workflow";
pub(crate) const MODERATION_STATE_KEY: &str = "moderation";
pub(crate) const RESULTS_LMS_STATE_KEY: &str = "results_lms";

pub fn persist_question_analysis_state(
    project_path: &Path,
    question_id: &str,
    state: &QuestionAnalysisState,
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    upsert_workflow_state(
        &transaction,
        "question",
        question_id,
        QUESTION_ANALYSIS_STATE_KEY,
        state.status_kind().as_str(),
        state,
    )?;
    if let Some(text) = state.question_text_clean.as_deref() {
        transaction.execute(
            "UPDATE question
             SET prompt_text = ?2,
                 updated_at = CURRENT_TIMESTAMP
             WHERE question_id = ?1",
            params![question_id, text.trim()],
        )?;
    }
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn load_question_analysis_states(
    connection: &Connection,
    question_ids: &[String],
) -> HostResult<HashMap<String, QuestionAnalysisState>> {
    load_question_state_map(connection, question_ids, QUESTION_ANALYSIS_STATE_KEY)
}

/// Updates only `question_context` in the persisted analysis payload (no `question` table prompt sync).
pub fn update_question_analysis_assets(
    project_path: &Path,
    question_id: &str,
    context: String,
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    let mut state: QuestionAnalysisState = load_workflow_state(
        &transaction,
        "question",
        question_id,
        QUESTION_ANALYSIS_STATE_KEY,
    )?
    .unwrap_or_default();
    state.question_context = Some(context);
    upsert_workflow_state(
        &transaction,
        "question",
        question_id,
        QUESTION_ANALYSIS_STATE_KEY,
        state.status_kind().as_str(),
        &state,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn save_rubric_state(
    project_path: &Path,
    question_id: &str,
    state: &RubricState,
) -> HostResult<()> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    upsert_workflow_state(
        &transaction,
        "question",
        question_id,
        QUESTION_RUBRIC_STATE_KEY,
        state.status_kind().as_str(),
        state,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn load_rubric_states(
    connection: &Connection,
    question_ids: &[String],
) -> HostResult<HashMap<String, RubricState>> {
    load_question_state_map(connection, question_ids, QUESTION_RUBRIC_STATE_KEY)
}

pub fn append_generated_rubric(
    project_path: &Path,
    question_id: &str,
    criteria: &[RubricCriterion],
    warnings: Vec<crate::models::WorkspaceWarning>,
    latest_job_id: &str,
    replace_existing: bool,
) -> HostResult<RubricState> {
    let mut current = load_rubric_state(project_path, question_id)?;
    let incoming_has_minimum_credit = criteria.iter().any(is_minimum_credit_criterion);
    let mut next = Vec::new();
    if !replace_existing {
        next.extend(
            current
                .criteria
                .iter()
                .filter(|criterion| {
                    !(incoming_has_minimum_credit && is_minimum_credit_criterion(criterion))
                })
                .cloned(),
        );
    }
    next.extend(criteria.iter().cloned());
    current.criteria = next;
    current.warnings = warnings;
    let was_approved = current.is_approved();
    current.mark_draft();
    current.latest_job_id = Some(latest_job_id.to_string());
    save_rubric_state(project_path, question_id, &current)?;
    if was_approved {
        mark_student_answers_stale_for_questions(project_path, &[question_id.to_string()])?;
    }
    Ok(current)
}

fn is_minimum_credit_criterion(criterion: &RubricCriterion) -> bool {
    criterion.source == "minimum_credit"
}

pub fn load_rubric_state(project_path: &Path, question_id: &str) -> HostResult<RubricState> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let state: Option<RubricState> = load_workflow_state(
        &connection,
        "question",
        question_id,
        QUESTION_RUBRIC_STATE_KEY,
    )?;
    Ok(state.unwrap_or_default())
}

pub fn save_student_intake_state(
    project_path: &Path,
    state: &StudentIntakeState,
) -> HostResult<StudentIntakeState> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    replace_student_scoped_states(
        &transaction,
        STUDENT_INTAKE_STATE_KEY,
        state
            .items
            .iter()
            .map(|item| (item.student_ref.as_str(), item.ingest_status.as_str(), item)),
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_student_intake_state(project_path)
}

pub fn load_student_intake_state(project_path: &Path) -> HostResult<StudentIntakeState> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let items: Vec<StudentIntakeSummary> =
        load_scoped_workflow_state_rows(&connection, "student", STUDENT_INTAKE_STATE_KEY)?;
    if items.is_empty() {
        return Ok(StudentIntakeState::not_started());
    }
    Ok(StudentIntakeState {
        status: crate::workflow_status::StudentIntakeStatus::Ready
            .as_str()
            .to_string(),
        latest_job_id: None,
        unresolved_count: 0,
        items,
    })
}

pub fn save_student_workflow_state(
    project_path: &Path,
    state: &StudentWorkflowState,
) -> HostResult<StudentWorkflowState> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    replace_student_scoped_states(
        &transaction,
        STUDENT_WORKFLOW_STATE_KEY,
        state.submissions.iter().map(|submission| {
            (
                submission.student_ref.as_str(),
                submission.stage.as_str(),
                submission,
            )
        }),
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_student_workflow_state(project_path)
}

pub fn save_student_workflow_submissions(
    project_path: &Path,
    submissions: &[StudentWorkflowSubmission],
) -> HostResult<StudentWorkflowState> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;
    for submission in submissions {
        upsert_workflow_state(
            &transaction,
            "student",
            &submission.student_ref,
            STUDENT_WORKFLOW_STATE_KEY,
            &submission.stage,
            submission,
        )?;
    }
    touch_project(&transaction)?;
    transaction.commit()?;
    load_student_workflow_state(project_path)
}

pub fn mark_student_answers_stale_for_questions(
    project_path: &Path,
    question_ids: &[String],
) -> HostResult<StudentWorkflowState> {
    if question_ids.is_empty() {
        return load_student_workflow_state(project_path);
    }
    let question_ids: std::collections::HashSet<&str> =
        question_ids.iter().map(String::as_str).collect();
    let mut workflow = load_student_workflow_state(project_path)?;
    if mark_workflow_answers_stale(&mut workflow, &question_ids) {
        save_student_workflow_state(project_path, &workflow)
    } else {
        Ok(workflow)
    }
}

fn mark_workflow_answers_stale(
    workflow: &mut StudentWorkflowState,
    question_ids: &std::collections::HashSet<&str>,
) -> bool {
    workflow
        .submissions
        .iter_mut()
        .flat_map(|submission| submission.answers.iter_mut())
        .fold(false, |changed, answer| {
            let should_mark = question_ids.contains(answer.question_id.as_str()) && !answer.stale;
            if should_mark {
                answer.stale = true;
            }
            changed || should_mark
        })
}

pub fn load_student_workflow_state(project_path: &Path) -> HostResult<StudentWorkflowState> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let submissions: Vec<StudentWorkflowSubmission> =
        load_scoped_workflow_state_rows(&connection, "student", STUDENT_WORKFLOW_STATE_KEY)?;
    let mut workflow = if submissions.is_empty() {
        StudentWorkflowState::not_started()
    } else {
        StudentWorkflowState {
            status: String::new(),
            latest_job_id: submissions
                .iter()
                .rev()
                .find_map(|submission| submission.latest_job_id.clone()),
            submissions,
        }
    };
    normalize_student_workflow_state(&mut workflow);
    Ok(workflow)
}

fn normalize_student_workflow_state(workflow: &mut StudentWorkflowState) {
    for submission in &mut workflow.submissions {
        for answer in &mut submission.answers {
            normalize_student_workflow_answer(answer);
        }
        normalize_submission_stage(submission);
    }
    workflow.status = normalized_workflow_status(workflow);
}

fn normalize_student_workflow_answer(answer: &mut StudentWorkflowAnswer) {
    let Some(prescreen) = answer.pii_prescreen.as_ref() else {
        return;
    };
    if prescreen.contains_pii {
        answer.manual_grading_required = true;
        answer.manual_grading_reason = Some("pii_detected".into());
        answer.parse_status = "blocked".into();
        answer.grading_status = "manual_required".into();
        return;
    }
    if !matches!(prescreen.status.as_str(), "ok" | "warning") {
        return;
    }

    answer.manual_grading_required = false;
    answer.manual_grading_reason = None;

    if answer.parse_status == "blocked"
        && answer.grading_status == "manual_required"
        && !answer.review_required
        && !answer.verified
        && answer.raw_parsed_text.is_none()
        && answer.verified_text.is_none()
        && answer.total_points_awarded.is_none()
        && answer.feedback_text.is_none()
    {
        answer.parse_status = "not_started".into();
        answer.grading_status = "not_started".into();
    }
}

fn normalize_submission_stage(submission: &mut StudentWorkflowSubmission) {
    if submission.stage != "manual_grading"
        || submission.answers.iter().any(|a| a.manual_grading_required)
    {
        return;
    }

    if let Some(next_stage) = normalized_manual_grading_stage(&submission.answers) {
        submission.stage = next_stage.into();
    }
}

fn normalized_manual_grading_stage(answers: &[StudentWorkflowAnswer]) -> Option<&'static str> {
    let has_review_required = answers.iter().any(|answer| answer.review_required);
    if has_review_required {
        return Some("parse_review");
    }

    let has_unverified_answer = answers
        .iter()
        .any(|answer| !answer.manual_grading_required && !answer.verified);
    if has_unverified_answer {
        return Some("parse");
    }

    let has_unstarted_grading = answers
        .iter()
        .any(|answer| !answer.manual_grading_required && answer.grading_status == "not_started");
    if has_unstarted_grading {
        return Some("grading");
    }

    let all_draft_ready = answers
        .iter()
        .all(|answer| answer.manual_grading_required || answer.grading_status == "draft_ready");
    all_draft_ready.then_some("graded")
}

fn normalized_workflow_status(workflow: &StudentWorkflowState) -> String {
    if workflow.submissions.is_empty() {
        "not_started".into()
    } else if workflow.submissions.iter().any(|submission| {
        matches!(
            submission.stage.as_str(),
            "alignment_review" | "parse_review" | "manual_grading" | "failed"
        )
    }) {
        "attention".into()
    } else if workflow
        .submissions
        .iter()
        .all(|submission| submission.stage == "graded")
    {
        "graded".into()
    } else if workflow
        .submissions
        .iter()
        .any(|submission| !matches!(submission.stage.as_str(), "intake_ready" | "stopped"))
    {
        "running".into()
    } else {
        "ready".into()
    }
}

pub fn save_moderation_state(
    project_path: &Path,
    state: &ModerationState,
) -> HostResult<ModerationState> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    upsert_workflow_state(
        &transaction,
        "project",
        &project_id,
        MODERATION_STATE_KEY,
        "ready",
        state,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_moderation_state(project_path)
}

pub fn load_moderation_state(project_path: &Path) -> HostResult<ModerationState> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let state: Option<ModerationState> =
        load_workflow_state(&connection, "project", &project_id, MODERATION_STATE_KEY)?;
    Ok(state.unwrap_or_default())
}

pub fn save_results_lms_state(
    project_path: &Path,
    state: &ResultsLmsState,
) -> HostResult<ResultsLmsState> {
    let mut connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let transaction = connection.transaction()?;
    upsert_workflow_state(
        &transaction,
        "project",
        &project_id,
        RESULTS_LMS_STATE_KEY,
        "ready",
        state,
    )?;
    touch_project(&transaction)?;
    transaction.commit()?;
    load_results_lms_state(project_path)
}

pub fn load_results_lms_state(project_path: &Path) -> HostResult<ResultsLmsState> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let project_id = project_id(&connection)?;
    let state: Option<ResultsLmsState> =
        load_workflow_state(&connection, "project", &project_id, RESULTS_LMS_STATE_KEY)?;
    Ok(state.unwrap_or_default())
}

pub fn canonical_intake_pdf_path(project_path: &Path, student_ref: &str) -> PathBuf {
    project_path
        .join(ARTIFACTS_DIR_NAME)
        .join("student_intake")
        .join("canonical_redacted")
        .join(format!("{student_ref}.pdf"))
}

pub fn load_job_trace(project_path: &Path, job_id: &str) -> HostResult<JobTraceState> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    refresh_job_student_ref_index(&connection)?;
    let mut trace = load_job_run_trace(&connection, job_id)?;
    trace.student_refs = student_refs_for_job(&connection, job_id)?;
    trace.events = load_job_trace_events(&connection, job_id)?;
    Ok(trace)
}

pub fn list_job_traces(project_path: &Path) -> HostResult<Vec<JobTraceSummary>> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    refresh_job_student_ref_index(&connection)?;
    let mut statement = connection.prepare(
        "SELECT
            job_run.job_id,
            job_run.command_name,
            job_run.state,
            job_run.submitted_at,
            job_run.started_at,
            job_run.finished_at,
            COUNT(job_event.event_id) AS event_count
         FROM job_run
         LEFT JOIN job_event ON job_event.job_id = job_run.job_id
         GROUP BY
            job_run.job_id,
            job_run.command_name,
            job_run.state,
            job_run.submitted_at,
            job_run.started_at,
            job_run.finished_at,
            job_run.rowid
         ORDER BY job_run.submitted_at DESC, job_run.rowid DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(JobTraceSummary {
            job_id: row.get(0)?,
            command_name: row.get(1)?,
            state: row.get(2)?,
            submitted_at: row.get(3)?,
            started_at: row.get(4)?,
            finished_at: row.get(5)?,
            event_count: row.get(6)?,
            student_refs: Vec::new(),
        })
    })?;
    let mut traces = Vec::new();
    for row in rows {
        let mut trace = row?;
        trace.student_refs = student_refs_for_job(&connection, &trace.job_id)?;
        traces.push(trace);
    }
    Ok(traces)
}

pub fn load_latest_job_trace_for_command(
    project_path: &Path,
    command_name: &str,
) -> HostResult<Option<JobTraceState>> {
    let connection = Connection::open(project_db_path(project_path))?;
    initialize_schema(&connection)?;
    let job_id: Option<String> = connection
        .query_row(
            "SELECT job_id
             FROM job_run
             WHERE command_name = ?1
             ORDER BY submitted_at DESC, rowid DESC
             LIMIT 1",
            [command_name],
            |row| row.get(0),
        )
        .optional()?;
    job_id
        .map(|job| load_job_trace(project_path, &job))
        .transpose()
}

fn load_question_state_map<T: DeserializeOwned + Default>(
    connection: &Connection,
    question_ids: &[String],
    state_key: &str,
) -> HostResult<HashMap<String, T>> {
    let mut map = HashMap::new();
    for question_id in question_ids {
        let state: Option<T> = load_workflow_state(connection, "question", question_id, state_key)?;
        if let Some(state) = state {
            map.insert(question_id.clone(), state);
        }
    }
    Ok(map)
}

fn load_workflow_state<T: DeserializeOwned>(
    connection: &Connection,
    scope_type: &str,
    scope_id: &str,
    state_key: &str,
) -> HostResult<Option<T>> {
    let payload_json: Option<String> = connection
        .query_row(
            "SELECT payload_json
             FROM workflow_state
             WHERE scope_type = ?1 AND scope_id = ?2 AND state_key = ?3",
            params![scope_type, scope_id, state_key],
            |row| row.get(0),
        )
        .optional()?;
    payload_json
        .map(|payload| serde_json::from_str::<T>(&payload).map_err(Into::into))
        .transpose()
}

fn load_scoped_workflow_state_rows<T: DeserializeOwned>(
    connection: &Connection,
    scope_type: &str,
    state_key: &str,
) -> HostResult<Vec<T>> {
    let mut statement = connection.prepare(
        "SELECT payload_json
         FROM workflow_state
         WHERE scope_type = ?1 AND state_key = ?2
         ORDER BY scope_id ASC",
    )?;
    let rows = statement.query_map(params![scope_type, state_key], |row| {
        row.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(serde_json::from_str::<T>(&row?)?);
    }
    Ok(out)
}

fn replace_student_scoped_states<'a, T, I>(
    transaction: &Transaction<'_>,
    state_key: &str,
    rows: I,
) -> HostResult<()>
where
    T: Serialize + 'a,
    I: IntoIterator<Item = (&'a str, &'a str, &'a T)>,
{
    transaction.execute(
        "DELETE FROM workflow_state
         WHERE state_key = ?1 AND scope_type IN ('student', 'project')",
        [state_key],
    )?;
    for (student_ref, status, payload) in rows {
        upsert_workflow_state(
            transaction,
            "student",
            student_ref,
            state_key,
            status,
            payload,
        )?;
    }
    Ok(())
}

fn upsert_workflow_state<T: Serialize>(
    transaction: &Transaction<'_>,
    scope_type: &str,
    scope_id: &str,
    state_key: &str,
    status: &str,
    payload: &T,
) -> HostResult<()> {
    transaction.execute(
        "INSERT INTO workflow_state (
            scope_type,
            scope_id,
            state_key,
            status,
            payload_json,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
         ON CONFLICT(scope_type, scope_id, state_key)
         DO UPDATE SET
            status = excluded.status,
            payload_json = excluded.payload_json,
            updated_at = CURRENT_TIMESTAMP",
        params![
            scope_type,
            scope_id,
            state_key,
            status,
            serde_json::to_string(payload)?
        ],
    )?;
    Ok(())
}

fn parse_or_empty(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::Object(Default::default()))
}

fn load_job_run_trace(connection: &Connection, job_id: &str) -> HostResult<JobTraceState> {
    connection
        .query_row(
            "SELECT command_name, state, submitted_at, started_at, finished_at, request_json, result_json, error_json
             FROM job_run
             WHERE job_id = ?1",
            [job_id],
            |row| {
                Ok(JobTraceState {
                    job_id: job_id.to_string(),
                    command_name: row.get(0)?,
                    state: row.get(1)?,
                    submitted_at: row.get(2)?,
                    started_at: row.get(3)?,
                    finished_at: row.get(4)?,
                    student_refs: Vec::new(),
                    request: parse_or_empty(&row.get::<_, String>(5)?),
                    result: row.get::<_, Option<String>>(6)?.map(|value| parse_or_empty(&value)),
                    error: row.get::<_, Option<String>>(7)?.map(|value| parse_or_empty(&value)),
                    events: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| HostError::Project(format!("Job trace '{job_id}' was not found.")))
}

fn load_job_trace_events(connection: &Connection, job_id: &str) -> HostResult<Vec<JobTraceEvent>> {
    let mut statement = connection.prepare(
        "SELECT sequence, event_type, progress_json, scope_json, data_json, created_at
         FROM job_event
         WHERE job_id = ?1
         ORDER BY sequence ASC, event_id ASC",
    )?;
    let rows = statement.query_map([job_id], |row| {
        let progress_json: Option<String> = row.get(2)?;
        let scope_json: Option<String> = row.get(3)?;
        let data_json: String = row.get(4)?;
        Ok(JobTraceEvent {
            sequence: row.get(0)?,
            event_type: row.get(1)?,
            progress: progress_json.as_deref().map(parse_or_empty),
            scope: scope_json.as_deref().map(parse_or_empty),
            data: parse_or_empty(&data_json),
            created_at: row.get(5)?,
        })
    })?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rusqlite::{params, Connection};

    use crate::models::{
        RubricCriterion, RubricState, StudentIntakeState, StudentIntakeSummary,
        StudentWorkflowAlignmentPage, StudentWorkflowAnswer, StudentWorkflowState,
        StudentWorkflowSubmission, StudentWorkflowTransform, WorkspaceWarning,
    };
    use crate::test_support::lock_env_vars;

    use super::super::schema::{initialize_schema, project_db_path};
    use super::{
        append_generated_rubric, list_job_traces, load_job_trace, load_student_intake_state,
        load_student_workflow_state, save_rubric_state, save_student_intake_state,
        save_student_workflow_state, save_student_workflow_submissions,
    };

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn bootstrap_project(project_path: &std::path::Path) {
        std::fs::create_dir_all(project_path).expect("project root should exist");
        let connection =
            Connection::open(project_db_path(project_path)).expect("project db should open");
        initialize_schema(&connection).expect("schema should initialize");
        connection
            .execute(
                "INSERT INTO project (
                    project_id,
                    display_name,
                    subject,
                    course_code,
                    lms_course_id,
                    redaction_required,
                    instructor_profile_json,
                    trace_refs_json,
                    created_at,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}', '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                params![
                    "proj_test",
                    "Midterm 1",
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    true
                ],
            )
            .expect("project should insert");
    }

    fn workflow_submission(student_ref: &str, stage: &str) -> StudentWorkflowSubmission {
        StudentWorkflowSubmission {
            student_ref: student_ref.into(),
            canonical_pdf_path: format!("/tmp/{student_ref}.pdf"),
            page_count: 1,
            stage: stage.into(),
            latest_job_id: None,
            failure_message: None,
            warnings: Vec::new(),
            page_artifacts: Vec::new(),
            alignment_pages: Vec::new(),
            detect_review: None,
            answers: Vec::new(),
        }
    }

    fn insert_job_run_row(
        project_path: &std::path::Path,
        job_id: &str,
        command_name: &str,
        state: &str,
        submitted_at: &str,
    ) {
        let connection =
            Connection::open(project_db_path(project_path)).expect("project db should open");
        connection
            .execute(
                "INSERT INTO job_run (
                    job_id,
                    command_name,
                    request_id,
                    state,
                    submitted_at,
                    started_at,
                    finished_at,
                    request_json,
                    result_json,
                    error_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '{}', NULL, NULL)",
                params![
                    job_id,
                    command_name,
                    format!("req_{job_id}"),
                    state,
                    submitted_at,
                    Some(format!("{submitted_at}:start")),
                    Some(format!("{submitted_at}:end")),
                ],
            )
            .expect("job run should insert");
    }

    fn insert_job_event_row(project_path: &std::path::Path, job_id: &str, sequence: i64) {
        let connection =
            Connection::open(project_db_path(project_path)).expect("project db should open");
        connection
            .execute(
                "INSERT INTO job_event (
                    job_id,
                    sequence,
                    event_type,
                    progress_json,
                    scope_json,
                    data_json,
                    created_at
                ) VALUES (?1, ?2, 'step', NULL, NULL, '{}', ?3)",
                params![job_id, sequence, format!("event-{sequence}")],
            )
            .expect("job event should insert");
    }

    fn insert_student_scoped_job_event_row(
        project_path: &std::path::Path,
        job_id: &str,
        sequence: i64,
        student_ref: &str,
    ) {
        let connection =
            Connection::open(project_db_path(project_path)).expect("project db should open");
        connection
            .execute(
                "INSERT INTO job_event (
                    job_id,
                    sequence,
                    event_type,
                    progress_json,
                    scope_json,
                    data_json,
                    created_at
                ) VALUES (?1, ?2, 'step', NULL, ?3, '{}', ?4)",
                params![
                    job_id,
                    sequence,
                    serde_json::json!({ "student_ref": student_ref }).to_string(),
                    format!("event-{sequence}")
                ],
            )
            .expect("student-scoped job event should insert");
    }

    #[test]
    fn list_job_traces_returns_newest_summaries_with_event_counts() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-trace-list");
        bootstrap_project(&project_path);

        insert_job_run_row(
            &project_path,
            "job_old",
            "exam.setup",
            "succeeded",
            "2026-04-01T00:00:00Z",
        );
        insert_job_run_row(
            &project_path,
            "job_new",
            "smoke.ping",
            "failed",
            "2026-04-02T00:00:00Z",
        );
        insert_job_event_row(&project_path, "job_new", 1);
        insert_job_event_row(&project_path, "job_new", 2);
        insert_job_event_row(&project_path, "job_old", 1);

        let traces = list_job_traces(&project_path).expect("trace summaries should load");

        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].job_id, "job_new");
        assert_eq!(traces[0].command_name, "smoke.ping");
        assert_eq!(traces[0].state, "failed");
        assert_eq!(traces[0].event_count, 2);
        assert_eq!(traces[1].job_id, "job_old");
        assert_eq!(traces[1].event_count, 1);
    }

    #[test]
    fn list_job_traces_returns_empty_history_for_new_project() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-trace-list-empty");
        bootstrap_project(&project_path);

        let traces = list_job_traces(&project_path).expect("trace summaries should load");

        assert!(traces.is_empty());
    }

    #[test]
    fn job_trace_history_indexes_student_refs_from_requests_and_events() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-trace-students");
        bootstrap_project(&project_path);
        let connection =
            Connection::open(project_db_path(&project_path)).expect("project db should open");
        connection
            .execute(
                "INSERT INTO job_run (
                    job_id,
                    command_name,
                    request_id,
                    state,
                    submitted_at,
                    started_at,
                    finished_at,
                    request_json,
                    result_json,
                    error_json
                ) VALUES (?1, 'grading.score-preliminary', 'req_student', 'running', ?2, ?3, NULL, ?4, NULL, NULL)",
                params![
                    "job_student",
                    "2026-04-03T00:00:00Z",
                    "2026-04-03T00:00:01Z",
                    serde_json::json!({ "studentRefs": ["student_a", "student_b"] }).to_string()
                ],
            )
            .expect("job run should insert");
        drop(connection);
        insert_student_scoped_job_event_row(&project_path, "job_student", 1, "student_c");

        let traces = list_job_traces(&project_path).expect("trace summaries should load");
        assert_eq!(
            traces[0].student_refs,
            vec!["student_a", "student_b", "student_c"]
        );

        let trace = load_job_trace(&project_path, "job_student").expect("trace should load");
        assert_eq!(
            trace.student_refs,
            vec!["student_a", "student_b", "student_c"]
        );
    }

    #[test]
    fn appending_generated_rubric_replaces_existing_minimum_credit_entry() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-rubric-append");
        bootstrap_project(&project_path);

        save_rubric_state(
            &project_path,
            "question_1",
            &RubricState {
                status: "draft".into(),
                criteria: vec![
                    RubricCriterion {
                        criterion_id: "minimum_credit_old".into(),
                        label: "Minimum credit".into(),
                        points: 1,
                        partial_credit_guidance: "Old minimum credit guidance".into(),
                        source: "minimum_credit".into(),
                    },
                    RubricCriterion {
                        criterion_id: "manual_1".into(),
                        label: "Reasoning".into(),
                        points: 4,
                        partial_credit_guidance: "Award for correct reasoning.".into(),
                        source: "manual".into(),
                    },
                ],
                warnings: Vec::new(),
                approved_at: None,
                latest_job_id: None,
                approval_basis: None,
            },
        )
        .expect("initial rubric state should save");

        let updated = append_generated_rubric(
            &project_path,
            "question_1",
            &[
                RubricCriterion {
                    criterion_id: "minimum_credit_new".into(),
                    label: "Minimum credit".into(),
                    points: 1,
                    partial_credit_guidance: "New minimum credit guidance".into(),
                    source: "minimum_credit".into(),
                },
                RubricCriterion {
                    criterion_id: "generated_1".into(),
                    label: "Generated criterion".into(),
                    points: 3,
                    partial_credit_guidance: "Generated guidance.".into(),
                    source: "generated".into(),
                },
            ],
            Vec::new(),
            "job_1",
            false,
        )
        .expect("generated rubric append should succeed");

        let minimum_credit = updated
            .criteria
            .iter()
            .filter(|criterion| criterion.source == "minimum_credit")
            .collect::<Vec<_>>();
        assert_eq!(minimum_credit.len(), 1);
        assert_eq!(updated.criteria.len(), 3);
        assert!(updated
            .criteria
            .iter()
            .any(|criterion| criterion.criterion_id == "manual_1"));
        assert!(updated
            .criteria
            .iter()
            .any(|criterion| criterion.criterion_id == "generated_1"));
    }

    #[test]
    fn student_workflow_state_round_trips_as_student_scoped_rows() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-student-workflow");
        bootstrap_project(&project_path);

        let saved = save_student_workflow_state(
            &project_path,
            &StudentWorkflowState {
                status: "attention".into(),
                latest_job_id: Some("job_student_workflow_1".into()),
                submissions: vec![StudentWorkflowSubmission {
                    student_ref: "student_1".into(),
                    canonical_pdf_path: "/tmp/student_1.pdf".into(),
                    page_count: 2,
                    stage: "alignment_review".into(),
                    latest_job_id: Some("job_align_1".into()),
                    failure_message: Some("Alignment confidence was too low.".into()),
                    warnings: vec![WorkspaceWarning {
                        code: Some("alignment_low_confidence".into()),
                        message: "Review required".into(),
                        scope: Some("page:1".into()),
                    }],
                    page_artifacts: Vec::new(),
                    alignment_pages: vec![StudentWorkflowAlignmentPage {
                        page_number: 1,
                        confidence: Some(0.42),
                        low_confidence: true,
                        review_exempt: false,
                        review_exempt_reason: None,
                        question_count: 1,
                        transform: StudentWorkflowTransform {
                            rotation: 0.5,
                            scale: 1.0,
                            translate_x: 12.0,
                            translate_y: -4.0,
                        },
                        warnings: Vec::new(),
                    }],
                    detect_review: None,
                    answers: Vec::new(),
                }],
            },
        )
        .expect("student workflow state should save");

        assert_eq!(saved.status, "attention");
        assert_eq!(saved.latest_job_id.as_deref(), Some("job_align_1"));
        assert_eq!(saved.submissions.len(), 1);
        assert_eq!(saved.submissions[0].stage, "alignment_review");
        assert_eq!(
            saved.submissions[0].alignment_pages[0]
                .transform
                .translate_x,
            12.0
        );
        assert_eq!(saved.submissions[0].alignment_pages[0].question_count, 1);
        assert!(!saved.submissions[0].alignment_pages[0].review_exempt);
        assert_eq!(
            saved.submissions[0].alignment_pages[0]
                .review_exempt_reason
                .as_deref(),
            None
        );

        let loaded =
            load_student_workflow_state(&project_path).expect("student workflow state should load");
        assert_eq!(loaded.status, "attention");
        assert_eq!(loaded.latest_job_id.as_deref(), Some("job_align_1"));
        assert_eq!(loaded.submissions.len(), 1);
        assert_eq!(loaded.submissions[0].student_ref, "student_1");
        assert_eq!(
            loaded.submissions[0].failure_message.as_deref(),
            Some("Alignment confidence was too low.")
        );
        assert_eq!(loaded.submissions[0].alignment_pages[0].question_count, 1);
        assert!(!loaded.submissions[0].alignment_pages[0].review_exempt);
        assert_eq!(
            loaded.submissions[0].warnings[0].code.as_deref(),
            Some("alignment_low_confidence")
        );

        let connection = Connection::open(project_db_path(&project_path)).expect("db should open");
        let scoped_count: i64 = connection
            .query_row(
                "SELECT COUNT(*)
                 FROM workflow_state
                 WHERE scope_type = 'student' AND state_key = 'student_workflow'",
                [],
                |row| row.get(0),
            )
            .expect("count should load");
        let aggregate_count: i64 = connection
            .query_row(
                "SELECT COUNT(*)
                 FROM workflow_state
                 WHERE scope_type = 'project' AND state_key = 'student_workflow'",
                [],
                |row| row.get(0),
            )
            .expect("count should load");
        assert_eq!(scoped_count, 1);
        assert_eq!(aggregate_count, 0);
    }

    #[test]
    fn student_workflow_submission_upsert_preserves_untouched_rows() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-student-upsert");
        bootstrap_project(&project_path);

        let mut student_1 = workflow_submission("student_1", "parse_review");
        let mut student_2 = workflow_submission("student_2", "grading");
        save_student_workflow_state(
            &project_path,
            &StudentWorkflowState {
                status: "running".into(),
                latest_job_id: None,
                submissions: vec![student_1.clone(), student_2.clone()],
            },
        )
        .expect("initial state should save");

        student_2.stage = "graded".into();
        save_student_workflow_submissions(&project_path, &[student_2.clone()])
            .expect("concurrent row update should save");

        student_1.stage = "grading".into();
        save_student_workflow_submissions(&project_path, &[student_1])
            .expect("single-row update should save");

        let loaded = load_student_workflow_state(&project_path).expect("workflow should load");
        let by_ref = loaded
            .submissions
            .iter()
            .map(|submission| (submission.student_ref.as_str(), submission.stage.as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(by_ref.get("student_1"), Some(&"grading"));
        assert_eq!(by_ref.get("student_2"), Some(&"graded"));
    }

    #[test]
    fn student_intake_state_round_trips_as_student_scoped_rows() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-student-intake");
        bootstrap_project(&project_path);

        let saved = save_student_intake_state(
            &project_path,
            &StudentIntakeState {
                status: "ready".into(),
                latest_job_id: Some("job_intake_1".into()),
                unresolved_count: 0,
                items: vec![StudentIntakeSummary {
                    student_ref: "student_1".into(),
                    local_display_name: None,
                    canonical_pdf_path: "/tmp/student_1.pdf".into(),
                    ingest_status: "prepared".into(),
                    page_count: 2,
                    exam_page_paths: vec!["/tmp/student_1_page_1.png".into()],
                    warnings: Vec::new(),
                    binding_token_hex: None,
                }],
            },
        )
        .expect("student intake state should save");

        assert_eq!(saved.status, "ready");
        assert_eq!(saved.latest_job_id, None);
        assert_eq!(saved.items.len(), 1);
        assert_eq!(saved.items[0].student_ref, "student_1");

        let loaded =
            load_student_intake_state(&project_path).expect("student intake state should load");
        assert_eq!(loaded.status, "ready");
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].canonical_pdf_path, "/tmp/student_1.pdf");

        let connection = Connection::open(project_db_path(&project_path)).expect("db should open");
        let scoped_count: i64 = connection
            .query_row(
                "SELECT COUNT(*)
                 FROM workflow_state
                 WHERE scope_type = 'student' AND state_key = 'student_intake'",
                [],
                |row| row.get(0),
            )
            .expect("count should load");
        let aggregate_count: i64 = connection
            .query_row(
                "SELECT COUNT(*)
                 FROM workflow_state
                 WHERE scope_type = 'project' AND state_key = 'student_intake'",
                [],
                |row| row.get(0),
            )
            .expect("count should load");
        assert_eq!(scoped_count, 1);
        assert_eq!(aggregate_count, 0);
    }

    #[test]
    fn load_student_workflow_state_normalizes_non_pii_warning_blocks() {
        let _guard = lock_env_vars();
        let project_path = temp_root("scriptscore-workflow-state-normalize-pii-warning");
        bootstrap_project(&project_path);

        save_student_workflow_state(
            &project_path,
            &StudentWorkflowState {
                status: "attention".into(),
                latest_job_id: Some("job_student_workflow_1".into()),
                submissions: vec![StudentWorkflowSubmission {
                    student_ref: "student_1".into(),
                    canonical_pdf_path: "/tmp/student_1.pdf".into(),
                    page_count: 1,
                    stage: "manual_grading".into(),
                    latest_job_id: Some("job_pii_1".into()),
                    failure_message: None,
                    warnings: Vec::new(),
                    page_artifacts: Vec::new(),
                    alignment_pages: Vec::new(),
                    detect_review: None,
                    answers: vec![StudentWorkflowAnswer {
                        question_id: "question_1".into(),
                        question_number: 1,
                        crop_image_path: Some("/tmp/q1.png".into()),
                        pii_prescreen: Some(crate::models::StudentWorkflowPiiPrescreen {
                            source_command: "scans.pii".into(),
                            status: "warning".into(),
                            contains_handwriting: "unknown".into(),
                            contains_pii: false,
                            pii_types_detected: Vec::new(),
                            warnings: vec![WorkspaceWarning {
                                code: Some("pii_handwriting_unknown".into()),
                                message: "Handwriting detection was inconclusive for this crop."
                                    .into(),
                                scope: None,
                            }],
                        }),
                        manual_grading_required: true,
                        manual_grading_reason: Some("pii_ambiguous".into()),
                        moderation_eligible: true,
                        parse_status: "blocked".into(),
                        parse_confidence: None,
                        parse_confidence_source: None,
                        raw_parsed_text: None,
                        verified_text: None,
                        review_required: false,
                        verified: false,
                        stale: false,
                        grading_status: "manual_required".into(),
                        grading_confidence: None,
                        grading_confidence_reason: None,
                        question_max_points: Some(5),
                        total_points_awarded: None,
                        feedback_text: None,
                        criterion_results: Vec::new(),
                        highlights: Vec::new(),
                        warnings: Vec::new(),
                    }],
                }],
            },
        )
        .expect("stale student workflow state should save");

        let loaded =
            load_student_workflow_state(&project_path).expect("student workflow state should load");
        let submission = &loaded.submissions[0];
        let answer = &submission.answers[0];

        assert_eq!(loaded.status, "running");
        assert_eq!(submission.stage, "parse");
        assert!(!answer.manual_grading_required);
        assert_eq!(answer.manual_grading_reason, None);
        assert_eq!(answer.parse_status, "not_started");
        assert_eq!(answer.grading_status, "not_started");
    }
}
