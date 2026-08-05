use crate::agent::desktop::LocalAgentState;

#[tauri::command]
pub fn desktop_cancel_llm_run(
    state: tauri::State<LocalAgentState>,
    run_id: String,
) -> Result<(), String> {
    state.cancel_run(&run_id)
}
