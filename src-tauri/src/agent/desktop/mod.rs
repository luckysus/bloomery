mod cancellation;
mod model;
mod prompt;
mod provider;
mod routing;
mod service;
mod session;

pub use cancellation::{permission_key_for, LocalAgentState};
pub use model::{
    load_agent_preferences, load_model_runtime_preferences, AgentPreferences, DesktopIntentKind,
    DesktopRoute, LocalAgentChatRequest, LocalLlmConfig, ModelRuntimePreferences,
    StreamedLlmAnswer, SummarizeConversationRequest, SummarizeConversationResponse,
};
pub(crate) use prompt::assistant_content_for_stream_result;
pub(crate) use provider::{
    load_local_llm_config, provider_profile_from_config, stream_llm_answer_core,
};
pub(crate) use routing::{build_agent_response_json, plan_steps_for_route, selected_agent_id};
pub(crate) use service::{
    add_mailbox_context, append_agent_message, build_agent_loop_request_with_attachments,
    prepare_chat, prepare_summary, recover_active_runs_if_allowed, save_summary, ChatPreparation,
};

#[cfg(test)]
mod tests;
