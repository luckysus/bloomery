use super::ChildTurnStore;
use crate::agent::protocol::{AgentEventData, AgentEventEnvelope, RunOutcome, RunStateChanged};
use crate::agent::runtime::AgentEventSink;
use std::sync::Arc;
use uuid::Uuid;

pub(super) struct ChildAgentEventSink {
    run_id: Uuid,
    conversation_id: Uuid,
    sequence: u64,
    events: Vec<AgentEventEnvelope>,
    store: Option<Arc<dyn ChildTurnStore>>,
    store_error: Option<String>,
}

impl ChildAgentEventSink {
    pub(super) fn new(
        run_id: Uuid,
        conversation_id: Uuid,
        store: Option<Arc<dyn ChildTurnStore>>,
    ) -> Self {
        Self {
            run_id,
            conversation_id,
            sequence: 0,
            events: Vec::new(),
            store,
            store_error: None,
        }
    }

    fn event(&mut self, data: AgentEventData) -> AgentEventEnvelope {
        self.sequence += 1;
        let event = AgentEventEnvelope {
            protocol_version: crate::agent::protocol::PROTOCOL_VERSION,
            event_id: Uuid::new_v4(),
            run_id: self.run_id,
            conversation_id: self.conversation_id,
            sequence: self.sequence,
            timestamp: chrono::Utc::now(),
            data,
        };
        if self.store_error.is_none() {
            if let Some(store) = &self.store {
                if let Err(error) = store.append(&event) {
                    self.store_error = Some(error);
                }
            }
        }
        self.events.push(event.clone());
        event
    }

    pub(super) fn events(&self) -> &[AgentEventEnvelope] {
        &self.events
    }

    pub(super) fn error(&self) -> Option<String> {
        self.store_error.clone()
    }
}

impl AgentEventSink for ChildAgentEventSink {
    fn record(&mut self, data: AgentEventData) -> Result<AgentEventEnvelope, String> {
        Ok(self.event(data))
    }

    fn transition(&mut self, changed: RunStateChanged) -> Result<AgentEventEnvelope, String> {
        Ok(self.event(AgentEventData::RunStateChanged(changed)))
    }

    fn finish(
        &mut self,
        changed: RunStateChanged,
        outcome: RunOutcome,
        assistant_message_id: Option<Uuid>,
    ) -> Result<Vec<AgentEventEnvelope>, String> {
        Ok(vec![
            self.event(AgentEventData::RunStateChanged(changed)),
            self.event(AgentEventData::RunCompleted(
                crate::agent::protocol::RunCompleted {
                    outcome,
                    assistant_message_id,
                },
            )),
        ])
    }
}
