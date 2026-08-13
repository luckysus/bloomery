use super::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolInvocation,
    ToolRegistration,
};
use crate::domains::DomainManifest;

/// Restricts a runtime tool snapshot to the active domain package's built-in allowlist.
/// The inner executor remains responsible for the global permission decision.
pub struct DomainToolExecutor<'a, T: ?Sized> {
    inner: &'a T,
    registrations: Vec<ToolRegistration>,
}

impl<'a, T: ToolExecutor + ?Sized> DomainToolExecutor<'a, T> {
    pub fn new(inner: &'a T, domain: Option<&DomainManifest>) -> Self {
        let domains = domain.into_iter().cloned().collect::<Vec<_>>();
        Self::new_for_domains(inner, &domains)
    }

    pub fn new_for_domains(inner: &'a T, domains: &[DomainManifest]) -> Self {
        let registrations = inner
            .registrations()
            .iter()
            .filter(|registration| {
                domains.is_empty() || {
                    registration.spec.id.starts_with("mcp.")
                        || domains.iter().any(|manifest| {
                            manifest
                                .builtin_tool_allowlist
                                .iter()
                                .any(|id| id == &registration.spec.id)
                        })
                }
            })
            .cloned()
            .collect();
        Self {
            inner,
            registrations,
        }
    }
}

impl<T: ToolExecutor + ?Sized> ToolExecutor for DomainToolExecutor<'_, T> {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let allowed = self.registrations.iter().any(|registration| {
            registration.spec.id == invocation.tool_id
                && registration.spec.name == invocation.tool_name
        });
        if allowed {
            self.inner.execute(invocation, cancellation)
        } else {
            Box::pin(async {
                Err(ToolExecutionError::new(
                    "domain_tool_not_allowed",
                    "tool is not allowed by the active domain package",
                ))
            })
        }
    }
}
