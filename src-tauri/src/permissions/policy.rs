use super::model::{
    DenialReason, ParameterScope, PermissionRequest, PermissionRule, PolicyDecision, RuleEffect,
};
use crate::agent::protocol::{PermissionDecision, PermissionRisk};
use crate::tools::ToolDefinition;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    UnknownRule { id: Uuid },
}

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRule { id } => write!(formatter, "permission rule is not active: {id}"),
        }
    }
}

impl std::error::Error for PolicyError {}

#[derive(Debug, Default)]
pub struct PermissionPolicy {
    dangerous_enabled: bool,
    once_rules: Vec<PermissionRule>,
    session_rules: Vec<PermissionRule>,
    persistent_rules: Vec<PermissionRule>,
}

impl PermissionPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_dangerous_enabled(&mut self, enabled: bool) {
        self.dangerous_enabled = enabled;
    }

    pub fn dangerous_enabled(&self) -> bool {
        self.dangerous_enabled
    }

    pub fn evaluate(
        &mut self,
        definition: &ToolDefinition,
        arguments: serde_json::Value,
    ) -> PolicyDecision {
        self.evaluate_request(PermissionRequest::from_tool(definition, arguments))
    }

    pub fn evaluate_request(&mut self, request: PermissionRequest) -> PolicyDecision {
        if let Some(rule) = self.find_rule(&request, RuleEffect::Deny) {
            let id = rule.id;
            self.consume_once(id);
            return PolicyDecision::Deny(DenialReason::ExplicitRule { rule_id: id });
        }
        if request.risk == PermissionRisk::Dangerous && !self.dangerous_enabled {
            return PolicyDecision::Deny(DenialReason::DangerousDisabled);
        }
        if let Some(rule) = self.find_rule(&request, RuleEffect::Allow) {
            let id = rule.id;
            self.consume_once(id);
            return PolicyDecision::AllowAutomatic;
        }
        if request.risk == PermissionRisk::Automatic && is_read_only(&request) {
            return PolicyDecision::AllowAutomatic;
        }
        PolicyDecision::RequireConfirmation(request)
    }

    pub fn resolve(
        &mut self,
        request: &PermissionRequest,
        decision: PermissionDecision,
    ) -> Result<(), PolicyError> {
        self.resolve_with_scope(
            request,
            decision,
            ParameterScope::exact(request.arguments.clone()),
        )
    }

    pub fn resolve_with_scope(
        &mut self,
        request: &PermissionRequest,
        decision: PermissionDecision,
        scope: ParameterScope,
    ) -> Result<(), PolicyError> {
        let rule = match decision {
            PermissionDecision::AllowOnce => {
                PermissionRule::for_request(request, scope, RuleEffect::Allow)
            }
            PermissionDecision::AllowSession => {
                PermissionRule::for_request(request, scope, RuleEffect::Allow)
            }
            PermissionDecision::AllowAlways => {
                PermissionRule::for_request(request, scope, RuleEffect::Allow)
            }
            PermissionDecision::Deny => {
                PermissionRule::for_request(request, scope, RuleEffect::Deny)
            }
        };
        match decision {
            PermissionDecision::AllowOnce | PermissionDecision::Deny => self.once_rules.push(rule),
            PermissionDecision::AllowSession => self.session_rules.push(rule),
            PermissionDecision::AllowAlways => self.persistent_rules.push(rule),
        }
        Ok(())
    }

    pub fn persistent_rules(&self) -> &[PermissionRule] {
        &self.persistent_rules
    }

    pub fn load_persistent_rules(&mut self, rules: impl IntoIterator<Item = PermissionRule>) {
        self.persistent_rules = rules.into_iter().collect();
    }

    pub fn revoke(&mut self, id: Uuid) -> Result<(), PolicyError> {
        let before = self.rule_count();
        self.once_rules.retain(|rule| rule.id != id);
        self.session_rules.retain(|rule| rule.id != id);
        self.persistent_rules.retain(|rule| rule.id != id);
        if self.rule_count() == before {
            return Err(PolicyError::UnknownRule { id });
        }
        Ok(())
    }

    pub fn clear_session(&mut self) {
        self.once_rules.clear();
        self.session_rules.clear();
    }

    fn find_rule(&self, request: &PermissionRequest, effect: RuleEffect) -> Option<PermissionRule> {
        self.once_rules
            .iter()
            .chain(self.session_rules.iter())
            .chain(self.persistent_rules.iter())
            .find(|rule| rule.effect == effect && rule.matches(request))
            .cloned()
    }

    fn consume_once(&mut self, id: Uuid) {
        self.once_rules.retain(|rule| rule.id != id);
    }

    fn rule_count(&self) -> usize {
        self.once_rules.len() + self.session_rules.len() + self.persistent_rules.len()
    }
}

fn is_read_only(request: &PermissionRequest) -> bool {
    request.read_only
}
