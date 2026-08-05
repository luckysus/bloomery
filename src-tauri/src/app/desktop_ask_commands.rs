use crate::agent::desktop::{
    assistant_content_for_stream_result, prepare_local_ask, LocalAgentState, LocalAskRequest,
};
use crate::app::desktop_stream::stream_llm_answer;
use crate::db::{current_workspace_id, with_conn, DbState};

#[tauri::command]
pub async fn desktop_llm_ask(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    agent_state: tauri::State<'_, LocalAgentState>,
    request: LocalAskRequest,
) -> Result<String, String> {
    let preparation = with_conn(&db, |conn| {
        prepare_local_ask(conn, current_workspace_id(), request)
    })?;
    let answer = stream_llm_answer(
        &app,
        &agent_state,
        "desktop-ask-delta",
        &preparation.run_id,
        &preparation.config,
        &preparation.prompt,
        &preparation.query,
    )
    .await;
    agent_state.clear_cancelled(&preparation.run_id);
    answer.map(|answer| assistant_content_for_stream_result(&answer))
}
