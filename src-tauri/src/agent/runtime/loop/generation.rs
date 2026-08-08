use super::helpers::prepare_tool_calls;
use super::types::{
    AgentEventSink, AgentLoop, AgentLoopError, CancellationToken, RepairedToolBatch, ToolExecutor,
};
use crate::agent::protocol::{AgentEventData, AgentMessageRole};
use crate::agent::runtime::model_adapter::ModelAdapter;
use crate::agent::tool_repair::{ToolRepairError, MAX_REPAIR_RETRIES};
use crate::providers::capabilities::{
    ChatEvent, ChatMessage, ChatRequest, ChatResponse, ChatToolCall,
};
use crate::providers::http::ProviderError;
use serde_json::Value;
use uuid::Uuid;

impl<'a, M: ?Sized, T: ?Sized, P: ?Sized> AgentLoop<'a, M, T, P>
where
    M: ModelAdapter,
    T: ToolExecutor,
{
    pub(super) async fn generate(
        &self,
        request: ChatRequest,
        message_id: Uuid,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
    ) -> Result<(ChatResponse, String), AgentLoopError> {
        let mut streamed_text = String::new();
        let mut sink_error = None;
        let mut on_event = |event: ChatEvent| match event {
            ChatEvent::TextDelta(delta) => {
                streamed_text.push_str(&delta);
                if sink_error.is_none() {
                    if let Err(error) = sink.record(AgentEventData::MessageDelta(
                        crate::agent::protocol::MessageDelta {
                            message_id,
                            role: AgentMessageRole::Assistant,
                            delta,
                        },
                    )) {
                        sink_error = Some(error);
                    }
                }
            }
            ChatEvent::Usage(usage) => {
                if sink_error.is_none() {
                    if let Err(error) = sink.record(AgentEventData::UsageUpdated(
                        crate::agent::protocol::UsageUpdated {
                            prompt_tokens: usage.prompt_tokens,
                            completion_tokens: usage.completion_tokens,
                            total_tokens: usage.total_tokens,
                        },
                    )) {
                        sink_error = Some(error);
                    }
                }
            }
            ChatEvent::ToolCallDelta(_) => {}
        };
        let response = self
            .model
            .generate(request, &mut on_event, cancellation.callback())
            .await
            .map_err(AgentLoopError::Provider)?;
        drop(on_event);
        if let Some(error) = sink_error {
            return Err(AgentLoopError::EventSink(error));
        }
        Ok((response, streamed_text))
    }

    pub(super) async fn repair_tool_calls(
        &self,
        messages: &[ChatMessage],
        initial: &[ChatToolCall],
        tool_payload: Option<&Value>,
        message_id: Uuid,
        sink: &mut dyn AgentEventSink,
        cancellation: &CancellationToken,
    ) -> Result<RepairedToolBatch, AgentLoopError> {
        let specs = self
            .tools
            .registrations()
            .iter()
            .map(|registration| registration.spec.clone())
            .collect::<Vec<_>>();
        let mut model_calls = initial.to_vec();
        let mut last_error = None;
        for attempt in 0..=MAX_REPAIR_RETRIES {
            match prepare_tool_calls(&model_calls, &specs, self.tools.registrations()) {
                Ok(calls) => return Ok(RepairedToolBatch { model_calls, calls }),
                Err(error) => {
                    last_error = Some(error);
                    if attempt == MAX_REPAIR_RETRIES {
                        break;
                    }
                }
            }
            let feedback = last_error
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "tool call is invalid".to_string());
            let mut repair_messages = messages.to_vec();
            repair_messages.push(ChatMessage::new(
                "system",
                format!("The previous tool call failed validation: {feedback}. Return corrected tool calls only."),
            ));
            let (response, _) = self
                .generate(
                    ChatRequest {
                        messages: repair_messages,
                        temperature: 0.0,
                        tools: tool_payload.cloned(),
                        response_format: None,
                    },
                    message_id,
                    sink,
                    cancellation,
                )
                .await?;
            if response.cancelled || cancellation.is_cancelled() {
                return Err(AgentLoopError::Provider(ProviderError::cancelled()));
            }
            model_calls = response.tool_calls;
            if model_calls.is_empty() {
                last_error = Some(ToolRepairError::InvalidEnvelope {
                    message: "repair response did not contain a tool call".to_string(),
                });
            }
        }
        Err(AgentLoopError::ToolRepair(
            ToolRepairError::RetryExhausted {
                attempts: MAX_REPAIR_RETRIES + 1,
                last: Box::new(last_error.unwrap_or(ToolRepairError::InvalidEnvelope {
                    message: "tool call repair failed".to_string(),
                })),
            },
        ))
    }
}
