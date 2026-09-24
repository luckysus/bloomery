use crate::agent::desktop::{stream_llm_answer_core, LocalLlmConfig, StreamedLlmAnswer};
use crate::agent::runtime::RuntimeHost;

pub async fn stream_llm_answer(
    state: &tauri::State<'_, RuntimeHost>,
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
        |_| {},
    )
    .await
}
