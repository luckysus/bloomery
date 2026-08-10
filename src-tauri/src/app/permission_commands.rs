use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::permissions::PermissionRule;
use uuid::Uuid;

#[tauri::command]
pub fn list_permission_rules(db: tauri::State<DbState>) -> Result<Vec<PermissionRule>, String> {
    with_conn(&db, |connection| {
        crate::storage::repositories::permissions::list(connection, current_workspace_id())
    })
}

#[tauri::command]
pub fn revoke_permission_rule(db: tauri::State<DbState>, rule_id: String) -> Result<(), String> {
    let rule_id = parse_rule_id(&rule_id)?;
    with_conn_mut(&db, |connection| {
        crate::storage::repositories::permissions::revoke(
            connection,
            current_workspace_id(),
            rule_id,
        )
    })
}

fn parse_rule_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value.trim()).map_err(|_| "rule_id must be a UUID".to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_rule_id;
    use uuid::Uuid;

    #[test]
    fn parses_trimmed_rule_ids() {
        let id = Uuid::new_v4();
        assert_eq!(parse_rule_id(&format!(" {id} ")).unwrap(), id);
    }

    #[test]
    fn rejects_invalid_rule_ids() {
        assert!(parse_rule_id("not-a-uuid").is_err());
    }
}
