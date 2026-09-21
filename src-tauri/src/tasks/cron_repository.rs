use super::cron::CronPlan;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronJob {
    pub id: Uuid,
    pub workspace_id: String,
    pub expression: String,
    pub timezone: String,
    pub prompt: String,
    pub identity: String,
    pub recurring: bool,
    pub durable: bool,
    pub next_run_at_utc: String,
    pub last_slot_at_utc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronEvent {
    pub event_id: Uuid,
    pub workspace_id: String,
    pub job_id: Uuid,
    pub slot_at_utc: String,
    pub identity: String,
    pub prompt: String,
}

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
    let next_run_at_utc = plan.next_after(now)?.to_rfc3339();
    let job = CronJob {
        id: Uuid::new_v4(),
        workspace_id: workspace_id.to_string(),
        expression: plan.expression,
        timezone: plan.timezone,
        prompt: prompt.to_string(),
        identity: identity.to_string(),
        recurring,
        durable,
        next_run_at_utc,
        last_slot_at_utc: None,
    };
    connection
        .execute(
            "INSERT INTO cron_jobs
                (id, workspace_id, expression, timezone, prompt, identity,
                 recurring, durable, next_run_at_utc, last_slot_at_utc, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?10)",
            params![
                job.id.to_string(),
                job.workspace_id,
                job.expression,
                job.timezone,
                job.prompt,
                job.identity,
                i64::from(job.recurring),
                i64::from(job.durable),
                job.next_run_at_utc,
                now.to_rfc3339(),
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(job)
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
    let mut statement = transaction
        .prepare(
            "SELECT id, expression, timezone, prompt, identity, recurring, durable,
                    next_run_at_utc
             FROM cron_jobs
             WHERE workspace_id = ?1 AND next_run_at_utc <= ?2
             ORDER BY next_run_at_utc ASC, id ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id, now.to_rfc3339()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let due = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);

    let mut events = Vec::new();
    for (id, expression, timezone, prompt, identity, recurring, durable, slot) in due {
        if events.len() >= capacity {
            break;
        }
        let job_id = Uuid::parse_str(&id).map_err(|error| error.to_string())?;
        let event = CronEvent {
            event_id: Uuid::new_v4(),
            workspace_id: workspace_id.to_string(),
            job_id,
            slot_at_utc: slot.clone(),
            identity,
            prompt,
        };
        transaction
            .execute(
                "INSERT INTO cron_outbox
                    (event_id, workspace_id, job_id, slot_at_utc, identity, prompt, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.event_id.to_string(),
                    event.workspace_id,
                    event.job_id.to_string(),
                    event.slot_at_utc,
                    event.identity,
                    event.prompt,
                    now.to_rfc3339(),
                ],
            )
            .map_err(|error| error.to_string())?;
        if recurring {
            let next = CronPlan::validate(&expression, &timezone)?
                .next_after(now)?
                .to_rfc3339();
            transaction
                .execute(
                    "UPDATE cron_jobs SET last_slot_at_utc = ?1, next_run_at_utc = ?2,
                        updated_at = ?3 WHERE workspace_id = ?4 AND id = ?5",
                    params![slot, next, now.to_rfc3339(), workspace_id, id],
                )
                .map_err(|error| error.to_string())?;
        } else {
            transaction
                .execute(
                    "DELETE FROM cron_jobs WHERE workspace_id = ?1 AND id = ?2",
                    params![workspace_id, id],
                )
                .map_err(|error| error.to_string())?;
        }
        let _ = durable;
        events.push(event);
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(events)
}

pub fn acknowledge(
    connection: &Connection,
    workspace_id: &str,
    event_id: Uuid,
) -> Result<bool, String> {
    let changed = connection
        .execute(
            "UPDATE cron_outbox SET acknowledged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE workspace_id = ?1 AND event_id = ?2 AND acknowledged_at IS NULL",
            params![workspace_id, event_id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    Ok(changed == 1)
}

pub fn pending(connection: &Connection, workspace_id: &str) -> Result<Vec<CronEvent>, String> {
    let mut statement = connection
        .prepare(
            "SELECT event_id, job_id, slot_at_utc, identity, prompt
             FROM cron_outbox WHERE workspace_id = ?1 AND acknowledged_at IS NULL
             ORDER BY slot_at_utc ASC, event_id ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], |row| {
            Ok(CronEvent {
                event_id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                workspace_id: workspace_id.to_string(),
                job_id: Uuid::parse_str(&row.get::<_, String>(1)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                slot_at_utc: row.get(2)?,
                identity: row.get(3)?,
                prompt: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
