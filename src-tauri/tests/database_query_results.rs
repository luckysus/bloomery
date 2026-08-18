use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::database_query_results::{
    self, QueryResultRecord, QueryResultSummary,
};
use rusqlite::Connection;
use uuid::Uuid;

const WORKSPACE: &str = "local";
const OTHER: &str = "other";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    connection
}

fn record(task_id: Uuid, query: &str) -> QueryResultRecord {
    QueryResultRecord {
        task_id,
        connection_id: Uuid::new_v4(),
        database_name: "SteelWorks".to_string(),
        query_text: query.to_string(),
        row_count: 3,
        truncated: false,
        duration_ms: 421,
        csv_path: format!("C:/cache/{task_id}.csv"),
        columns: vec!["heat_id".to_string(), "carbon_pct".to_string()],
        rows: vec![
            vec![Some("H1".to_string()), Some("0.18".to_string())],
            vec![Some("H2".to_string()), Some("0.21".to_string())],
            vec![Some("H3".to_string()), Some("0.16".to_string())],
        ],
        created_at: "2026-08-18T10:00:00+08:00".to_string(),
    }
}

#[test]
fn query_result_round_trip() {
    let conn = database();
    let first = record(Uuid::new_v4(), "SELECT 1 AS heat_id");
    let mut second = record(Uuid::new_v4(), "SELECT 2 AS heat_id");
    second.created_at = "2026-08-18T11:00:00+08:00".to_string();

    database_query_results::insert(&conn, WORKSPACE, &first).expect("insert first");
    database_query_results::insert(&conn, WORKSPACE, &second).expect("insert second");

    let fetched = database_query_results::get(&conn, WORKSPACE, first.task_id)
        .expect("get")
        .expect("exists");
    assert_eq!(fetched.query_text, "SELECT 1 AS heat_id");
    assert_eq!(
        fetched.columns,
        vec!["heat_id".to_string(), "carbon_pct".to_string()]
    );
    assert_eq!(fetched.rows.len(), 3);
    assert_eq!(fetched.rows[1][1].as_deref(), Some("0.21"));
    assert_eq!(fetched.csv_path, format!("C:/cache/{}.csv", first.task_id));

    let recent = database_query_results::list_recent(&conn, WORKSPACE, 10).expect("list");
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].query_text, "SELECT 2 AS heat_id", "倒序:最新在前");
}

#[test]
fn query_results_are_workspace_scoped() {
    let conn = database();
    let owned = record(Uuid::new_v4(), "SELECT 1");
    database_query_results::insert(&conn, WORKSPACE, &owned).expect("insert");

    assert!(database_query_results::get(&conn, OTHER, owned.task_id)
        .expect("get other")
        .is_none());
    assert!(database_query_results::list_recent(&conn, OTHER, 10)
        .expect("list other")
        .is_empty());
}

#[test]
fn query_result_rejects_duplicate_task() {
    let conn = database();
    let owned = record(Uuid::new_v4(), "SELECT 1");
    database_query_results::insert(&conn, WORKSPACE, &owned).expect("insert");
    assert!(database_query_results::insert(&conn, WORKSPACE, &owned).is_err());
}

#[test]
fn list_recent_respects_limit() {
    let conn = database();
    for index in 0..15 {
        let mut item = record(Uuid::new_v4(), &format!("SELECT {index}"));
        item.created_at = format!("2026-08-18T10:{index:02}:00+08:00");
        database_query_results::insert(&conn, WORKSPACE, &item).expect("insert");
    }
    let recent = database_query_results::list_recent(&conn, WORKSPACE, 10).expect("list");
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0].query_text, "SELECT 14", "created_at 最新的在前");
}

#[test]
fn summary_has_no_rows_payload() {
    let summary = QueryResultSummary {
        task_id: Uuid::new_v4(),
        database_name: "SteelWorks".to_string(),
        query_text: "SELECT 1".to_string(),
        row_count: 3,
        truncated: true,
        duration_ms: 421,
        created_at: "2026-08-18T10:00:00+08:00".to_string(),
    };
    assert!(!format!("{summary:?}").contains("rows"));
}
