use crate::agent::desktop::{
    stream_llm_answer_core, LocalAgentDelta, LocalAgentState, LocalLlmConfig, StreamedLlmAnswer,
};
use tauri::Emitter;

pub async fn stream_llm_answer(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, LocalAgentState>,
    event_name: &str,
    run_id: &str,
    config: &LocalLlmConfig,
    context_prompt: &str,
    user_message: &str,
) -> Result<StreamedLlmAnswer, String> {
    stream_llm_answer_core(
        config,
        context_prompt,
        user_message,
        || state.is_cancelled(run_id),
        |delta| {
            let _ = app.emit(
                event_name,
                LocalAgentDelta {
                    run_id: run_id.to_string(),
                    delta: delta.to_string(),
                },
            );
        },
    )
    .await
}
