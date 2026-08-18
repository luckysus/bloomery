use super::types::{DatabaseConnectionInput, DatabaseConnectionSummary, DatabaseQuerySubmitInput};
use crate::{
    db::{current_workspace_id, with_conn, DbState},
    storage::{
        repositories::database_connections::DatabaseConnectionRecord,
        secrets::{self, SecretRef, SecretStore, SecretValue},
    },
};
use uuid::Uuid;

pub(crate) const PASSWORD_CREDENTIAL: &str = "password";

pub(super) fn summary(
    record: &DatabaseConnectionRecord,
    store: &dyn SecretStore,
) -> DatabaseConnectionSummary {
    DatabaseConnectionSummary {
        id: record.id.to_string(),
        display_name: record.display_name.clone(),
        host: record.host.clone(),
        port: record.port,
        username: record.username.clone(),
        timeout_ms: record.timeout_ms,
        enabled: record.enabled,
        secret_configured: secret_configured(store, record.id),
        last_checked_at: record.last_checked_at.clone(),
        last_latency_ms: record.last_latency_ms,
        last_version: record.last_version.clone(),
        last_error: record.last_error.clone(),
    }
}

pub(super) fn secret_configured(store: &dyn SecretStore, id: Uuid) -> bool {
    SecretRef::new(id, PASSWORD_CREDENTIAL)
        .and_then(|reference| secrets::status(store, &reference).map(|status| status.configured))
        .unwrap_or(false)
}

pub(super) fn normalized(
    input: DatabaseConnectionInput,
    existing: Option<Uuid>,
) -> Result<(Uuid, DatabaseConnectionRecord), String> {
    let display_name = input.display_name.trim().to_string();
    let host = input.host.trim().to_string();
    let username = input.username.trim().to_string();
    if display_name.is_empty() || host.is_empty() || username.is_empty() {
        return Err("display name, host, and username are required".to_string());
    }
    let id = match (&input.id, existing) {
        (Some(value), _) => super::types::parse_id(value)?,
        (None, Some(id)) => id,
        (None, None) => Uuid::new_v4(),
    };
    Ok((
        id,
        DatabaseConnectionRecord {
            id,
            display_name,
            host,
            port: input.port.unwrap_or(crate::database::DEFAULT_PORT),
            // ponytail: 库名由用户登录后自选，DB 列置空表示"不指定/连默认库"
            database_name: String::new(),
            username,
            timeout_ms: input
                .timeout_ms
                .unwrap_or(crate::database::DEFAULT_TIMEOUT_MS)
                .clamp(1_000, 60_000),
            enabled: input.enabled.unwrap_or(true),
            last_checked_at: None,
            last_latency_ms: None,
            last_version: None,
            last_error: None,
        },
    ))
}

pub(super) fn password(store: &dyn SecretStore, id: Uuid) -> Result<String, String> {
    let reference = SecretRef::new(id, PASSWORD_CREDENTIAL).map_err(|error| error.to_string())?;
    store
        .get(&reference)
        .map(|value| value.expose().to_string())
        .map_err(|_| "database password is not configured".to_string())
}

pub(super) fn set_password(store: &dyn SecretStore, id: Uuid, value: &str) -> Result<(), String> {
    let secret = SecretValue::new(value).map_err(|error| error.to_string())?;
    let reference = SecretRef::new(id, PASSWORD_CREDENTIAL).map_err(|error| error.to_string())?;
    store
        .set(&reference, &secret)
        .map_err(|error| error.to_string())
}

