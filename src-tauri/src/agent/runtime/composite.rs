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
                registration.spec.validate_schema()?;
                registrations.push(registration.clone());
            }
        }
        // Keep the tool prefix deterministic. Sources may be assembled from
        // dynamic MCP responses, so source order is not a stable contract.
        registrations.sort_by(|left, right| {
            left.spec
                .id
                .cmp(&right.spec.id)
                .then_with(|| left.spec.name.cmp(&right.spec.name))
        });
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
