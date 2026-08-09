use super::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolInvocation,
    ToolRegistration,
};
use std::collections::BTreeSet;

pub struct CompositeToolExecutor<'a> {
    sources: Vec<&'a dyn ToolExecutor>,
    registrations: Vec<ToolRegistration>,
}

impl<'a> CompositeToolExecutor<'a> {
    pub fn try_new(sources: Vec<&'a dyn ToolExecutor>) -> Result<Self, String> {
        let mut ids = BTreeSet::new();
        let mut registrations = Vec::new();
        for source in &sources {
            for registration in source.registrations() {
                if !ids.insert(registration.spec.id.clone()) {
                    return Err(format!(
                        "tool id is already registered: {}",
                        registration.spec.id
                    ));
                }
                registrations.push(registration.clone());
            }
        }
        Ok(Self {
            sources,
            registrations,
        })
    }
}

impl ToolExecutor for CompositeToolExecutor<'_> {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let Some(source) = self.sources.iter().find(|source| {
            source.registrations().iter().any(|registration| {
                registration.spec.id == invocation.tool_id
                    && registration.spec.name == invocation.tool_name
            })
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "tool is not registered in any executor",
                ))
            });
        };
        source.execute(invocation, cancellation)
    }
}
