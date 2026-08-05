use bloomery::agent::protocol::{PermissionDecision, PermissionRisk};
use bloomery::permissions::{
    DenialReason, ParameterScope, PermissionAction, PermissionPolicy, PolicyDecision,
};
use bloomery::tools::{ConcurrencyPolicy, ToolDefinition, ToolId, ToolSource, ToolVersion};
use serde_json::json;
use std::collections::BTreeSet;
use std::time::Duration;

fn tool(id: &str, risk: PermissionRisk, version: &str, source: ToolSource) -> ToolDefinition {
    ToolDefinition {
        id: ToolId::new(id).unwrap(),
        version: ToolVersion::parse(version).unwrap(),
        name: id.rsplit('.').next().unwrap_or(id).to_string(),
        description: "Permission test tool".to_string(),
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        risk,
        read_only: risk == PermissionRisk::Automatic,
        concurrency: ConcurrencyPolicy::ParallelRead,
        timeout: Duration::from_secs(1),
        source,
        domains: BTreeSet::new(),
    }
}

fn builtin_source() -> ToolSource {
    ToolSource::Builtin
}

fn request(
    policy: &mut PermissionPolicy,
    definition: &ToolDefinition,
    arguments: serde_json::Value,
) -> bloomery::permissions::PermissionRequest {
    match policy.evaluate(definition, arguments) {
        PolicyDecision::RequireConfirmation(request) => request,
        other => panic!("expected confirmation request, got {other:?}"),
    }
}

#[test]
fn automatic_read_only_tools_are_allowed_without_confirmation() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.search",
        PermissionRisk::Automatic,
        "1.0.0",
        builtin_source(),
    );

    assert!(matches!(
        policy.evaluate(&definition, json!({"query": "steel"})),
        PolicyDecision::AllowAutomatic
    ));
}

#[test]
fn confirmation_tools_request_explicit_user_approval() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );

    let request = request(&mut policy, &definition, json!({"path": "draft.txt"}));
    assert_eq!(request.tool_id, definition.id);
    assert_eq!(request.action, PermissionAction::Execute);
}

#[test]
fn dangerous_tools_are_disabled_by_default() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.shell",
        PermissionRisk::Dangerous,
        "1.0.0",
        builtin_source(),
    );

    assert!(matches!(
        policy.evaluate(&definition, json!({"command": "echo safe"})),
        PolicyDecision::Deny(DenialReason::DangerousDisabled)
    ));
}

#[test]
fn once_approval_is_consumed_after_one_matching_execution() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let arguments = json!({"path": "draft.txt"});
    let permission = request(&mut policy, &definition, arguments.clone());
    policy
        .resolve(&permission, PermissionDecision::AllowOnce)
        .unwrap();

    assert!(matches!(
        policy.evaluate(&definition, arguments.clone()),
        PolicyDecision::AllowAutomatic
    ));
    assert!(matches!(
        policy.evaluate(&definition, arguments),
        PolicyDecision::RequireConfirmation(_)
    ));
}

#[test]
fn session_approval_survives_until_session_is_cleared() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let arguments = json!({"path": "draft.txt"});
    let permission = request(&mut policy, &definition, arguments.clone());
    policy
        .resolve(&permission, PermissionDecision::AllowSession)
        .unwrap();

    assert!(matches!(
        policy.evaluate(&definition, arguments.clone()),
        PolicyDecision::AllowAutomatic
    ));
    policy.clear_session();
    assert!(matches!(
        policy.evaluate(&definition, arguments),
        PolicyDecision::RequireConfirmation(_)
    ));
}

#[test]
fn always_approval_is_persistent_and_deny_creates_a_deny_rule() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let permission = request(&mut policy, &definition, json!({"path": "draft.txt"}));
    policy
        .resolve(&permission, PermissionDecision::AllowAlways)
        .unwrap();
    assert_eq!(policy.persistent_rules().len(), 1);
    assert!(matches!(
        policy.evaluate(&definition, json!({"path": "draft.txt"})),
        PolicyDecision::AllowAutomatic
    ));

    let mut deny_policy = PermissionPolicy::new();
    let denied = request(&mut deny_policy, &definition, json!({"path": "draft.txt"}));
    deny_policy
        .resolve(&denied, PermissionDecision::Deny)
        .unwrap();
    assert!(matches!(
        deny_policy.evaluate(&definition, json!({"path": "draft.txt"})),
        PolicyDecision::Deny(DenialReason::ExplicitRule { .. })
    ));
}

#[test]
fn revoked_rules_no_longer_grant_access() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let permission = request(&mut policy, &definition, json!({"path": "draft.txt"}));
    policy
        .resolve(&permission, PermissionDecision::AllowAlways)
        .unwrap();
    let rule_id = policy.persistent_rules()[0].id;
    policy.revoke(rule_id).unwrap();

    assert!(matches!(
        policy.evaluate(&definition, json!({"path": "draft.txt"})),
        PolicyDecision::RequireConfirmation(_)
    ));
}

#[test]
fn tool_version_and_source_changes_do_not_match_old_rules() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let permission = request(&mut policy, &definition, json!({"path": "draft.txt"}));
    policy
        .resolve(&permission, PermissionDecision::AllowAlways)
        .unwrap();

    let changed_version = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "2.0.0",
        builtin_source(),
    );
    assert!(matches!(
        policy.evaluate(&changed_version, json!({"path": "draft.txt"})),
        PolicyDecision::RequireConfirmation(_)
    ));

    let changed_source = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        ToolSource::Mcp {
            server_id: "other".to_string(),
            server_version: ToolVersion::parse("1.0.0").unwrap(),
        },
    );
    assert!(matches!(
        policy.evaluate(&changed_source, json!({"path": "draft.txt"})),
        PolicyDecision::RequireConfirmation(_)
    ));
}

#[test]
fn parameter_scopes_match_only_the_declared_argument_fields() {
    let mut policy = PermissionPolicy::new();
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let permission = request(&mut policy, &definition, json!({"path": "draft.txt"}));
    policy
        .resolve_with_scope(
            &permission,
            PermissionDecision::AllowAlways,
            ParameterScope::fields(json!({"path": "draft.txt"})).unwrap(),
        )
        .unwrap();

    assert!(matches!(
        policy.evaluate(
            &definition,
            json!({"path": "draft.txt", "encoding": "utf-8"})
        ),
        PolicyDecision::AllowAutomatic
    ));
    assert!(matches!(
        policy.evaluate(&definition, json!({"path": "public.txt"})),
        PolicyDecision::RequireConfirmation(_)
    ));
}

#[test]
fn persisted_rules_can_be_loaded_into_a_new_policy() {
    let definition = tool(
        "builtin.write_file",
        PermissionRisk::ConfirmationRequired,
        "1.0.0",
        builtin_source(),
    );
    let arguments = json!({"path": "draft.txt"});
    let mut original = PermissionPolicy::new();
    let permission = request(&mut original, &definition, arguments.clone());
    original
        .resolve(&permission, PermissionDecision::AllowAlways)
        .unwrap();
    let persisted = original.persistent_rules().to_vec();

    let mut restored = PermissionPolicy::new();
    restored.load_persistent_rules(persisted);

    assert!(matches!(
        restored.evaluate(&definition, arguments),
        PolicyDecision::AllowAutomatic
    ));
}
