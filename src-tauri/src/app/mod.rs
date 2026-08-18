pub(crate) mod agent_commands;
pub(crate) mod bundled_domain;
pub(crate) mod bundled_domain_commands;
pub(crate) mod commands;
pub mod compute_commands;
pub(crate) mod database_commands;
pub(crate) mod desktop_agent_runtime;
pub(crate) mod desktop_ask_commands;
pub(crate) mod desktop_cancel_commands;
pub(crate) mod desktop_chat_commands;
pub(crate) mod desktop_stream;
pub(crate) mod desktop_summary_commands;
pub mod domain_commands;
pub(crate) mod domain_removal;
pub(crate) mod event_sink;
pub(crate) mod identity;
pub(crate) mod knowledge_commands;
pub(crate) mod mcp_agent_runtime;
pub(crate) mod mcp_commands;
pub(crate) mod mcp_runtime;
pub(crate) mod permission_commands;
pub(crate) mod provider_commands;
pub(crate) mod secret_commands;
pub(crate) mod skills_commands;
pub(crate) mod steel_agent_gateway;
pub(crate) mod steel_commands;
pub(crate) mod storage_commands;
pub(crate) mod task_commands;

use crate::{db, tasks::scheduler::SchedulerState};
use std::time::Duration;
use tauri::{Manager, RunEvent};

pub(crate) const BLOOMERY_TOKIO_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

pub fn run() {
    // 先安装脱敏 panic hook，确保任何崩溃报告在写入 stderr 前都经过 Redactor。
    crate::diagnostics::observability::install_panic_hook();
    install_async_runtime();

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
        .manage(mcp_runtime::McpRuntimeState::default())
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
        RunEvent::Exit => {
            let _ = tauri::async_runtime::block_on(
                app_handle
                    .state::<mcp_runtime::McpRuntimeState>()
                    .shutdown_all(),
            );
            app_handle.state::<SchedulerState>().request_shutdown();
        }
        _ => {}
    });
}

fn install_async_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("bloomery-async")
        .thread_stack_size(BLOOMERY_TOKIO_WORKER_STACK_BYTES)
        .build()
        .expect("failed to build Bloomery async runtime");
    let handle = runtime.handle().clone();
    let _ = Box::leak(Box::new(runtime));
    tauri::async_runtime::set(handle);
}

#[cfg(test)]
mod tests {
    use super::BLOOMERY_TOKIO_WORKER_STACK_BYTES;

    #[test]
    fn async_runtime_stack_is_sized_for_desktop_agent_runs() {
        assert!(BLOOMERY_TOKIO_WORKER_STACK_BYTES >= 8 * 1024 * 1024);
    }
}
