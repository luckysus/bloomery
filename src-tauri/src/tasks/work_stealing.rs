use super::model::{TaskError, TaskRecord};
use super::repository;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskClaim {
    pub task: TaskRecord,
    pub owner: String,
    pub claim_token: Uuid,
    pub lease_expires_at: DateTime<Utc>,
}

pub fn claim_next(
    connection: &mut Connection,
    workspace_id: &str,
    owner: &str,
    now: DateTime<Utc>,
    lease: Duration,
) -> Result<Option<TaskClaim>, TaskError> {
    if owner.trim().is_empty() || lease <= Duration::zero() {
        return Err(TaskError::new(
            "invalid_task_claim",
            "owner and lease are required",
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    transaction
        .execute(
            "UPDATE background_tasks
             SET state = 'queued', owner = NULL, claim_token = NULL, lease_expires_at = NULL
             WHERE workspace_id = ?1 AND state = 'running'
               AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?2",
            params![workspace_id, now.to_rfc3339()],
        )
        .map_err(storage_error)?;
    let raw_id = transaction
        .query_row(
            "SELECT id FROM background_tasks
             WHERE workspace_id = ?1 AND state = 'queued' AND owner IS NULL
             ORDER BY created_at ASC, id ASC LIMIT 1",
            params![workspace_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage_error)?;
    let Some(raw_id) = raw_id else {
        transaction.commit().map_err(storage_error)?;
        return Ok(None);
    };
    let token = Uuid::new_v4();
    let expires = now + lease;
    transaction
        .execute(
            "INSERT INTO task_claim_tokens (token, task_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![token.to_string(), raw_id, now.to_rfc3339()],
        )
        .map_err(storage_error)?;
    let changed = transaction
        .execute(
            "UPDATE background_tasks
             SET state = 'running', owner = ?1, claim_token = ?2, lease_expires_at = ?3,
                 attempt = attempt + 1, started_at = ?4, updated_at = ?4
             WHERE workspace_id = ?5 AND id = ?6 AND state = 'queued' AND owner IS NULL",
            params![
                owner,
                token.to_string(),
                expires.to_rfc3339(),
                now.to_rfc3339(),
                workspace_id,
                raw_id
            ],
        )
        .map_err(storage_error)?;
    if changed != 1 {
        return Err(TaskError::new(
            "task_claim_conflict",
            "task was claimed by another worker",
        ));
    }
    let task = repository::get(
        &transaction,
        workspace_id,
        Uuid::parse_str(&raw_id)
            .map_err(|_| TaskError::new("storage_error", "claimed task id is invalid"))?,
    )?
    .ok_or_else(|| TaskError::new("storage_error", "claimed task disappeared"))?;
    transaction.commit().map_err(storage_error)?;
    Ok(Some(TaskClaim {
        task,
        owner: owner.to_string(),
        claim_token: token,
        lease_expires_at: expires,
    }))
}

pub fn complete(
    connection: &mut Connection,
    workspace_id: &str,
    task_id: Uuid,
    owner: &str,
    claim_token: Uuid,
    now: DateTime<Utc>,
) -> Result<TaskRecord, TaskError> {
    let changed = connection
        .execute(
            "UPDATE background_tasks
             SET state = 'completed', progress = 100, owner = NULL, claim_token = NULL,
                 lease_expires_at = NULL, finished_at = ?1, updated_at = ?1
             WHERE workspace_id = ?2 AND id = ?3 AND state = 'running'
               AND owner = ?4 AND claim_token = ?5 AND lease_expires_at > ?1",
            params![
                now.to_rfc3339(),
                workspace_id,
                task_id.to_string(),
                owner,
                claim_token.to_string()
            ],
        )
        .map_err(storage_error)?;
    if changed != 1 {
        return Err(TaskError::new(
            "task_claim_mismatch",
            "owner, token or lease does not match",
        ));
    }
    repository::get(connection, workspace_id, task_id)?
        .ok_or_else(|| TaskError::new("storage_error", "completed task disappeared"))
}

fn storage_error(error: rusqlite::Error) -> TaskError {
    TaskError::new("storage_error", error.to_string())
}
