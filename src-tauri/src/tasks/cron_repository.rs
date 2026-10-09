use super::cron::CronPlan;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: Uuid,
    pub workspace_id: String,
    pub expression: String,
    pub timezone: String,
    pub prompt: String,
    pub identity: String,
    pub recurring: bool,
    pub durable: bool,
    pub enabled: bool,
    pub conversation_id: Option<Uuid>,
    pub agent_id: String,
    pub next_run_at_utc: String,
    pub last_slot_at_utc: Option<String>,
    pub last_error: Option<String>,
    pub last_run_id: Option<Uuid>,
    pub last_run_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronEvent {
    pub event_id: Uuid,
    pub workspace_id: String,
    pub job_id: Uuid,
    pub slot_at_utc: String,
    pub identity: String,
    pub prompt: String,
    pub conversation_id: Option<Uuid>,
    pub agent_id: String,
    pub run_id: Option<Uuid>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAgentSchedule {
    pub id: Option<Uuid>,
    pub conversation_id: Uuid,
    pub agent_id: String,
    pub expression: String,
    pub timezone: String,
    pub prompt: String,
    pub enabled: bool,
    #[serde(default = "default_recurring")]
    pub recurring: bool,
}

fn default_recurring() -> bool {
    true
}

pub fn list(connection: &Connection, workspace_id: &str) -> Result<Vec<CronJob>, String> {
    let mut statement = connection.prepare(
        "SELECT j.id, j.workspace_id, j.expression, j.timezone, j.prompt, j.identity, j.recurring,
                j.durable, j.enabled, j.conversation_id, j.agent_id, j.next_run_at_utc, j.last_slot_at_utc,
                (SELECT o.error_message FROM cron_outbox o WHERE o.workspace_id = j.workspace_id AND o.job_id = j.id ORDER BY o.slot_at_utc DESC LIMIT 1),
                (SELECT o.run_id FROM cron_outbox o WHERE o.workspace_id = j.workspace_id AND o.job_id = j.id ORDER BY o.slot_at_utc DESC LIMIT 1),
                (SELECT r.state FROM cron_outbox o LEFT JOIN agent_runs r ON r.workspace_id = o.workspace_id AND r.id = o.run_id WHERE o.workspace_id = j.workspace_id AND o.job_id = j.id ORDER BY o.slot_at_utc DESC LIMIT 1)
         FROM cron_jobs j WHERE j.workspace_id = ?1 ORDER BY j.next_run_at_utc, j.id"
    ).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], decode_job)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn save(
    connection: &mut Connection,
    workspace_id: &str,
    request: SaveAgentSchedule,
    now: DateTime<Utc>,
) -> Result<CronJob, String> {
    let plan = CronPlan::validate(&request.expression, &request.timezone)?;
    super::model::validate_identifier("workspace_id", workspace_id)
        .map_err(|error| error.to_string())?;
    super::model::validate_identifier("agent_id", &request.agent_id)
        .map_err(|error| error.to_string())?;
    let profile = crate::agent::profiles::get(connection, workspace_id, &request.agent_id)?
        .ok_or_else(|| "schedule Agent profile was not found".to_string())?;
    if request.enabled && !profile.enabled {
        return Err("enable the selected Agent before enabling its schedule".to_string());
    }
    if request.prompt.trim().is_empty() || request.prompt.len() > 64 * 1024 {
        return Err("schedule prompt must contain 1 to 65536 bytes".to_string());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE workspace_id = ?1 AND id = ?2 AND archived = 0)",
        params![workspace_id, request.conversation_id.to_string()], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    if !exists {
        return Err("schedule conversation was not found or is archived".to_string());
    }
    let id = request.id.unwrap_or_else(Uuid::new_v4);
    let next = plan.next_after(now)?.to_rfc3339();
    if request.id.is_some() {
        let changed = transaction.execute(
            "UPDATE cron_jobs SET expression = ?1, timezone = ?2, prompt = ?3, identity = ?4,
                recurring = ?5, durable = 1, enabled = ?6, conversation_id = ?7,
                agent_id = ?4, next_run_at_utc = ?8, updated_at = ?9 WHERE workspace_id = ?10 AND id = ?11",
            params![plan.expression, plan.timezone, request.prompt.trim(), request.agent_id, request.recurring,
                request.enabled, request.conversation_id.to_string(), next, now.to_rfc3339(), workspace_id, id.to_string()],
        ).map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("schedule was not found".to_string());
        }
        cancel_unstarted(&transaction, workspace_id, id, "schedule_updated")?;
    } else {
        transaction.execute(
            "INSERT INTO cron_jobs (id, workspace_id, expression, timezone, prompt, identity,
                recurring, durable, enabled, conversation_id, agent_id, next_run_at_utc, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?6, ?10, ?11, ?11)",
            params![id.to_string(), workspace_id, plan.expression, plan.timezone, request.prompt.trim(), request.agent_id,
                request.recurring, request.enabled, request.conversation_id.to_string(), next, now.to_rfc3339()],
        ).map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    list(connection, workspace_id)?
        .into_iter()
        .find(|job| job.id == id)
        .ok_or_else(|| "saved schedule could not be read".to_string())
}

