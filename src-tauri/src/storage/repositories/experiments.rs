use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

/// 第 43 章「加入实验计划」：写入 `experiments` 表（0033 迁移建立）。
///
/// `variables_json` 是 `[{name, low, high, value}]` 数组：来自优化方案时
/// `value` 是推荐的工艺取值、`low/high` 为搜索边界；手工条目可以只给范围。
/// `recommendation_json` 保存来源候选的完整数据（含训练任务 ID）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExperimentRecord {
    pub id: String,
    pub conversation_id: Option<String>,
    pub run_id: Option<String>,
    pub title: String,
    pub objective: String,
    pub variables_json: String,
    pub recommendation_json: Option<String>,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewExperiment<'a> {
    pub title: &'a str,
    pub objective: Option<&'a str>,
    pub variables_json: &'a str,
    pub recommendation_json: Option<&'a str>,
    /// 来源状态：optimization → "proposed"，manual → "draft"。
    pub state: &'a str,
}

pub const EXPERIMENT_STATES: [&str; 4] = ["draft", "proposed", "accepted", "rejected"];

const SELECT_EXPERIMENT: &str = "SELECT id, conversation_id, run_id, title, objective,
    variables_json, recommendation_json, state, created_at, updated_at
    FROM experiments WHERE workspace_id = ?1 AND id = ?2";