pub(super) fn delete_password(store: &dyn SecretStore, id: Uuid) -> Result<(), String> {
    let reference = SecretRef::new(id, PASSWORD_CREDENTIAL).map_err(|error| error.to_string())?;
    match store.delete(&reference) {
        Ok(()) => Ok(()),
        Err(error) if error.is_not_found() => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn load_record(
    db: &tauri::State<'_, DbState>,
    id: Uuid,
) -> Result<DatabaseConnectionRecord, String> {
    with_conn(db, |connection| {
        crate::storage::repositories::database_connections::get(
            connection,
            current_workspace_id(),
            id,
        )?
        .ok_or_else(|| "database connection not found".to_string())
    })
}

pub(super) fn load_enabled_record(
    db: &tauri::State<'_, DbState>,
    store: &dyn SecretStore,
    id: Uuid,
) -> Result<DatabaseConnectionRecord, String> {
    let _ = store;
    let record = load_record(db, id)?;
    if !record.enabled {
        return Err("database connection is disabled".to_string());
    }
    Ok(record)
}

pub(super) struct PreparedQuerySubmission {
    pub connection_id: Uuid,
    pub sql: String,
    pub database: Option<String>,
    pub row_limit: u64,
}

pub(super) fn validate_submission(
    input: &DatabaseQuerySubmitInput,
) -> Result<PreparedQuerySubmission, String> {
    let connection_id = super::types::parse_id(input.connection_id.trim())?;
    let sql = crate::database::query::normalize_query(&input.sql)?;
    let row_limit = crate::database::query::clamp_row_limit(input.row_limit);
    let database = input
        .database
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(PreparedQuerySubmission {
        connection_id,
        sql,
        database,
        row_limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::secrets::{SecretError, SecretValue};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeSecretStore {
        values: Mutex<HashMap<String, String>>,
    }

    impl SecretStore for FakeSecretStore {
        fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
            self.values
                .lock()
                .unwrap()
                .insert(reference.account(), value.expose().to_string());
            Ok(())
        }

        fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
            self.values
                .lock()
                .unwrap()
                .get(&reference.account())
                .cloned()
                .map(SecretValue::new)
                .transpose()?
                .ok_or(SecretError::not_found())
        }

        fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
            self.values.lock().unwrap().remove(&reference.account());
            Ok(())
        }
    }

    fn input() -> DatabaseConnectionInput {
        DatabaseConnectionInput {
            id: None,
            display_name: "3 号高炉".to_string(),
            host: " 192.168.1.10 ".to_string(),
            port: None,
            username: " sa ".to_string(),
            password: Some("secret-value".to_string()),
            timeout_ms: None,
            enabled: None,
        }
    }

    #[test]
    fn normalized_applies_defaults_and_trims() {
        let (_, record) = normalized(input(), None).expect("normalize");
        assert_eq!(record.host, "192.168.1.10");
        assert_eq!(record.username, "sa");
        assert_eq!(record.port, crate::database::DEFAULT_PORT);
        assert_eq!(record.timeout_ms, crate::database::DEFAULT_TIMEOUT_MS);
        assert!(record.enabled);
        // 库名不再要求填写；DB 列置空表示"不指定/连默认库"
        assert!(record.database_name.is_empty());
    }

    #[test]
    fn normalized_rejects_missing_fields() {
        let missing_host = DatabaseConnectionInput {
            host: "  ".to_string(),
            ..input()
        };
        assert!(normalized(missing_host, None).is_err());
    }

    #[test]
    fn normalized_reuses_existing_id_when_updating() {
        let existing = Uuid::new_v4();
        let update = DatabaseConnectionInput {
            id: None,
            ..input()
        };
        let (id, _) = normalized(update, Some(existing)).expect("normalize");
        assert_eq!(id, existing);
    }

    #[test]
    fn summary_reports_secret_without_exposing_it() {
        let store = FakeSecretStore::default();
        let (id, record) = normalized(input(), None).expect("normalize");
        let summary_before = summary(&record, &store);
        assert!(!summary_before.secret_configured);

        set_password(&store, id, "secret-value").expect("set password");
        let summary_after = summary(&record, &store);
        assert!(summary_after.secret_configured);
        assert!(!format!("{summary_after:?}").contains("secret-value"));
    }

    #[test]
    fn validate_submission_rejects_guard_violations() {
        let input = DatabaseQuerySubmitInput {
            connection_id: "11111111-1111-1111-1111-111111111111".to_string(),
            database: None,
            sql: "DELETE FROM heats".to_string(),
            row_limit: None,
        };
        assert!(validate_submission(&input).is_err());
    }

    #[test]
    fn validate_submission_normalizes_sql_and_limit() {
        let input = DatabaseQuerySubmitInput {
            connection_id: " 11111111-1111-1111-1111-111111111111 ".to_string(),
            database: Some("SteelWorks".to_string()),
            sql: " SELECT 1; ".to_string(),
            row_limit: Some(9_999_999),
        };
        let prepared = validate_submission(&input).expect("validate");
        assert_eq!(prepared.sql, "SELECT 1");
        assert_eq!(prepared.database.as_deref(), Some("SteelWorks"));
        assert_eq!(prepared.row_limit, 5_000);
        assert_eq!(
            prepared.connection_id.to_string(),
            "11111111-1111-1111-1111-111111111111"
        );
    }

    #[test]
    fn validate_submission_rejects_bad_uuid() {
        let input = DatabaseQuerySubmitInput {
            connection_id: "not-a-uuid".to_string(),
            database: None,
            sql: "SELECT 1".to_string(),
            row_limit: None,
        };
        assert!(validate_submission(&input).is_err());
    }
}
