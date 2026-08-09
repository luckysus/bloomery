mod cancellation;
mod model;
mod prompt;
mod provider;
mod routing;
mod service;
mod session;

pub use cancellation::{permission_key_for, LocalAgentState};
pub use model::{
    DesktopIntentKind, DesktopRoute, LocalAgentChatRequest, LocalAgentDelta, LocalAskRequest,
    LocalLlmConfig, StreamedLlmAnswer, SummarizeConversationRequest, SummarizeConversationResponse,
};
pub(crate) use prompt::assistant_content_for_stream_result;
pub(crate) use provider::{provider_profile_from_config, stream_llm_answer_core};
pub(crate) use routing::build_agent_response_json;
pub(crate) use service::{
    append_agent_message, build_agent_loop_request, prepare_chat, prepare_local_ask,
    prepare_summary, save_summary, ChatPreparation,
};

#[cfg(test)]
mod tests;