fn row_to_experiment(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExperimentRecord> {
    Ok(ExperimentRecord {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        run_id: row.get(2)?,
        title: row.get(3)?,
        objective: row.get(4)?,
        variables_json: row.get(5)?,
        recommendation_json: row.get(6)?,
        state: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn validate_variables(variables_json: &str) -> Result<(), String> {
    let variables: serde_json::Value = serde_json::from_str(variables_json)
        .map_err(|error| format!("variables must be valid JSON: {error}"))?;
    let items = variables
        .as_array()
        .ok_or_else(|| "variables must be a JSON array".to_string())?;
    if items.is_empty() || items.len() > 64 {
        return Err("experiment plan requires 1-64 variables".to_string());
    }
    for (index, item) in items.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("variables[{index}].name is required"))?;
        if name.len() > 120 {
            return Err(format!("variables[{index}].name is too long"));
        }
        for key in ["low", "high", "value"] {
            if let Some(value) = item.get(key) {
                value
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| format!("variables[{index}].{key} must be a finite number"))?;
            }
        }
        let low = item.get("low").and_then(serde_json::Value::as_f64);
        let high = item.get("high").and_then(serde_json::Value::as_f64);
        if let (Some(low), Some(high)) = (low, high) {
            if low > high {
                return Err(format!("variables[{index}] range is inverted"));
            }
        }
    }
    Ok(())
}

pub fn create(
    connection: &mut Connection,
    workspace_id: &str,
    experiment: NewExperiment<'_>,
) -> Result<ExperimentRecord, String> {
    if experiment.state != "draft" && experiment.state != "proposed" {
        return Err("new experiment plans must start as draft or proposed".to_string());
    }
    let title = experiment.title.trim();
    if title.is_empty() || title.len() > 200 {
        return Err("experiment plan title must be 1-200 characters".to_string());
    }
    if let Some(objective) = experiment.objective {
        if objective.len() > 2000 {
            return Err("experiment plan objective is too long".to_string());
        }
    }
    validate_variables(experiment.variables_json)?;
    if let Some(recommendation) = experiment.recommendation_json {
        serde_json::from_str::<serde_json::Value>(recommendation)
            .map_err(|error| format!("recommendation must be valid JSON: {error}"))?;
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    transaction
        .execute(
            "INSERT INTO experiments
               (workspace_id, id, conversation_id, run_id, title, objective,
                variables_json, recommendation_json, state, created_at, updated_at)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                workspace_id,
                id,
                title,
                experiment.objective.map(str::trim).filter(|note| !note.is_empty()).unwrap_or(""),
                experiment.variables_json,
                experiment.recommendation_json,
                experiment.state,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    get(connection, workspace_id, &id)?.ok_or_else(|| "created experiment plan could not be read back".to_string())
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    id: &str,
) -> Result<Option<ExperimentRecord>, String> {
    connection
        .query_row(SELECT_EXPERIMENT, params![workspace_id, id], row_to_experiment)
        .optional()
        .map_err(|error| error.to_string())
}

pub fn list(connection: &Connection, workspace_id: &str) -> Result<Vec<ExperimentRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, conversation_id, run_id, title, objective,
                    variables_json, recommendation_json, state, created_at, updated_at
             FROM experiments WHERE workspace_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], row_to_experiment)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn set_state(
    connection: &mut Connection,
    workspace_id: &str,
    id: &str,
    state: &str,
) -> Result<ExperimentRecord, String> {
    if !EXPERIMENT_STATES.contains(&state) {
        return Err(format!(
            "experiment state must be one of {}",
            EXPERIMENT_STATES.join(", ")
        ));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let updated = transaction
        .execute(
            "UPDATE experiments SET state = ?3, updated_at = ?4
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id, state, Utc::now().to_rfc3339()],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("experiment plan was not found".to_string());
    }
    get(connection, workspace_id, id)?.ok_or_else(|| "updated experiment plan could not be read back".to_string())
}

pub fn delete(connection: &mut Connection, workspace_id: &str, id: &str) -> Result<(), String> {
    let deleted = connection
        .execute(
            "DELETE FROM experiments WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id],
        )
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err("experiment plan was not found".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{create, delete, list, set_state, NewExperiment};
    use crate::storage::migrations::migrate;
    use rusqlite::Connection;

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open database");
        migrate(&mut connection).expect("migrate database");
        connection
    }

    fn sample(state: &'static str) -> NewExperiment<'static> {
        NewExperiment {
            title: "方案 A：880℃ 淬火",
            objective: Some("最大化硬度"),
            variables_json: r#"[{"name":"quench","low":800.0,"high":900.0,"value":880.0}]"#,
            recommendation_json: Some(r#"{"training_task_id":"task-1"}"#),
            state,
        }
    }

    #[test]
    fn creates_lists_and_updates_plans() {
        let mut connection = database();
        let proposed = create(&mut connection, "ws", sample("proposed")).expect("create proposed");
        assert_eq!(proposed.state, "proposed");
        let manual = create(&mut connection, "ws", sample("draft")).expect("create draft");
        assert_eq!(manual.objective, "最大化硬度");

        let plans = list(&connection, "ws").expect("list plans");
        assert_eq!(plans.len(), 2);

        let accepted = set_state(&mut connection, "ws", &proposed.id, "accepted").expect("accept");
        assert_eq!(accepted.state, "accepted");
        assert!(delete(&mut connection, "ws", &manual.id).is_ok());
        assert_eq!(list(&connection, "ws").unwrap().len(), 1);
    }

    #[test]
    fn rejects_invalid_states_titles_and_variables() {
        let mut connection = database();
        let mut invalid_state = sample("accepted");
        invalid_state.state = "accepted";
        assert!(create(&mut connection, "ws", invalid_state).is_err());

        let mut empty_title = sample("draft");
        empty_title.title = "  ";
        assert!(create(&mut connection, "ws", empty_title).is_err());

        let mut no_variables = sample("draft");
        no_variables.variables_json = "[]";
        assert!(create(&mut connection, "ws", no_variables).is_err());

        let mut inverted = sample("draft");
        inverted.variables_json = r#"[{"name":"t","low":900.0,"high":800.0}]"#;
        assert!(create(&mut connection, "ws", inverted).is_err());

        let created = create(&mut connection, "ws", sample("draft")).expect("create");
        assert!(set_state(&mut connection, "ws", &created.id, "finished").is_err());
    }
}