pub fn delete(connection: &mut Connection, workspace_id: &str, id: Uuid) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let changed = transaction
        .execute(
            "DELETE FROM cron_jobs WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("schedule was not found".to_string());
    }
    cancel_unstarted(&transaction, workspace_id, id, "schedule_deleted")?;
    transaction.commit().map_err(|error| error.to_string())
}

pub fn set_enabled(
    connection: &mut Connection,
    workspace_id: &str,
    id: Uuid,
    enabled: bool,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let (expression, timezone, agent_id, conversation_id) = transaction
        .query_row(
            "SELECT expression, timezone, agent_id, conversation_id FROM cron_jobs WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "schedule was not found".to_string())?;
    if enabled && conversation_id.is_some() {
        crate::agent::profiles::get(&transaction, workspace_id, &agent_id)?
            .filter(|profile| profile.enabled)
            .ok_or_else(|| "schedule Agent is missing or disabled".to_string())?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE workspace_id = ?1 AND id = ?2 AND archived = 0)",
            params![workspace_id, conversation_id], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if !exists {
            return Err("schedule conversation was deleted or archived".to_string());
        }
    }
    let next = CronPlan::validate(&expression, &timezone)?
        .next_after(now)?
        .to_rfc3339();
    transaction.execute(
        "UPDATE cron_jobs SET enabled = ?1, next_run_at_utc = ?2, updated_at = ?3 WHERE workspace_id = ?4 AND id = ?5",
        params![enabled, next, now.to_rfc3339(), workspace_id, id.to_string()],
    ).map_err(|error| error.to_string())?;
    if !enabled {
        cancel_unstarted(&transaction, workspace_id, id, "schedule_disabled")?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn cancel_unstarted(
    connection: &Connection,
    workspace_id: &str,
    id: Uuid,
    reason: &str,
) -> Result<(), String> {
    connection.execute(
        "UPDATE cron_outbox SET acknowledged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), error_message = ?3
         WHERE workspace_id = ?1 AND job_id = ?2 AND acknowledged_at IS NULL
           AND (run_id IS NULL OR NOT EXISTS (SELECT 1 FROM agent_runs r
             WHERE r.workspace_id = cron_outbox.workspace_id AND r.id = cron_outbox.run_id))",
        params![workspace_id, id.to_string(), reason],
    ).map(|_| ()).map_err(|error| error.to_string())
}

// Generic scheduler callers retain this API; desktop Agent plans use save()
// with an explicit existing conversation and profile.
pub fn schedule(
    connection: &Connection,
    workspace_id: &str,
    expression: &str,
    timezone: &str,
    prompt: &str,
    identity: &str,
    recurring: bool,
    durable: bool,
    now: DateTime<Utc>,
) -> Result<CronJob, String> {
    let plan = CronPlan::validate(expression, timezone)?;
    if workspace_id.trim().is_empty() || identity.trim().is_empty() || prompt.trim().is_empty() {
        return Err("workspace, identity and prompt are required".to_string());
    }
    let id = Uuid::new_v4();
    let next = plan.next_after(now)?.to_rfc3339();
    connection
        .execute(
            "INSERT INTO cron_jobs (id, workspace_id, expression, timezone, prompt, identity,
            recurring, durable, next_run_at_utc, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                id.to_string(),
                workspace_id,
                plan.expression,
                plan.timezone,
                prompt,
                identity,
                recurring,
                durable,
                next,
                now.to_rfc3339()
            ],
        )
        .map_err(|error| error.to_string())?;
    list(connection, workspace_id)?
        .into_iter()
        .find(|job| job.id == id)
        .ok_or_else(|| "scheduled job could not be read".to_string())
}

pub fn tick(
    connection: &mut Connection,
    workspace_id: &str,
    now: DateTime<Utc>,
    capacity: usize,
) -> Result<Vec<CronEvent>, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let due = list(&transaction, workspace_id)?
        .into_iter()
        .filter(|job| job.enabled)
        .filter(|job| {
            DateTime::parse_from_rfc3339(&job.next_run_at_utc).is_ok_and(|slot| slot <= now)
        })
        .take(capacity)
        .collect::<Vec<_>>();
    let mut events = Vec::new();
    for job in due {
        let event = CronEvent {
            event_id: Uuid::new_v4(),
            workspace_id: workspace_id.to_string(),
            job_id: job.id,
            slot_at_utc: job.next_run_at_utc.clone(),
            identity: job.identity,
            prompt: job.prompt,
            conversation_id: job.conversation_id,
            agent_id: job.agent_id,
            run_id: None,
            error_message: None,
        };
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO cron_outbox (event_id, workspace_id, job_id, slot_at_utc, identity,
                prompt, conversation_id, agent_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![event.event_id.to_string(), workspace_id, job.id.to_string(), event.slot_at_utc, event.identity,
                event.prompt, event.conversation_id.map(|id| id.to_string()), event.agent_id, now.to_rfc3339()],
        ).map_err(|error| error.to_string())?;
        let next = CronPlan::validate(&job.expression, &job.timezone)?
            .next_after(now)?
            .to_rfc3339();
        transaction.execute(
            "UPDATE cron_jobs SET last_slot_at_utc = ?1, next_run_at_utc = ?2, enabled = ?3, updated_at = ?4
             WHERE workspace_id = ?5 AND id = ?6",
            params![job.next_run_at_utc, next, job.recurring, now.to_rfc3339(), workspace_id, job.id.to_string()],
        ).map_err(|error| error.to_string())?;
        if inserted == 1 {
            events.push(event);
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(events)
}

pub fn pending(connection: &Connection, workspace_id: &str) -> Result<Vec<CronEvent>, String> {
    let mut statement = connection.prepare(
        "SELECT event_id, job_id, slot_at_utc, identity, prompt, conversation_id, agent_id, run_id, error_message
         FROM cron_outbox WHERE workspace_id = ?1 AND acknowledged_at IS NULL ORDER BY slot_at_utc, event_id",
    ).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], |row| {
            Ok(CronEvent {
                event_id: parse_id(row.get(0)?)?,
                workspace_id: workspace_id.to_string(),
                job_id: parse_id(row.get(1)?)?,
                slot_at_utc: row.get(2)?,
                identity: row.get(3)?,
                prompt: row.get(4)?,
                conversation_id: row.get::<_, Option<String>>(5)?.map(parse_id).transpose()?,
                agent_id: row.get(6)?,
                run_id: row.get::<_, Option<String>>(7)?.map(parse_id).transpose()?,
                error_message: row.get(8)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// Reserve a stable identity before starting. A crash before run creation
/// reuses the same id, while a recorded run is never started again.
pub fn reserve_run(
    connection: &Connection,
    workspace_id: &str,
    event_id: Uuid,
) -> Result<Uuid, String> {
    connection
        .execute(
            "UPDATE cron_outbox SET run_id = event_id WHERE workspace_id = ?1 AND event_id = ?2
         AND acknowledged_at IS NULL AND run_id IS NULL",
            params![workspace_id, event_id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    let id: String = connection.query_row(
        "SELECT run_id FROM cron_outbox WHERE workspace_id = ?1 AND event_id = ?2 AND acknowledged_at IS NULL",
        params![workspace_id, event_id.to_string()], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    Uuid::parse_str(&id).map_err(|error| error.to_string())
}

pub fn record_error(
    connection: &Connection,
    workspace_id: &str,
    event_id: Uuid,
    error: &str,
) -> Result<(), String> {
    let error = crate::diagnostics::observability::redact_line(error);
    connection.execute(
        "UPDATE cron_outbox SET error_message = ?3, acknowledged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE workspace_id = ?1 AND event_id = ?2 AND acknowledged_at IS NULL",
        params![workspace_id, event_id.to_string(), error.chars().take(2_000).collect::<String>()],
    ).map(|_| ()).map_err(|error| error.to_string())
}

pub fn acknowledge(
    connection: &Connection,
    workspace_id: &str,
    event_id: Uuid,
) -> Result<bool, String> {
    connection
        .execute(
            "UPDATE cron_outbox SET acknowledged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE workspace_id = ?1 AND event_id = ?2 AND acknowledged_at IS NULL",
            params![workspace_id, event_id.to_string()],
        )
        .map(|changed| changed == 1)
        .map_err(|error| error.to_string())
}

fn parse_id(id: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn decode_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<CronJob> {
    Ok(CronJob {
        id: parse_id(row.get(0)?)?,
        workspace_id: row.get(1)?,
        expression: row.get(2)?,
        timezone: row.get(3)?,
        prompt: row.get(4)?,
        identity: row.get(5)?,
        recurring: row.get(6)?,
        durable: row.get(7)?,
        enabled: row.get(8)?,
        conversation_id: row.get::<_, Option<String>>(9)?.map(parse_id).transpose()?,
        agent_id: row.get(10)?,
        next_run_at_utc: row.get(11)?,
        last_slot_at_utc: row.get(12)?,
        last_error: row.get(13)?,
        last_run_id: row
            .get::<_, Option<String>>(14)?
            .map(parse_id)
            .transpose()?,
        last_run_state: row.get(15)?,
    })
}
