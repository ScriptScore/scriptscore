// SPDX-License-Identifier: AGPL-3.0-only
mod binding_token;
mod commands;
pub mod errors;
mod lms;
pub mod models;
mod path_utils;
mod project_store;
mod protocol;
mod secrets;
pub mod state;
pub mod test_support;
mod updates;
mod worker;
mod worker_runtime;
mod workflow_status;

use state::AppState;
#[cfg(target_os = "linux")]
use tauri::image::Image;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_linux_webkit_environment();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::bootstrap())
        .setup(|app| {
            app.state::<AppState>()
                .attach_app_handle(app.handle().clone());
            #[cfg(not(target_os = "linux"))]
            let _ = app;
            #[cfg(target_os = "linux")]
            apply_main_window_icon(app);
            eprintln!("scriptscore-desktop-host:startup-complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_shell_state,
            commands::create_project,
            commands::open_project,
            commands::close_current_project,
            commands::get_default_projects_root,
            commands::project_exists,
            commands::get_legal_disclosure,
            updates::check_app_update,
            commands::run_smoke_ping,
            commands::list_llm_models,
            commands::validate_llm_model,
            commands::cancel_active_job,
            commands::start_job,
            commands::get_exam_workspace_state,
            commands::recover_interrupted_student_workflow,
            commands::save_question_edits,
            commands::save_redaction_regions,
            commands::approve_template_setup,
            commands::ensure_automatic_rubric_jobs,
            commands::save_project_config,
            commands::skip_template_redaction,
            commands::generate_question_rubric,
            commands::reanalyze_question,
            commands::save_rubric_update,
            commands::save_criterion_score,
            commands::save_moderated_score,
            commands::save_moderated_feedback,
            commands::set_moderation_question_reviewed,
            commands::run_student_intake,
            commands::save_student_intake_page_order,
            commands::delete_student_submission,
            commands::begin_student_workflow,
            commands::regrade_question_answers,
            commands::confirm_student_alignment,
            commands::save_student_alignment_review,
            commands::confirm_student_detect_review,
            commands::save_student_detect_review,
            commands::confirm_student_parse_review,
            commands::save_student_parse_review,
            commands::get_job_trace,
            commands::list_job_traces,
            commands::replace_template_pdf,
            commands::export_stamped_template_pdf,
            commands::list_canvas_courses,
            commands::list_canvas_course_roster,
            commands::compute_lms_binding_token,
            commands::prior_canonical_submission_exists_for_lms_student,
            commands::resolve_lms_student_ref,
            commands::get_lms_roster_cache_state,
            commands::ensure_lms_roster_preload,
            commands::list_lms_assignments,
            commands::list_lms_assignments_for_course,
            commands::save_results_lms_assignment,
            commands::set_submission_result_finalized,
            commands::finalize_ready_results,
            commands::preview_results_lms_report,
            commands::run_results_lms_upload,
            commands::retry_results_lms_upload,
            commands::run_results_export,
            commands::transient_pdf_clip_text,
            commands::transient_scans_ocr_hint,
            commands::transient_render_pdf_page_png,
            commands::transient_clip_pdf_rects_png_base64,
            commands::intake_default_pdf_rects_from_template,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ScriptScore desktop host");
}

#[cfg(target_os = "linux")]
fn configure_linux_webkit_environment() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // Must be set before WebKitGTK initializes; matches the Linux dev launcher.
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_webkit_environment() {}

#[cfg(target_os = "linux")]
fn apply_main_window_icon(app: &mut tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let Ok(icon) = Image::from_bytes(include_bytes!("../icons/icon.png")) else {
        return;
    };

    let _ = window.set_icon(icon);
}
