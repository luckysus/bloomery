use bloomery::permissions::{ParameterScope, PermissionAction, PermissionRule, RuleEffect};
use bloomery::storage::{migrations::migrate, repositories::permissions};
use bloomery::tools::{ToolId, ToolSource, ToolVersion};
use rusqlite::Connection;
use serde_json::json;
use std::collections::BTreeMap;
use uuid::Uuid;

fn rule(id: Uuid) -> PermissionRule {
    PermissionRule {
        id,
        tool_id: ToolId::new("builtin.write_file").unwrap(),
        tool_version: ToolVersion::parse("1.0.0").unwrap(),
        source: ToolSource::Builtin,
        action: PermissionAction::Execute,
        scope: ParameterScope::Fields(BTreeMap::from([("path".to_string(), json!("draft.txt"))])),
        effect: RuleEffect::Allow,
    }
}

#[test]
fn permission_rules_round_trip_with_workspace_isolation() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let id = Uuid::new_v4();
    let original = rule(id);

    permissions::insert(&mut connection, "workspace-a", &original).unwrap();

    let loaded = permissions::list(&connection, "workspace-a").unwrap();
    assert_eq!(loaded, vec![original]);
    assert!(permissions::list(&connection, "workspace-b")
        .unwrap()
        .is_empty());
}

#[test]
fn revoked_permission_rules_are_not_returned_as_active() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let id = Uuid::new_v4();
    permissions::insert(&mut connection, "workspace-a", &rule(id)).unwrap();

    permissions::revoke(&mut connection, "workspace-a", id).unwrap();

    assert!(permissions::list(&connection, "workspace-a")
        .unwrap()
        .is_empty());
}
