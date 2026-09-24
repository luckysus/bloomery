use crate::agent::runtime::RuntimeHost;
use uuid::Uuid;

#[tauri::command]
pub fn desktop_cancel_llm_run(
    state: tauri::State<RuntimeHost>,
    run_id: String,
) -> Result<(), String> {
    let run_id = Uuid::parse_str(run_id.trim()).map_err(|_| "run_id must be a UUID".to_string())?;
    state.cancel_turn(run_id)
}
