pub(crate) mod agent_commands;
pub(crate) mod commands;
pub(crate) mod desktop_agent_runtime;
pub(crate) mod desktop_ask_commands;
pub(crate) mod desktop_cancel_commands;
pub(crate) mod desktop_chat_commands;
pub(crate) mod desktop_stream;
pub(crate) mod desktop_summary_commands;
pub mod domain_commands;
pub(crate) mod event_sink;
pub(crate) mod identity;
pub(crate) mod knowledge_commands;
pub(crate) mod provider_commands;
pub(crate) mod secret_commands;
pub(crate) mod skills_commands;
pub(crate) mod storage_commands;
pub(crate) mod task_commands;

use crate::{db, tasks::scheduler::SchedulerState};
use std::time::Duration;
use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(identity::LocalIdentity)
        .manage(db::DbState::default())
        .manage(crate::agent::desktop::LocalAgentState::default())
        .manage(crate::storage::secrets::SecretState::default())
        .manage(SchedulerState::default())
        .invoke_handler(commands::handler!())
        .build(tauri::generate_context!())
        .expect("failed to build Bloomery");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            let stopped = app_handle
                .state::<SchedulerState>()
                .shutdown(Duration::from_secs(2));
            if !stopped {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => app_handle.state::<SchedulerState>().request_shutdown(),
        _ => {}
    });
}
