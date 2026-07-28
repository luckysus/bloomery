#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod cloud_tasks;
mod context;
mod db;
mod local_agent;
mod models;
mod retrieval;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(auth::AuthState::default())
        .manage(db::DbState::default())
        .manage(local_agent::LocalAgentState::default())
        .invoke_handler(tauri::generate_handler![
            auth::auth_get_session,
            auth::auth_save_session,
            auth::auth_clear_session,
            db::db_init,
            db::list_conversations,
            db::list_archived_conversations,
            db::create_conversation,
            db::update_conversation_title,
            db::update_conversation_pinned,
            db::archive_conversation,
            db::restore_conversation,
            db::delete_conversation_local,
            db::list_messages,
            db::search_history,
            db::append_message,
            db::save_conversation_snapshot,
            db::replace_message_after_edit,
            db::truncate_conversation_after_message,
            db::fork_conversation_from_message,
            db::list_memories,
            db::list_archived_memories,
            db::get_memory,
            db::save_memory,
            db::archive_memory,
            db::restore_memory,
            db::search_memories,
            db::suggest_memories,
            db::get_conversation_summary,
            db::save_conversation_summary,
            db::get_conversation_draft,
            db::save_conversation_draft,
            db::clear_conversation_draft,
            db::list_cloud_jobs,
            db::save_cloud_job,
            db::update_cloud_job,
            db::get_setting,
            db::set_setting,
            db::export_diagnostics,
            cloud_tasks::desktop_cloud_binary_request,
            cloud_tasks::desktop_cloud_download_request,
            cloud_tasks::desktop_cloud_task_request,
            cloud_tasks::sync_cloud_jobs,
            context::build_context_packet,
            local_agent::desktop_cancel_llm_run,
            local_agent::desktop_agent_chat,
            local_agent::desktop_confirm_cloud_job,
            local_agent::desktop_llm_ask,
            local_agent::desktop_summarize_conversation,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Bloomery");
}
