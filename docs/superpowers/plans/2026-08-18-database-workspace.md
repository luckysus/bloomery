# 数据库工作区实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 SQL Server 连接模块升级为完整数据库工作区:一级导航「数据库」分区(库/表浏览、受控只读查询、结果送入数据分析),设置页标签导航 + 连接健康持久化。

**Architecture:** 查询作为后台任务(`database_query` kind)在 Rust 主进程直连 tiberius 执行,gaurd 强制单条 SELECT/WITH 并外层 `TOP (n)` 包装;结果双写 SQLite(`database_query_results` 表)与缓存 CSV(复用 `saveSteelDataset` 文件管道);凭据只经 keyring,不下发 compute-worker。设计规格:`docs/superpowers/specs/2026-08-18-database-workspace-design.md`。

**Tech Stack:** Rust/Tauri 2、rusqlite、tiberius 0.12(tds73)、React 18 + TypeScript、Vitest。

**对规格的一处修正:** 缓存 CSV 只含表头与数据行,不含 `--` 注释头(注释行会破坏 CSV 解析);溯源信息全部在 `database_query_results` 表。

**全程约束:**

- 仓库:`F:/steel-agent/bloomery`(独立嵌套 git 仓库)。工作树有大量他人未提交改动,**每次 commit 只 `git add` 本任务列出的文件**。
- 命令用 PowerShell。前端测试:`Set-Location frontend; npm test -- <文件名>`;Rust:`Set-Location src-tauri; cargo test <名> `。
- 不改 `frontend/src/bridge/generated/protocol.ts`(自动生成)。
- i18n 新 key 必须同时加进 `src/i18n/locale.tsx` 的 zhCN 与 en-US 两个字典(`Record<MessageKey,...>` 类型强制,漏一个编译不过)。

---

### Task 1: 迁移 0024 -- 健康列 + 查询结果表

**Files:**
- Create: `src-tauri/src/storage/migrations/0024_database_workspace.sql`
- Modify: `src-tauri/src/storage/migrations.rs`(数组末尾追加)
- Modify: `src-tauri/src/storage/repositories/database_connections.rs`(Record 健康字段 + `record_health`)
- Modify: `src-tauri/tests/database_connections.rs`(新测试)
- Modify: `src-tauri/tests/migrations.rs`(版本断言)

- [ ] **Step 1: 写失败的仓储测试**

在 `src-tauri/tests/database_connections.rs` 末尾追加(record() 结构体需同步加 4 个 `None` 字段,见 Step 3):

```rust
#[test]
fn record_health_updates_health_columns_only() {
    let mut conn = database();
    let original = record("转炉");
    database_connections::save(&mut conn, WORKSPACE, &original).expect("save");

    database_connections::record_health(
        &conn,
        WORKSPACE,
        original.id,
        &Utc::now().to_rfc3339(),
        Some(128),
        Some("Microsoft SQL Server 2022"),
        None,
    )
    .expect("record health");

    let healthy = database_connections::get(&conn, WORKSPACE, original.id)
        .expect("get")
        .expect("exists");
    assert_eq!(healthy.last_latency_ms, Some(128));
    assert_eq!(healthy.last_version.as_deref(), Some("Microsoft SQL Server 2022"));
    assert!(healthy.last_error.is_none());
    assert_eq!(healthy.display_name, "转炉", "health write must not touch other columns");
}

#[test]
fn record_health_rejects_foreign_workspace() {
    let mut conn = database();
    let owned = record("电炉");
    database_connections::save(&mut conn, WORKSPACE, &owned).expect("save");
    assert!(database_connections::record_health(
        &conn,
        OTHER_WORKSPACE,
        owned.id,
        &Utc::now().to_rfc3339(),
        None,
        None,
        Some("timeout")
    )
    .is_err());
}
```

文件顶部 `use` 区加:`use chrono::Utc;`

`src-tauri/tests/migrations.rs` 中找到最新版本断言(当前应为 `23`),改为断言 `24`。

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location src-tauri; cargo test database_connections migrations`
Expected: 编译失败(`record_health` 未定义、Record 缺字段)。

- [ ] **Step 3: 迁移 SQL + 仓储实现**

新建 `src-tauri/src/storage/migrations/0024_database_workspace.sql`:

```sql
ALTER TABLE database_connections ADD COLUMN last_checked_at TEXT;
ALTER TABLE database_connections ADD COLUMN last_latency_ms INTEGER;
ALTER TABLE database_connections ADD COLUMN last_version TEXT;
ALTER TABLE database_connections ADD COLUMN last_error TEXT;

CREATE TABLE database_query_results (
  task_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  database_name TEXT NOT NULL DEFAULT '',
  query_text TEXT NOT NULL,
  row_count INTEGER NOT NULL,
  truncated INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  csv_path TEXT NOT NULL,
  columns_json TEXT NOT NULL,
  rows_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_database_query_results_workspace_created
  ON database_query_results(workspace_id, created_at DESC, task_id);
```

`src-tauri/src/storage/migrations.rs` 的 `MIGRATIONS` 数组末尾(version 23 之后)追加:

```rust
    Migration {
        version: 24,
        sql: include_str!("migrations/0024_database_workspace.sql"),
    },
```

`src-tauri/src/storage/repositories/database_connections.rs` 修改:

`DatabaseConnectionRecord` 结构体加 4 个字段:

```rust
    pub enabled: bool,
    pub last_checked_at: Option<String>,
    pub last_latency_ms: Option<i64>,
    pub last_version: Option<String>,
    pub last_error: Option<String>,
```

`SELECT_COLUMNS` 改为:

```rust
const SELECT_COLUMNS: &str =
    "id, display_name, host, port, database_name, username, timeout_ms, enabled, last_checked_at, last_latency_ms, last_version, last_error";
```

`decode` 末尾追加(替换原 `enabled` 行之后):

```rust
        enabled: row.get::<_, i64>(7)? == 1,
        last_checked_at: row.get(8)?,
        last_latency_ms: row.get(9)?,
        last_version: row.get(10)?,
        last_error: row.get(11)?,
```

注意:`save()` 的 INSERT/UPSERT 不写这 4 列(健康列只由 `record_health` 更新,UPDATE SET 列表不含它们,已满足)。文件末尾追加:

```rust
pub fn record_health(
    conn: &Connection,
    workspace_id: &str,
    id: Uuid,
    checked_at: &str,
    latency_ms: Option<i64>,
    version: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE database_connections
             SET last_checked_at = ?3, last_latency_ms = ?4, last_version = ?5, last_error = ?6
             WHERE workspace_id = ?1 AND id = ?2",
            params![
                workspace_id,
                id.to_string(),
                checked_at,
                latency_ms,
                version,
                error
            ],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("database connection not found".to_string());
    }
    Ok(())
}
```

同步修 `tests/database_connections.rs` 顶部的 `record()` 辅助(加 4 个 `None`)与 `src-tauri/src/app/database_commands/logic.rs` 中 `normalized()` 构造 `DatabaseConnectionRecord` 处(加 4 个 `None`)。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location src-tauri; cargo test database_connections migrations`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/storage/migrations/0024_database_workspace.sql src-tauri/src/storage/migrations.rs src-tauri/src/storage/repositories/database_connections.rs src-tauri/tests/database_connections.rs src-tauri/tests/migrations.rs src-tauri/src/app/database_commands/logic.rs
git commit -m "支持数据库连接健康记录与查询结果表"
```

---

### Task 2: database_query_results 仓储

**Files:**
- Create: `src-tauri/src/storage/repositories/database_query_results.rs`
- Modify: `src-tauri/src/storage/repositories/mod.rs`(加 `pub mod database_query_results;`,按字母序放 `database_connections` 之后)
- Create: `src-tauri/tests/database_query_results.rs`

- [ ] **Step 1: 写失败的集成测试**

新建 `src-tauri/tests/database_query_results.rs`:

```rust
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

pub fn record(task_id: Uuid, query: &str) -> QueryResultRecord {
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
    let second = record(Uuid::new_v4(), "SELECT 2 AS heat_id");

    database_query_results::insert(&conn, WORKSPACE, &first).expect("insert first");
    database_query_results::insert(&conn, WORKSPACE, &second).expect("insert second");

    let fetched = database_query_results::get(&conn, WORKSPACE, first.task_id)
        .expect("get")
        .expect("exists");
    assert_eq!(fetched.query_text, "SELECT 1 AS heat_id");
    assert_eq!(fetched.columns, vec!["heat_id".to_string()]);
    assert_eq!(fetched.rows.len(), 3);
    assert_eq!(fetched.rows[1][1].as_deref(), Some("0.21"));

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
```

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location src-tauri; cargo test database_query_results`
Expected: 编译失败(模块不存在)。

- [ ] **Step 3: 实现仓储**

新建 `src-tauri/src/storage/repositories/database_query_results.rs`:

```rust
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct QueryResultRecord {
    pub task_id: Uuid,
    pub connection_id: Uuid,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub csv_path: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryResultSummary {
    pub task_id: Uuid,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub created_at: String,
}

pub fn insert(conn: &Connection, workspace_id: &str, record: &QueryResultRecord) -> Result<(), String> {
    let columns_json = serde_json::to_string(&record.columns).map_err(|error| error.to_string())?;
    let rows_json = serde_json::to_string(&record.rows).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO database_query_results
          (task_id, workspace_id, connection_id, database_name, query_text,
           row_count, truncated, duration_ms, csv_path, columns_json, rows_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.task_id.to_string(),
            workspace_id,
            record.connection_id.to_string(),
            record.database_name,
            record.query_text,
            record.row_count,
            record.truncated as i64,
            record.duration_ms,
            record.csv_path,
            columns_json,
            rows_json,
            record.created_at
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get(
    conn: &Connection,
    workspace_id: &str,
    task_id: Uuid,
) -> Result<Option<QueryResultRecord>, String> {
    conn.query_row(
        "SELECT connection_id, database_name, query_text, row_count, truncated, duration_ms,
                csv_path, columns_json, rows_json, created_at
         FROM database_query_results WHERE workspace_id = ?1 AND task_id = ?2",
        params![workspace_id, task_id.to_string()],
        |row| {
            let columns_json: String = row.get(7)?;
            let rows_json: String = row.get(8)?;
            Ok(QueryResultRecord {
                task_id,
                connection_id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error)))?,
                database_name: row.get(1)?,
                query_text: row.get(2)?,
                row_count: row.get(3)?,
                truncated: row.get::<_, i64>(4)? == 1,
                duration_ms: row.get(5)?,
                csv_path: row.get(6)?,
                columns: serde_json::from_str(&columns_json)
                    .map_err(|error| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error)))?,
                rows: serde_json::from_str(&rows_json)
                    .map_err(|error| rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error)))?,
                created_at: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(Ok)
    .transpose()
}

pub fn list_recent(
    conn: &Connection,
    workspace_id: &str,
    limit: u32,
) -> Result<Vec<QueryResultSummary>, String> {
    let mut statement = conn
        .prepare(
            "SELECT task_id, database_name, query_text, row_count, truncated, duration_ms, created_at
             FROM database_query_results
             WHERE workspace_id = ?1
             ORDER BY created_at DESC, task_id DESC
             LIMIT ?2",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id, limit], |row| {
            Ok(QueryResultSummary {
                task_id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error)))?,
                database_name: row.get(1)?,
                query_text: row.get(2)?,
                row_count: row.get(3)?,
                truncated: row.get::<_, i64>(4)? == 1,
                duration_ms: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
}

#[allow(dead_code)]
fn _unused_created_at_helper() -> String {
    Utc::now().to_rfc3339()
}
```

注意:若 `Utc` 未被使用产生 warning,删除 import 与 `_unused_created_at_helper`(仓储自身不生成时间戳,由调用方传 `created_at`)。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location src-tauri; cargo test database_query_results`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/storage/repositories/database_query_results.rs src-tauri/src/storage/repositories/mod.rs src-tauri/tests/database_query_results.rs
git commit -m "新增数据库查询结果仓储"
```

---

### Task 3: 查询 guard(纯函数)

**Files:**
- Create: `src-tauri/src/database/query.rs`
- Modify: `src-tauri/src/database/mod.rs`(加 `pub mod query;`)

- [ ] **Step 1: 写失败的单测**

新建 `src-tauri/src/database/query.rs`,先只放测试骨架:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_accepts_select_and_with() {
        assert_eq!(
            normalize_query(" select heat_id from heats; ").expect("select"),
            "select heat_id from heats"
        );
        assert_eq!(
            normalize_query("WITH c AS (SELECT 1 AS n) SELECT n FROM c").expect("with"),
            "WITH c AS (SELECT 1 AS n) SELECT n FROM c"
        );
    }

    #[test]
    fn normalize_rejects_writes_and_ddl() {
        for sql in [
            "INSERT INTO t VALUES (1)",
            "update t set a = 1",
            "DELETE FROM t",
            "truncate table t",
            "drop table t",
            "alter table t add c int",
            "exec sp_help",
            "MERGE INTO t USING s ON 1=1;",
            "sp_executesql N'select 1'",
            "",
            "   ",
            ";",
        ] {
            assert!(normalize_query(sql).is_err(), "must reject: {sql}");
        }
    }

    #[test]
    fn normalize_rejects_multi_statement_and_leading_comment() {
        assert!(normalize_query("SELECT 1; DELETE FROM t").is_err());
        assert!(normalize_query("/* select */ DELETE FROM t").is_err());
        assert!(normalize_query("SELECT 1;").is_ok(), "单条尾分号允许");
    }

    #[test]
    fn wrap_forces_top_and_derived_table() {
        let wrapped = wrap_query("SELECT a FROM t", 500);
        assert!(wrapped.starts_with("SELECT TOP (500) * FROM ("));
        assert!(wrapped.ends_with(") AS [_bloomery_query]"));
        assert!(wrap_query("SELECT 1", 1).contains("TOP (1)"));
    }

    #[test]
    fn row_limit_clamps() {
        assert_eq!(clamp_row_limit(None), 500);
        assert_eq!(clamp_row_limit(Some(0)), 1);
        assert_eq!(clamp_row_limit(Some(100)), 100);
        assert_eq!(clamp_row_limit(Some(9_999_999)), 5_000);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location src-tauri; cargo test database::query`
Expected: 编译失败(函数未定义)。

- [ ] **Step 3: 实现**

在 `src-tauri/src/database/query.rs` 顶部(mod tests 之前)加入:

```rust
pub const DEFAULT_ROW_LIMIT: u64 = 500;
pub const MAX_ROW_LIMIT: u64 = 5_000;

pub fn clamp_row_limit(limit: Option<u64>) -> u64 {
    limit.unwrap_or(DEFAULT_ROW_LIMIT).clamp(1, MAX_ROW_LIMIT)
}

fn first_keyword(sql: &str) -> String {
    sql.chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// 只放行单条 SELECT / WITH 查询;拒绝多语句、写操作与前导注释伪装。
pub fn normalize_query(sql: &str) -> Result<String, String> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Err("查询不能为空".to_string());
    }
    let first = first_keyword(trimmed);
    if first.is_empty() || (first != "select" && first != "with") {
        return Err(format!("仅支持只读 SELECT/WITH 查询，当前开头为“{first}”"));
    }
    if trimmed.contains(';') {
        return Err("一次只能执行一条查询语句".to_string());
    }
    if trimmed.starts_with('/') || trimmed.starts_with('-') {
        return Err("查询不能以注释开头".to_string());
    }
    Ok(trimmed.to_string())
}

/// 外层 TOP (n) + 派生表包装，使写操作在结构上不可能执行。
pub fn wrap_query(sql: &str, row_limit: u64) -> String {
    format!("SELECT TOP ({row_limit}) * FROM ({sql}) AS [_bloomery_query]")
}
```

`src-tauri/src/database/mod.rs` 顶部加 `pub mod query;`。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location src-tauri; cargo test database::query`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/database/query.rs src-tauri/src/database/mod.rs
git commit -m "新增数据库只读查询守卫"
```

---

### Task 4: 目录查询与结果执行(catalog + execute_read)

**Files:**
- Create: `src-tauri/src/database/catalog.rs`
- Modify: `src-tauri/src/database/mod.rs`(加 `pub mod catalog;`,把 `SqlClient` 保留在 mod.rs)
- Modify: `src-tauri/src/database/catalog.rs`(含单测)

- [ ] **Step 1: 写失败的单测**

新建 `src-tauri/src/database/catalog.rs`,先写测试:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_identifier_brackets_and_doubles_closing_brackets() {
        assert_eq!(escape_identifier("SteelWorks").expect("plain"), "[SteelWorks]");
        assert_eq!(escape_identifier(" my]db ").expect("trimmed"), "[my]]db]");
    }

    #[test]
    fn escape_identifier_rejects_empty() {
        assert!(escape_identifier("").is_err());
        assert!(escape_identifier("   ").is_err());
    }

    #[test]
    fn value_to_string_maps_scalars_and_null() {
        use tiberius::Value;
        assert_eq!(value_to_string(Value::I32(42)), Some("42".to_string()));
        assert_eq!(value_to_string(Value::F64(0.5)), Some("0.5".to_string()));
        assert_eq!(value_to_string(Value::Bit(true)), Some("1".to_string()));
        assert_eq!(value_to_string(Value::Null), None);
    }

    #[test]
    fn csv_cell_quotes_specials() {
        assert_eq!(csv_cell(Some("plain")), "plain");
        assert_eq!(csv_cell(Some("a,b")), "\"a,b\"");
        assert_eq!(csv_cell(Some("say \"hi\"")), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell(Some("line\nbreak")), "\"line\nbreak\"");
        assert_eq!(csv_cell(None), "");
    }
}
```

注意:`Value` 的字符串变体(如 `NVarChar`)如与 0.12.3 实际变体名不符,以 `cargo check` 报错为准修正测试断言只覆盖 `I32/F64/Bit/Null` 这四个确定存在的变体;`value_to_string` 实现中对字符串变体的处理保留 catch-all 分支(见 Step 3),不依赖精确变体名。

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location src-tauri; cargo test database::catalog`
Expected: 编译失败。

- [ ] **Step 3: 实现**

在 `src-tauri/src/database/catalog.rs` 顶部加入:

```rust
use crate::database::SqlClient;

/// 把数据库名转成安全的 `[name]` 标识符(右括号翻倍转义)。
pub fn escape_identifier(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("database name is required".to_string());
    }
    if trimmed.len() > 128 {
        return Err("database name is too long".to_string());
    }
    Ok(format!("[{}]", trimmed.replace(']', "]]")))
}

pub async fn list_databases(client: &mut SqlClient) -> Result<Vec<String>, String> {
    let stream = client
        .query("SELECT name FROM sys.databases ORDER BY name", &[])
        .await
        .map_err(|error| error.to_string())?;
    let rows = stream
        .into_first_result()
        .await
        .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.get::<&str, _>(0).map(|value| value.to_string()))
        .collect())
}

pub async fn table_names(client: &mut SqlClient, database: Option<&str>) -> Result<Vec<String>, String> {
    if let Some(name) = database {
        let use_statement = format!("USE {}", escape_identifier(name)?);
        client
            .simple_query(use_statement)
            .await
            .map_err(|error| format!("cannot switch to database {name}: {error}"))?;
    }
    let stream = client
        .query(
            "SELECT s.name + '.' + t.name
             FROM sys.tables AS t
             JOIN sys.schemas AS s ON s.schema_id = t.schema_id
             ORDER BY s.name ASC, t.name ASC",
            &[],
        )
        .await
        .map_err(|error| error.to_string())?;
    let rows = stream
        .into_first_result()
        .await
        .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.get::<&str, _>(0).map(|value| value.to_string()))
        .collect())
}

pub struct QueryRows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

pub async fn execute_read(client: &mut SqlClient, sql: &str) -> Result<QueryRows, String> {
    let mut stream = client
        .query(sql, &[])
        .await
        .map_err(|error| format!("query failed: {error}"))?;
    let columns: Vec<String> = stream
        .columns()
        .map(|metadata| metadata.iter().map(|column| column.name().to_string()).collect())
        .unwrap_or_default();
    let groups = stream
        .into_results()
        .await
        .map_err(|error| format!("query failed: {error}"))?;
    let rows = groups
        .into_iter()
        .flatten()
        .map(|row| {
            (0..columns.len())
                .map(|index| row.try_get::<tiberius::Value, _>(index).ok().and_then(value_to_string))
                .collect()
        })
        .collect();
    Ok(QueryRows { columns, rows })
}

/// tiberius 动态值转展示字符串;Null -> None,未显式处理的类型回退 Debug 输出。
pub fn value_to_string(value: tiberius::Value) -> Option<String> {
    use tiberius::Value;
    match value {
        Value::Null => None,
        Value::U8(value) => Some(value.to_string()),
        Value::I16(value) => Some(value.to_string()),
        Value::I32(value) => Some(value.to_string()),
        Value::I64(value) => Some(value.to_string()),
        Value::F32(value) => Some(value.to_string()),
        Value::F64(value) => Some(value.to_string()),
        Value::Bit(value) => Some(if value { "1".to_string() } else { "0".to_string() }),
        other => Some(format!("{other:?}")),
    }
}

/// 最小 CSV 单元格转义;供查询结果缓存文件使用。
pub fn csv_cell(value: Option<&str>) -> String {
    match value {
        None => String::new(),
        Some(text) => {
            if text.contains(',') || text.contains('"') || text.contains('\n') || text.contains('\r') {
                format!("\"{}\"", text.replace('"', "\"\""))
            } else {
                text.to_string()
            }
        }
    }
}
```

`src-tauri/src/database/mod.rs` 顶部加 `pub mod catalog;`。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location src-tauri; cargo test database::catalog`
Expected: PASS(如 `Value` 变体名与实际不符,按编译错误修正匹配臂;catch-all 保证最终可编译)。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/database/catalog.rs src-tauri/src/database/mod.rs
git commit -m "新增数据库目录查询与结果执行"
```

---

### Task 5: database_query 后台任务 handler

**Files:**
- Create: `src-tauri/src/database/query_task.rs`
- Modify: `src-tauri/src/database/mod.rs`(加 `pub mod query_task;`)
- Modify: `src-tauri/src/db.rs`(`rag_task_handlers_with_compute` 加第 10 个 handler;测试断言 `handlers.len() == 9` 改 `10`)

- [ ] **Step 1: 写失败的单测(payload 解析与结果构造的纯函数部分)**

新建 `src-tauri/src/database/query_task.rs`,先写测试:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_payload_reads_fields() {
        let payload = parse_payload(
            r#"{"connection_id":"11111111-1111-1111-1111-111111111111","database":"SteelWorks","sql":"SELECT 1","row_limit":100}"#,
        )
        .expect("parse");
        assert_eq!(payload.sql, "SELECT 1");
        assert_eq!(payload.database.as_deref(), Some("SteelWorks"));
        assert_eq!(payload.row_limit, Some(100));
    }

    #[test]
    fn parse_payload_rejects_invalid_json() {
        assert!(parse_payload("not json").is_err());
        assert!(parse_payload("{}").is_err(), "缺 connection_id/sql 应拒绝");
    }

    #[test]
    fn csv_document_contains_header_and_rows() {
        let document = build_csv_document(
            &["heat_id".to_string(), "carbon_pct".to_string()],
            &vec![
                vec![Some("H1".to_string()), Some("0.18".to_string())],
                vec![Some("H,2".to_string()), None],
            ],
        );
        let lines: Vec<&str> = document.lines().collect();
        assert_eq!(lines[0], "heat_id,carbon_pct");
        assert_eq!(lines[1], "H1,0.18");
        assert_eq!(lines[2], "\"H,2\",");
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location src-tauri; cargo test database::query_task`
Expected: 编译失败。

- [ ] **Step 3: 实现 handler**

在 `src-tauri/src/database/query_task.rs` 顶部加入:

```rust
use crate::database::{catalog, query as query_guard, SqlClient};
use crate::storage::repositories::{database_connections as connections_repository, database_query_results as results_repository};
use crate::storage::repositories::database_query_results::QueryResultRecord;
use crate::storage::secrets::{KeyringSecretStore, SecretRef, SecretStore};
use crate::tasks::scheduler::{HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, TaskHandler};
use crate::tasks::TaskRecord;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const DATABASE_QUERY_KIND: &str = "database_query";
pub const PASSWORD_CREDENTIAL: &str = "password";
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
struct QueryTaskPayload {
    connection_id: String,
    database: Option<String>,
    sql: String,
    row_limit: Option<u64>,
}

fn parse_payload(payload_json: &str) -> Result<QueryTaskPayload, HandlerError> {
    let payload: QueryTaskPayload =
        serde_json::from_str(payload_json).map_err(|_| HandlerError::permanent("invalid_payload"))?;
    if payload.connection_id.trim().is_empty() || payload.sql.trim().is_empty() {
        return Err(HandlerError::permanent("invalid_payload"));
    }
    Ok(payload)
}

fn csv_line(cells: &[Option<String>]) -> String {
    cells
        .iter()
        .map(|cell| catalog::csv_cell(cell.as_deref()))
        .collect::<Vec<_>>()
        .join(",")
}

fn build_csv_document(columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    let mut document = String::new();
    document.push_str(&columns.join(","));
    document.push('\n');
    for row in rows {
        document.push_str(&csv_line(row));
        document.push('\n');
    }
    document
}

async fn cancellation_watch(context: HandlerContext) {
    loop {
        if context.shutdown_requested() || context.cancellation_requested().unwrap_or(true) {
            return;
        }
        tokio::time::sleep(CANCEL_POLL_INTERVAL).await;
    }
}

async fn run_query(
    mut client: SqlClient,
    database: Option<&str>,
    wrapped: String,
    timeout: Duration,
) -> Result<catalog::QueryRows, HandlerError> {
    if let Some(name) = database {
        let statement = format!("USE {}", catalog::escape_identifier(name).map_err(HandlerError::permanent)?);
        client
            .simple_query(statement)
            .await
            .map_err(|error| HandlerError::permanent(format!("cannot switch database: {error}")))?;
    }
    match tokio::time::timeout(timeout, catalog::execute_read(&mut client, &wrapped)).await {
        Ok(result) => result.map_err(|error| HandlerError::permanent(format!("query_failed: {error}"))),
        Err(_) => Err(HandlerError::permanent("query_timeout")),
    }
}

pub struct DatabaseQueryTaskHandler {
    db_path: PathBuf,
}

impl DatabaseQueryTaskHandler {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl TaskHandler for DatabaseQueryTaskHandler {
    fn kind(&self) -> &str {
        DATABASE_QUERY_KIND
    }

    fn resumable(&self) -> bool {
        false
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let db_path = self.db_path.clone();
        Box::pin(async move { execute(task, context, db_path).await })
    }
}

async fn execute(
    task: TaskRecord,
    context: HandlerContext,
    db_path: PathBuf,
) -> Result<HandlerOutcome, HandlerError> {
    let payload = parse_payload(&task.payload_json)?;
    let connection_id = Uuid::parse_str(payload.connection_id.trim())
        .map_err(|_| HandlerError::permanent("invalid_payload"))?;
    let row_limit = query_guard::clamp_row_limit(payload.row_limit);
    let normalized = query_guard::normalize_query(&payload.sql)
        .map_err(|reason| HandlerError::permanent(format!("query_guard_rejected: {reason}")))?;

    let workspace_id = task.workspace_id.clone();
    let connection = rusqlite::Connection::open(&db_path)
        .map_err(|_| HandlerError::retryable("storage_unavailable"))?;
    let record = connections_repository::get(&connection, &workspace_id, connection_id)
        .map_err(|_| HandlerError::retryable("storage_unavailable"))?
        .ok_or_else(|| HandlerError::permanent("connection_not_found"))?;
    if !record.enabled {
        return Err(HandlerError::permanent("connection_disabled"));
    }
    let secret_reference = SecretRef::new(connection_id, PASSWORD_CREDENTIAL)
        .map_err(|_| HandlerError::permanent("password_not_configured"))?;
    let password = KeyringSecretStore
        .get(&secret_reference)
        .map_err(|_| HandlerError::permanent("password_not_configured"))?
        .expose()
        .to_string();

    context
        .checkpoint(Some(r#"{"stage":"running"}"#), 10, None)
        .map_err(|_| HandlerError::retryable("checkpoint_failed"))?;

    let started = Instant::now();
    let wrapped = query_guard::wrap_query(&normalized, row_limit);
    let timeout = Duration::from_millis(record.timeout_ms);
    let query = crate::database::connect(&record, &password);

    let rows = tokio::select! {
        joined = async {
            match query.await {
                Ok(client) => run_query(client, payload.database.as_deref(), wrapped, timeout).await,
                Err(error) => Err(HandlerError::permanent(format!("connection_failed: {error}"))),
            }
        } => joined?,
        _ = cancellation_watch(context.clone()) => return Ok(HandlerOutcome::Cancelled),
    };
    let duration_ms = started.elapsed().as_millis() as i64;

    let row_count = rows.rows.len() as i64;
    let truncated = row_limit > 0 && row_count == row_limit as i64;
    let csv_directory = db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("query-cache");
    std::fs::create_dir_all(&csv_directory)
        .map_err(|_| HandlerError::permanent("result_write_failed"))?;
    let csv_path = csv_directory.join(format!("{}.csv", task.id));
    std::fs::write(&csv_path, build_csv_document(&rows.columns, &rows.rows))
        .map_err(|_| HandlerError::permanent("result_write_failed"))?;

    let result = QueryResultRecord {
        task_id: task.id,
        connection_id,
        database_name: payload.database.clone().unwrap_or_default(),
        query_text: normalized,
        row_count,
        truncated,
        duration_ms,
        csv_path: csv_path.to_string_lossy().to_string(),
        columns: rows.columns,
        rows: rows.rows,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    results_repository::insert(&connection, &workspace_id, &result)
        .map_err(|_| HandlerError::permanent("result_write_failed"))?;

    context
        .checkpoint(Some(r#"{"stage":"completed"}"#), 100, None)
        .map_err(|_| HandlerError::retryable("checkpoint_failed"))?;
    Ok(HandlerOutcome::Completed)
}
```

`src-tauri/src/database/mod.rs` 加 `pub mod query_task;`。

`src-tauri/src/db.rs` 修改两处:

1. `rag_task_handlers_with_compute` 的 vec 末尾(`IndexRebuildHandler` 之后)追加:

```rust
        Arc::new(crate::database::query_task::DatabaseQueryTaskHandler::new(path.clone())),
```

2. 测试中断言 `assert_eq!(handlers.len(), 9);` 改为 `assert_eq!(handlers.len(), 10);`

注意:`cargo test` 时若 `tiberius`/`tokio::time` 在 handler 测试二进制中不可用属正常(主 crate 依赖齐全,无需新增 Cargo.toml 依赖;`tokio` 已是既有依赖,确认 `Cargo.toml` 已含 `tokio` 且带 `time` feature--若无则在 `[dependencies] tokio]` 的 features 里追加 `"time"`)。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location src-tauri; cargo test database::query_task db_init`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/database/query_task.rs src-tauri/src/database/mod.rs src-tauri/src/db.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "新增数据库查询后台任务"
```

(若未改 Cargo.toml 则去掉最后两个路径。)

---

### Task 6: 命令层 -- 提交/取结果/历史/列库/表浏览/健康持久化

**Files:**
- Modify: `src-tauri/src/app/database_commands/mod.rs`(新命令)
- Modify: `src-tauri/src/app/database_commands/types.rs`(新类型)
- Modify: `src-tauri/src/app/database_commands/logic.rs`(校验辅助 + summary 扩展)
- Modify: `src-tauri/src/app/commands.rs`(注册)

- [ ] **Step 1: 写失败的逻辑单测**

在 `src-tauri/src/app/database_commands/logic.rs` 的 `#[cfg(test)] mod tests` 内追加:

```rust
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
        assert_eq!(prepared.row_limit, 5_000);
        assert_eq!(prepared.connection_id.to_string(), "11111111-1111-1111-1111-111111111111");
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
```

测试模块顶部补 `use super::super::types::DatabaseQuerySubmitInput;` 与 `use uuid::Uuid;`(如已有则跳过)。

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location src-tauri; cargo test database_commands`
Expected: 编译失败。

- [ ] **Step 3: 实现**

`src-tauri/src/app/database_commands/types.rs` 追加:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseQuerySubmitInput {
    pub connection_id: String,
    pub database: Option<String>,
    pub sql: String,
    pub row_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseQueryResultResponse {
    pub task_id: String,
    pub connection_id: String,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub csv_path: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseQuerySummaryResponse {
    pub task_id: String,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub created_at: String,
}
```

`src-tauri/src/app/database_commands/logic.rs` 追加(同时把现有私有函数 `fn secret_configured` 改为 `pub(super) fn secret_configured`,Task 6 的 submit 命令要复用它;summary 同步扩健康字段,替换现有 `summary` 函数):

```rust
pub(super) struct PreparedQuerySubmission {
    pub connection_id: Uuid,
    pub sql: String,
    pub database: Option<String>,
    pub row_limit: u64,
}

pub(super) fn validate_submission(
    input: &super::types::DatabaseQuerySubmitInput,
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
    Ok(PreparedQuerySubmission { connection_id, sql, database, row_limit })
}
```

`summary()` 的 `DatabaseConnectionSummary` 构造追加 4 个字段(types.rs 的 `DatabaseConnectionSummary` 同步加):

```rust
        last_checked_at: record.last_checked_at.clone(),
        last_latency_ms: record.last_latency_ms,
        last_version: record.last_version.clone(),
        last_error: record.last_error.clone(),
```

`src-tauri/src/app/database_commands/mod.rs` 追加命令(文件顶部 use 区按需补 `std::time::Instant`、`crate::app::task_commands::tasks::{background_task_response, BackgroundTaskResponse}`、`crate::storage::repositories::database_query_results as results_repository`、`crate::tasks::model::NewTask`、`crate::tasks::repository as task_repository`、`crate::database::query_task::DATABASE_QUERY_KIND`):

```rust
#[tauri::command]
pub(crate) async fn list_databases(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<Vec<String>, String> {
    let id = parse_id(&id)?;
    let record = load_enabled_record(&db, secrets.store(), id)?;
    let secret = logic::password(secrets.store(), id)?;
    let mut client = database::connect(&record, &secret).await?;
    database::catalog::list_databases(&mut client).await
}

#[tauri::command]
pub(crate) async fn list_database_tables(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
    database_name: Option<String>,
) -> Result<Vec<String>, String> {
    let id = parse_id(&id)?;
    let record = load_enabled_record(&db, secrets.store(), id)?;
    let secret = logic::password(secrets.store(), id)?;
    let mut client = database::connect(&record, &secret).await?;
    let target = database_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    database::catalog::table_names(&mut client, target).await
}

#[tauri::command]
pub(crate) fn submit_database_query(
    db: tauri::State<DbState>,
    secrets: tauri::State<SecretState>,
    input: DatabaseQuerySubmitInput,
) -> Result<BackgroundTaskResponse, String> {
    let prepared = logic::validate_submission(&input)?;
    let payload = serde_json::json!({
        "connection_id": prepared.connection_id.to_string(),
        "database": prepared.database,
        "sql": prepared.sql,
        "row_limit": prepared.row_limit,
    });
    crate::db::with_conn_mut(&db, |connection| {
        let record = repository::get(connection, current_workspace_id(), prepared.connection_id)?
            .ok_or_else(|| "database connection not found".to_string())?;
        if !record.enabled {
            return Err("database connection is disabled".to_string());
        }
        if !logic::secret_configured(secrets.store(), prepared.connection_id) {
            return Err("database password is not configured".to_string());
        }
        task_repository::create(
            connection,
            NewTask {
                workspace_id: current_workspace_id().to_string(),
                kind: DATABASE_QUERY_KIND.to_string(),
                payload_json: payload.to_string(),
                checkpoint_json: Some(r#"{"stage":"queued"}"#.to_string()),
                next_run_at: None,
                progress: 0,
            },
        )
        .map(background_task_response)
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn get_database_query_result(
    db: tauri::State<DbState>,
    task_id: String,
) -> Result<Option<DatabaseQueryResultResponse>, String> {
    let task_id = parse_id(&task_id)?;
    crate::db::with_conn(&db, |connection| {
        Ok(results_repository::get(connection, current_workspace_id(), task_id)?
            .map(|record| DatabaseQueryResultResponse {
                task_id: record.task_id.to_string(),
                connection_id: record.connection_id.to_string(),
                database_name: record.database_name,
                query_text: record.query_text,
                row_count: record.row_count,
                truncated: record.truncated,
                duration_ms: record.duration_ms,
                csv_path: record.csv_path,
                columns: record.columns,
                rows: record.rows,
                created_at: record.created_at,
            }))
    })
}

#[tauri::command]
pub(crate) fn list_database_query_results(
    db: tauri::State<DbState>,
) -> Result<Vec<DatabaseQuerySummaryResponse>, String> {
    crate::db::with_conn(&db, |connection| {
        Ok(results_repository::list_recent(connection, current_workspace_id(), 10)?
            .into_iter()
            .map(|summary| DatabaseQuerySummaryResponse {
                task_id: summary.task_id.to_string(),
                database_name: summary.database_name,
                query_text: summary.query_text,
                row_count: summary.row_count,
                truncated: summary.truncated,
                duration_ms: summary.duration_ms,
                created_at: summary.created_at,
            })
            .collect())
    })
}
```

同文件把现有 `test_database_connection` 替换为(健康持久化版):

```rust
#[tauri::command]
pub(crate) async fn test_database_connection(
    db: tauri::State<'_, DbState>,
    secrets: tauri::State<'_, SecretState>,
    id: String,
) -> Result<String, String> {
    let id = parse_id(&id)?;
    let record = load_record(&db, id)?;
    let secret = logic::password(secrets.store(), id)?;
    let started = Instant::now();
    let outcome = async {
        let mut client = database::connect(&record, &secret).await?;
        database::server_version(&mut client).await
    }
    .await;
    let checked_at = chrono::Utc::now().to_rfc3339();
    match &outcome {
        Ok(version) => {
            crate::db::with_conn(&db, |connection| {
                repository::record_health(
                    connection,
                    current_workspace_id(),
                    id,
                    &checked_at,
                    Some(started.elapsed().as_millis() as i64),
                    Some(version),
                    None,
                )
            })?;
            Ok(version.clone())
        }
        Err(error) => {
            let _ = crate::db::with_conn(&db, |connection| {
                repository::record_health(
                    connection,
                    current_workspace_id(),
                    id,
                    &checked_at,
                    None,
                    None,
                    Some(error),
                )
            });
            Err(error.clone())
        }
    }
}
```

`logic.rs` 追加禁用校验辅助(供 list_databases/list_database_tables 使用):

```rust
pub(super) fn load_enabled_record(
    db: &tauri::State<'_, crate::db::DbState>,
    store: &dyn SecretStore,
    id: Uuid,
) -> Result<DatabaseConnectionRecord, String> {
    let record = load_record(db, id)?;
    if !record.enabled {
        return Err("database connection is disabled".to_string());
    }
    Ok(record)
}
```

(`load_enabled_record` 的 `store` 参数如触发未使用告警,去掉该参数并同步调整调用点。)

`src-tauri/src/app/commands.rs` 的 database_commands 注册块追加 4 行:

```rust
            crate::app::database_commands::list_databases,
            crate::app::database_commands::submit_database_query,
            crate::app::database_commands::get_database_query_result,
            crate::app::database_commands::list_database_query_results,
```

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location src-tauri; cargo test database_commands; cargo check`
Expected: PASS / 无错误。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/app/database_commands/mod.rs src-tauri/src/app/database_commands/types.rs src-tauri/src/app/database_commands/logic.rs src-tauri/src/app/commands.rs
git commit -m "支持数据库查询提交与结果查询命令"
```

---

### Task 7: 前端 bridge 扩展

**Files:**
- Modify: `frontend/src/bridge/desktop.ts`(类型 + 7 个方法)

- [ ] **Step 1: 写失败的类型测试**

在 `frontend/src/bridge/desktop.ts` 同目录无测试文件的惯例下,类型正确性由 `npm run build` 的 tsc 保证;本任务以 Task 8 的组件测试驱动方法存在性。先直接实现,验证放 Step 3 的 tsc。

- [ ] **Step 2: 实现**

`DatabaseConnectionSummary` 接口追加:

```ts
  last_checked_at: string | null;
  last_latency_ms: number | null;
  last_version: string | null;
  last_error: string | null;
```

在 `DatabaseConnectionInput` 之后追加接口:

```ts
export interface DatabaseQuerySubmitInput {
  connection_id: string;
  database?: string | null;
  sql: string;
  row_limit?: number;
}

export interface DatabaseQueryResult {
  task_id: string;
  connection_id: string;
  database_name: string;
  query_text: string;
  row_count: number;
  truncated: boolean;
  duration_ms: number;
  csv_path: string;
  columns: string[];
  rows: (string | null)[][];
  created_at: string;
}

export interface DatabaseQuerySummary {
  task_id: string;
  database_name: string;
  query_text: string;
  row_count: number;
  truncated: boolean;
  duration_ms: number;
  created_at: string;
}
```

`desktop` 对象中 `listDatabaseTables` 替换并追加方法(现有 `listDatabaseTables: (id: string) => call<string[]>("list_database_tables", { id })` 改为):

```ts
  listDatabaseTables: (id: string, database?: string) =>
    call<string[]>("list_database_tables", { id, databaseName: database ?? null }),
  listDatabases: (id: string) => call<string[]>("list_databases", { id }),
  submitDatabaseQuery: (input: DatabaseQuerySubmitInput) =>
    call<BackgroundTask>("submit_database_query", { input }),
  getDatabaseQueryResult: (taskId: string) =>
    call<DatabaseQueryResult | null>("get_database_query_result", { taskId }),
  listDatabaseQueryResults: () =>
    call<DatabaseQuerySummary[]>("list_database_query_results"),
```

注意 Tauri 2 参数命名:Rust 命令参数 `database_name: Option<String>` / `task_id: String` 对应前端 camelCase 键 `databaseName` / `taskId`(Tauri 自动转换)。

- [ ] **Step 3: 验证编译**

Run: `Set-Location frontend; npm run build`
Expected: tsc + Vite 构建通过(`DatabaseConnectionsPanel` 等现有消费方若因 Summary 新字段报缺字段错,在相应 mock/fixture 中补 4 个 null 字段)。

- [ ] **Step 4: Commit**

```powershell
git add frontend/src/bridge/desktop.ts frontend/src/features/settings/DatabaseConnectionsPanel.test.tsx frontend/src/features/settings/SettingsPage.test.tsx
git commit -m "扩展桌面桥接的数据库查询接口"
```

(仅添加实际被修改的测试文件路径。)

---

### Task 8: 一级导航 + DatabasePage 骨架

**Files:**
- Modify: `frontend/src/app/navigation.ts`
- Modify: `frontend/src/app/BloomeryApp.tsx`
- Modify: `frontend/src/app/BloomeryApp.test.tsx`
- Create: `frontend/src/features/databases/DatabasePage.tsx`
- Create: `frontend/src/features/databases/DatabasePage.test.tsx`
- Modify: `frontend/src/i18n/locale.tsx`

- [ ] **Step 1: 写失败的导航测试**

`frontend/src/app/BloomeryApp.test.tsx` 参照现有用例(该文件已有渲染外壳并点击导航的测试)追加:

```tsx
it("renders the databases section", async () => {
  renderBloomeryApp();
  const button = await screen.findByRole("button", { name: "navDatabases" });
  fireEvent.click(button);
  expect(await screen.findByRole("heading", { name: "dbTitle" })).toBeInTheDocument();
});
```

(具体辅助函数名 `renderBloomeryApp` 以文件内现有写法为准;mock desktop 需补 `listDatabaseConnections` 等新方法,见 Step 3。)

新建 `frontend/src/features/databases/DatabasePage.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DatabasePage from "./DatabasePage";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    preference: "zh-CN",
    setPreference: vi.fn(),
    t: (key: string) => key,
  }),
}));

vi.mock("../../bridge/desktop", () => ({
  isDesktopRuntime: vi.fn().mockReturnValue(false),
  desktop: {
    listDatabaseConnections: vi.fn(),
    listDatabases: vi.fn(),
    listDatabaseTables: vi.fn(),
    submitDatabaseQuery: vi.fn(),
    getDatabaseQueryResult: vi.fn(),
    listDatabaseQueryResults: vi.fn(),
    cancelBackgroundTask: vi.fn(),
    listBackgroundTasks: vi.fn(),
    saveSteelDataset: vi.fn(),
    activateSteelDataset: vi.fn(),
  },
}));

import { desktop } from "../../bridge/desktop";

const connection = {
  id: "c1",
  display_name: "3 号高炉",
  host: "192.168.1.10",
  port: 1433,
  username: "sa",
  timeout_ms: 10000,
  enabled: true,
  secret_configured: true,
  last_checked_at: null,
  last_latency_ms: null,
  last_version: null,
  last_error: null,
};

describe("DatabasePage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
    vi.mocked(desktop.listDatabases).mockResolvedValue(["master", "SteelWorks"]);
    vi.mocked(desktop.listDatabaseTables).mockResolvedValue(["dbo.heats", "dbo.chemistry"]);
    vi.mocked(desktop.listDatabaseQueryResults).mockResolvedValue([]);
    vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([]);
  });

  it("renders workspace landmarks", async () => {
    render(<DatabasePage />);
    expect(await screen.findByRole("heading", { name: "dbTitle" })).toBeInTheDocument();
    expect(await screen.findByRole("combobox", { name: "dbConnectionLabel" })).toBeInTheDocument();
    expect(await screen.findByRole("combobox", { name: "dbDatabaseLabel" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "dbSqlLabel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "dbRun" })).toBeInTheDocument();
  });

  it("shows empty guidance without connections", async () => {
    vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([]);
    render(<DatabasePage />);
    expect(await screen.findByText("dbEmptyConnections")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location frontend; npm test -- DatabasePage.test.tsx BloomeryApp.test.tsx`
Expected: FAIL(导航按钮/页面不存在)。

- [ ] **Step 3: 实现**

`frontend/src/i18n/locale.tsx` -- zhCN 字典(navigation 附近)追加,同时 en-US 字典追加对应英文:

```ts
navDatabases: "数据库",
navDatabasesDescription: "SQL Server 连接与只读查询",
dbTitle: "数据库工作台",
dbEmptyConnections: "还没有可用的数据库连接，请先在设置页添加并启用",
dbConnectionLabel: "连接",
dbDatabaseLabel: "数据库",
dbTables: "表",
dbSqlLabel: "查询",
dbRowLimit: "行数上限",
dbRun: "运行查询",
dbCancel: "取消",
dbRunning: "查询执行中",
dbResultEmpty: "运行查询后在此查看结果",
dbTruncatedNotice: "已达到行数上限 {count} 行，结果被截断",
dbDuration: "耗时 {ms} 毫秒",
dbSendToAnalysis: "送入数据分析",
dbSending: "正在保存数据集",
dbSent: "已保存并激活数据集，正在前往数据分析",
dbHistory: "最近查询",
dbQueryFailed: "查询失败",
dbSendError: "保存数据集失败",
dbLoadError: "加载数据库信息失败",
```

en-US 对应:

```ts
navDatabases: "Databases",
navDatabasesDescription: "SQL Server connections and read-only queries",
dbTitle: "Database workspace",
dbEmptyConnections: "No enabled database connection yet. Add and enable one in Settings first",
dbConnectionLabel: "Connection",
dbDatabaseLabel: "Database",
dbTables: "Tables",
dbSqlLabel: "Query",
dbRowLimit: "Row limit",
dbRun: "Run query",
dbCancel: "Cancel",
dbRunning: "Query running",
dbResultEmpty: "Run a query to see results here",
dbTruncatedNotice: "Row limit of {count} reached; results truncated",
dbDuration: "{ms} ms elapsed",
dbSendToAnalysis: "Send to data analysis",
dbSending: "Saving dataset",
dbSent: "Dataset saved and activated; opening data analysis",
dbHistory: "Recent queries",
dbQueryFailed: "Query failed",
dbSendError: "Failed to save dataset",
dbLoadError: "Failed to load database information",
```

`frontend/src/app/navigation.ts`:`SectionId` 联合加 `"databases"`;`primaryNavigationSections` 的 knowledge 之后插入:

```ts
  { id: "databases", labelKey: "navDatabases", descriptionKey: "navDatabasesDescription", icon: Database },
```

顶部 lucide 导入加 `Database`。

`frontend/src/app/BloomeryApp.tsx` 渲染链(`activeSection === "knowledge"` 分支后)插入:

```tsx
    ) : activeSection === "databases" ? (
      <DatabasePage />
```

并加 `import DatabasePage from "../features/databases/DatabasePage";`

新建 `frontend/src/features/databases/DatabasePage.tsx`(骨架版,后续任务扩展):

```tsx
import { useEffect, useState } from "react";
import { Database } from "lucide-react";
import { desktop, type DatabaseConnectionSummary } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

export default function DatabasePage() {
  const { t } = useLocale();
  const [connections, setConnections] = useState<DatabaseConnectionSummary[]>([]);
  const [connectionId, setConnectionId] = useState<string>("");
  const [databases, setDatabases] = useState<string[]>([]);
  const [databaseName, setDatabaseName] = useState<string>("");
  const [tables, setTables] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    desktop
      .listDatabaseConnections()
      .then((items) => {
        if (!mounted) return;
        const enabled = items.filter((item) => item.enabled && item.secret_configured);
        setConnections(enabled);
        setConnectionId(enabled[0]?.id ?? "");
      })
      .catch(() => mounted && setError(t("dbLoadError")))
      .finally(() => mounted && setLoading(false));
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!connectionId) return;
    let mounted = true;
    setError(null);
    Promise.all([desktop.listDatabases(connectionId), desktop.listDatabaseTables(connectionId)])
      .then(([names, tableNames]) => {
        if (!mounted) return;
        setDatabases(names);
        setTables(tableNames);
      })
      .catch((cause) => mounted && setError(cause instanceof Error ? cause.message : t("dbLoadError")));
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionId]);

  return (
    <div className="bloomery-db bloomery-page-surface">
      <header className="bloomery-db-header">
        <h1 id="db-heading">{t("dbTitle")}</h1>
      </header>
      {error && (
        <div className="bloomery-settings-alert" role="alert">
          <span>{error}</span>
        </div>
      )}
      {loading ? null : connections.length === 0 ? (
        <div className="bloomery-extensions-empty">
          <Database size={18} aria-hidden="true" />
          <span>{t("dbEmptyConnections")}</span>
        </div>
      ) : (
        <div className="bloomery-db-toolbar">
          <label>
            <span>{t("dbConnectionLabel")}</span>
            <select
              aria-label={t("dbConnectionLabel")}
              value={connectionId}
              onChange={(event) => setConnectionId(event.target.value)}
            >
              {connections.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.display_name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>{t("dbDatabaseLabel")}</span>
            <select
              aria-label={t("dbDatabaseLabel")}
              value={databaseName}
              onChange={(event) => setDatabaseName(event.target.value)}
            >
              <option value="">{t("dbDatabaseLabel")}</option>
              {databases.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}
    </div>
  );
}
```

注:骨架版先渲染连接/库选择器;SQL 编辑器、表浏览、结果区在 Task 9/10 加入。为让 Step 1 测试通过,骨架需同时包含 `dbSqlLabel` textbox 与 `dbRun` 按钮 -- 在 toolbar 后补最小占位:

```tsx
      <label className="bloomery-db-sql">
        <span>{t("dbSqlLabel")}</span>
        <textarea aria-label={t("dbSqlLabel")} rows={5} />
      </label>
      <button type="button" className="bloomery-action-primary" aria-label={t("dbRun")}>
        {t("dbRun")}
      </button>
```

(占位的 textarea/button 用受控 state 包好,Task 9 直接扩展。)

`BloomeryApp.test.tsx` 的 desktop mock 对象补齐:`listDatabaseConnections: vi.fn().mockResolvedValue([])`(以及 `listDatabases/listDatabaseTables/listDatabaseQueryResults/listBackgroundTasks` 的空返回 mock)。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location frontend; npm test -- DatabasePage.test.tsx BloomeryApp.test.tsx; npm run build`
Expected: PASS + 构建通过。

- [ ] **Step 5: Commit**

```powershell
git add frontend/src/app/navigation.ts frontend/src/app/BloomeryApp.tsx frontend/src/app/BloomeryApp.test.tsx frontend/src/features/databases/DatabasePage.tsx frontend/src/features/databases/DatabasePage.test.tsx frontend/src/i18n/locale.tsx
git commit -m "新增数据库一级导航与工作台骨架"
```

---

### Task 9: 查询工作台 -- 编辑器/表浏览/提交/轮询/取消

**Files:**
- Modify: `frontend/src/features/databases/DatabasePage.tsx`
- Modify: `frontend/src/features/databases/DatabasePage.test.tsx`
- Modify: `frontend/src/design/polish.css`(`.bloomery-db-*` 样式)

- [ ] **Step 1: 写失败的测试**

`DatabasePage.test.tsx` 追加(mock 顶部导入 `wait` 用 `waitFor` 从 @testing-library/react):

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const runningTask = {
  id: "task-1",
  kind: "database_query",
  state: "running" as const,
  progress: 10,
  attempt: 1,
  error_code: null,
  cancel_requested: false,
  can_cancel: true,
  can_retry: false,
  created_at: "2026-08-18T10:00:00Z",
  updated_at: "2026-08-18T10:00:01Z",
};

it("submits a query and renders rows after completion", async () => {
  vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
  vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
    { ...runningTask, state: "completed" as const, progress: 100 },
  ]);
  vi.mocked(desktop.getDatabaseQueryResult).mockResolvedValue({
    task_id: "task-1",
    connection_id: "c1",
    database_name: "SteelWorks",
    query_text: "SELECT heat_id FROM dbo.heats",
    row_count: 2,
    truncated: false,
    duration_ms: 120,
    csv_path: "C:/cache/task-1.csv",
    columns: ["heat_id", "carbon_pct"],
    rows: [
      ["H1", "0.18"],
      ["H2", "0.21"],
    ],
    created_at: "2026-08-18T10:00:02Z",
  });

  render(<DatabasePage />);
  const editor = await screen.findByRole("textbox", { name: "dbSqlLabel" });
  fireEvent.change(editor, { target: { value: "SELECT heat_id FROM dbo.heats" } });
  fireEvent.click(screen.getByRole("button", { name: "dbRun" }));

  expect(desktop.submitDatabaseQuery).toHaveBeenCalledWith({
    connection_id: "c1",
    database: "",
    sql: "SELECT heat_id FROM dbo.heats",
    row_limit: 500,
  });
  expect(await screen.findByRole("table")).toBeInTheDocument();
  expect(await screen.findByText("H2")).toBeInTheDocument();
  expect(screen.queryByText("dbTruncatedNotice")).not.toBeInTheDocument();
});

it("cancels a running query", async () => {
  vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
  vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([runningTask]);
  vi.mocked(desktop.cancelBackgroundTask).mockResolvedValue({ ...runningTask, cancel_requested: true });

  render(<DatabasePage />);
  fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), {
    target: { value: "SELECT 1" },
  });
  fireEvent.click(screen.getByRole("button", { name: "dbRun" }));
  const cancel = await screen.findByRole("button", { name: "dbCancel" });
  fireEvent.click(cancel);
  expect(desktop.cancelBackgroundTask).toHaveBeenCalledWith("task-1");
});

it("fills the editor from a table name click", async () => {
  render(<DatabasePage />);
  const tableButton = await screen.findByRole("button", { name: "dbo.heats" });
  fireEvent.click(tableButton);
  const editor = screen.getByRole("textbox", { name: "dbSqlLabel" }) as HTMLTextAreaElement;
  expect(editor.value).toContain("SELECT TOP (500) * FROM [dbo].[heats]");
});

it("shows truncated notice", async () => {
  vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
  vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
    { ...runningTask, state: "completed" as const, progress: 100 },
  ]);
  vi.mocked(desktop.getDatabaseQueryResult).mockResolvedValue({
    task_id: "task-1",
    connection_id: "c1",
    database_name: "",
    query_text: "SELECT 1",
    row_count: 500,
    truncated: true,
    duration_ms: 90,
    csv_path: "C:/cache/task-1.csv",
    columns: ["n"],
    rows: Array.from({ length: 500 }, () => ["1"]),
    created_at: "2026-08-18T10:00:02Z",
  });

  render(<DatabasePage />);
  fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), {
    target: { value: "SELECT 1" },
  });
  fireEvent.click(screen.getByRole("button", { name: "dbRun" }));
  expect(await screen.findByText("dbTruncatedNotice:500")).toBeInTheDocument();
});
```

注意:骨架版 `t` mock 需支持插值以匹配 `dbTruncatedNotice:500`:

```tsx
t: (key: string, params?: Record<string, string | number>) =>
  params ? `${key}:${JSON.stringify(params)}` : key,
```

按 JSON 序渲染 `{count:500}`;断言相应调整为 `findByText(/dbTruncatedNotice/)`,以实际输出为准。

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location frontend; npm test -- DatabasePage.test.tsx`
Expected: FAIL。

- [ ] **Step 3: 实现**

`DatabasePage.tsx` 重写为完整工作台(保留 Task 8 的加载逻辑,新增以下状态与逻辑):

```tsx
const POLL_INTERVAL_MS = 500;
const ROW_LIMITS = [100, 500, 1000, 5000];
const TERMINAL_STATES = ["completed", "failed", "cancelled", "interrupted"];
const isTerminal = (state: string) => TERMINAL_STATES.includes(state);

// 新增 state:
const [sql, setSql] = useState("");
const [rowLimit, setRowLimit] = useState(500);
const [task, setTask] = useState<BackgroundTask | null>(null);
const [result, setResult] = useState<DatabaseQueryResult | null>(null);
const [busy, setBusy] = useState(false);

const run = async () => {
  if (!connectionId || busy || !sql.trim()) return;
  setBusy(true);
  setError(null);
  try {
    const queued = await desktop.submitDatabaseQuery({
      connection_id: connectionId,
      database: databaseName || null,
      sql,
      row_limit: rowLimit,
    });
    setTask(queued);
    setResult(null);
  } catch (cause) {
    setError(cause instanceof Error ? cause.message : t("dbQueryFailed"));
    setTask(null);
  } finally {
    setBusy(false);
  }
};

const cancel = async () => {
  if (!task) return;
  try {
    await desktop.cancelBackgroundTask(task.id);
  } catch (cause) {
    setError(cause instanceof Error ? cause.message : t("dbQueryFailed"));
  }
};

useEffect(() => {
  if (!task || isTerminal(task.state)) return;
  let mounted = true;
  const refresh = async () => {
    try {
      const tasks = await desktop.listBackgroundTasks();
      const current = tasks.find((candidate) => candidate.id === task.id);
      if (!mounted || !current) return;
      setTask(current);
      if (current.state === "completed") {
        const next = await desktop.getDatabaseQueryResult(current.id);
        if (mounted) setResult(next);
      }
    } catch {
      /* 轮询失败下次重试 */
    }
  };
  void refresh();
  const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
  return () => {
    mounted = false;
    window.clearInterval(timer);
  };
}, [task]);
```

表浏览侧栏(连接/库加载完成后渲染):

```tsx
<aside className="bloomery-db-tables" aria-label={t("dbTables")}>
  <h2>{t("dbTables")}</h2>
  {tables.map((name) => (
    <button
      key={name}
      type="button"
      className="bloomery-db-table-button"
      onClick={() => setSql(`SELECT TOP (${rowLimit}) * FROM [${name.replace(".", "].[")}]`)}
    >
      <code>{name}</code>
    </button>
  ))}
</aside>
```

编辑器 + 动作区(toolbar 后):

```tsx
<div className="bloomery-db-editor">
  <label>
    <span>{t("dbSqlLabel")}</span>
    <textarea
      aria-label={t("dbSqlLabel")}
      rows={6}
      className="bloomery-db-sql-input"
      value={sql}
      onChange={(event) => setSql(event.target.value)}
      spellCheck={false}
    />
  </label>
  <div className="bloomery-db-actions">
    <label>
      <span>{t("dbRowLimit")}</span>
      <select
        aria-label={t("dbRowLimit")}
        value={rowLimit}
        onChange={(event) => setRowLimit(Number(event.target.value))}
      >
        {ROW_LIMITS.map((limit) => (
          <option key={limit} value={limit}>
            {limit}
          </option>
        ))}
      </select>
    </label>
    {task && !isTerminal(task.state) ? (
      <button type="button" className="bloomery-action-secondary" onClick={() => void cancel()} aria-label={t("dbCancel")}>
        {t("dbCancel")}
      </button>
    ) : (
      <button type="button" className="bloomery-action-primary" onClick={() => void run()} disabled={busy} aria-label={t("dbRun")}>
        {t("dbRun")}
      </button>
    )}
    {task && !isTerminal(task.state) && <span aria-live="polite">{t("dbRunning")}</span>}
  </div>
</div>
```

`polish.css` 末尾追加:

```css
.bloomery-db {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 22px;
  min-height: 0;
}

.bloomery-db-toolbar {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  align-items: end;
}

.bloomery-db-toolbar label,
.bloomery-db-actions label {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
  color: var(--bloomery-text-muted);
}

.bloomery-db-body {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  gap: 16px;
  min-height: 0;
  flex: 1;
}

.bloomery-db-tables {
  border: 1px solid var(--bloomery-line);
  border-radius: var(--bloomery-radius);
  padding: 12px;
  overflow: auto;
  background: var(--bloomery-bg-raised);
}

.bloomery-db-table-button {
  display: block;
  width: 100%;
  text-align: left;
  padding: 6px 8px;
  border-radius: var(--bloomery-radius-small);
  background: transparent;
  border: none;
  cursor: pointer;
  color: var(--bloomery-text);
}

.bloomery-db-table-button:hover {
  background: var(--bloomery-bg-hover);
}

.bloomery-db-sql-input {
  font-family: var(--bloomery-mono);
  width: 100%;
  border-radius: var(--bloomery-radius-small);
  border: 1px solid var(--bloomery-line);
  padding: 10px;
  resize: vertical;
}

.bloomery-db-actions {
  display: flex;
  gap: 12px;
  align-items: end;
}

.bloomery-db-result {
  border: 1px solid var(--bloomery-line);
  border-radius: var(--bloomery-radius);
  overflow: auto;
  max-height: 420px;
}

.bloomery-db-result table {
  border-collapse: collapse;
  width: 100%;
  font-size: 13px;
}

.bloomery-db-result th,
.bloomery-db-result td {
  border-bottom: 1px solid var(--bloomery-line);
  padding: 6px 10px;
  text-align: left;
  white-space: nowrap;
}

.bloomery-db-result th {
  position: sticky;
  top: 0;
  background: var(--bloomery-bg-soft);
}

[data-theme="dark"] .bloomery-db-tables,
[data-theme="dark"] .bloomery-db-result {
  background: var(--bloomery-bg-raised);
}
```

(布局类 `bloomery-db-body` 用于包住表浏览侧栏与编辑器/结果列,实现时按此结构包 JSX。)

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location frontend; npm test -- DatabasePage.test.tsx`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add frontend/src/features/databases/DatabasePage.tsx frontend/src/features/databases/DatabasePage.test.tsx frontend/src/design/polish.css
git commit -m "支持数据库工作台查询执行与表浏览"
```

---

### Task 10: 结果表格 + 历史 + 送入分析

**Files:**
- Modify: `frontend/src/features/databases/DatabasePage.tsx`
- Modify: `frontend/src/features/databases/DatabasePage.test.tsx`

- [ ] **Step 1: 写失败的测试**

`DatabasePage.test.tsx` 追加:

```tsx
it("sends a result to data analysis", async () => {
  vi.mocked(desktop.submitDatabaseQuery).mockResolvedValue(runningTask);
  vi.mocked(desktop.listBackgroundTasks).mockResolvedValue([
    { ...runningTask, state: "completed" as const, progress: 100 },
  ]);
  vi.mocked(desktop.getDatabaseQueryResult).mockResolvedValue({
    task_id: "task-1", connection_id: "c1", database_name: "", query_text: "SELECT 1",
    row_count: 1, truncated: false, duration_ms: 50, csv_path: "C:/cache/task-1.csv",
    columns: ["n"], rows: [["1"]], created_at: "2026-08-18T10:00:02Z",
  });
  vi.mocked(desktop.saveSteelDataset).mockResolvedValue({
    id: "ds-1",
    name: "task-1",
    sourcePath: "C:/cache/task-1.csv",
    sourceSha256: "hash",
    selectedSheet: "",
    mappingState: "preview",
    columns: [],
    preview: { columns: [], rowCount: 1, truncated: false, sheets: [] },
  } as never);
  vi.mocked(desktop.activateSteelDataset).mockResolvedValue({} as never);
  const onOpenSection = vi.fn();

  render(<DatabasePage onOpenSection={onOpenSection} />);
  fireEvent.change(await screen.findByRole("textbox", { name: "dbSqlLabel" }), { target: { value: "SELECT 1" } });
  fireEvent.click(screen.getByRole("button", { name: "dbRun" }));
  const send = await screen.findByRole("button", { name: "dbSendToAnalysis" });
  fireEvent.click(send);

  await waitFor(() => expect(desktop.saveSteelDataset).toHaveBeenCalledWith({ sourcePath: "C:/cache/task-1.csv" }));
  await waitFor(() => expect(desktop.activateSteelDataset).toHaveBeenCalledWith("ds-1"));
  expect(onOpenSection).toHaveBeenCalledWith("analysis");
});

it("fills the editor from history", async () => {
  vi.mocked(desktop.listDatabaseQueryResults).mockResolvedValue([
    {
      task_id: "old-1", database_name: "SteelWorks", query_text: "SELECT TOP (10) * FROM dbo.heats",
      row_count: 10, truncated: false, duration_ms: 300, created_at: "2026-08-18T09:00:00Z",
    },
  ]);
  render(<DatabasePage />);
  const historyItem = await screen.findByRole("button", { name: /SELECT TOP \(10\) \* FROM dbo\.heats/ });
  fireEvent.click(historyItem);
  const editor = screen.getByRole("textbox", { name: "dbSqlLabel" }) as HTMLTextAreaElement;
  expect(editor.value).toBe("SELECT TOP (10) * FROM dbo.heats");
});
```

(`saveSteelDataset` 返回的 fixture 字段以 `bridge/desktop.ts` 中 `SteelDatasetRecord` 实际字段为准,`as never` 仅绕过测试 fixture 的类型宽松。)

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location frontend; npm test -- DatabasePage.test.tsx`
Expected: FAIL。

- [ ] **Step 3: 实现**

`DatabasePage.tsx`:

组件签名改为:

```tsx
export default function DatabasePage({
  onOpenSection,
}: {
  onOpenSection?: (section: "analysis" | "settings") => void;
}) {
```

新增 state 与函数:

```tsx
const [history, setHistory] = useState<DatabaseQuerySummary[]>([]);
const [sending, setSending] = useState(false);
const [notice, setNotice] = useState<string | null>(null);

const refreshHistory = async () => {
  try {
    setHistory(await desktop.listDatabaseQueryResults());
  } catch {
    /* 历史加载失败不打断主流程 */
  }
};

useEffect(() => {
  void refreshHistory();
}, []);

// 查询完成后刷新历史:在轮询 useEffect 的 current.state === "completed" 分支中,
// setResult(next) 之后追加:
//   void refreshHistory();

const sendToAnalysis = async () => {
  if (!result || sending) return;
  setSending(true);
  setError(null);
  setNotice(null);
  try {
    const saved = await desktop.saveSteelDataset({ sourcePath: result.csv_path });
    await desktop.activateSteelDataset(saved.id);
    setNotice(t("dbSent"));
    onOpenSection?.("analysis");
  } catch (cause) {
    setError(cause instanceof Error ? cause.message : t("dbSendError"));
  } finally {
    setSending(false);
  }
};
```

结果区 JSX(编辑器之后;空态/结果态):

```tsx
<section className="bloomery-db-result-section" aria-label={t("dbResultsTitle")}>
  {result ? (
    <>
      <div className="bloomery-db-result-meta">
        <span>{t("dbDuration", { ms: result.duration_ms })}</span>
        {result.truncated && (
          <span className="bloomery-db-truncated" role="status">
            {t("dbTruncatedNotice", { count: result.row_count })}
          </span>
        )}
        <button
          type="button"
          className="bloomery-action-secondary"
          onClick={() => void sendToAnalysis()}
          disabled={sending}
          aria-label={t("dbSendToAnalysis")}
        >
          {sending ? t("dbSending") : t("dbSendToAnalysis")}
        </button>
      </div>
      <div className="bloomery-db-result">
        <table>
          <thead>
            <tr>{result.columns.map((column) => <th key={column} scope="col">{column}</th>)}</tr>
          </thead>
          <tbody>
            {result.rows.map((row, index) => (
              <tr key={index}>
                {row.map((cell, cellIndex) => <td key={cellIndex}>{cell ?? ""}</td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  ) : (
    <div className="bloomery-extensions-empty"><span>{t("dbResultEmpty")}</span></div>
  )}
</section>

<aside className="bloomery-db-history" aria-label={t("dbHistory")}>
  <h2>{t("dbHistory")}</h2>
  {history.map((item) => (
    <button
      key={item.task_id}
      type="button"
      className="bloomery-db-table-button"
      title={item.query_text}
      onClick={() => setSql(item.query_text)}
    >
      <code>{item.query_text}</code>
      <span className="bloomery-db-history-meta">
        {item.database_name || connectionName(connectionId)} · {item.row_count}
      </span>
    </button>
  ))}
</aside>
```

辅助:

```tsx
const connectionName = (id: string) => connections.find((item) => item.id === id)?.display_name ?? id;
```

i18n 补充 key(zh / en 同步):

```ts
dbResultsTitle: "查询结果",
```
```ts
dbResultsTitle: "Query results",
```

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location frontend; npm test -- DatabasePage.test.tsx`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add frontend/src/features/databases/DatabasePage.tsx frontend/src/features/databases/DatabasePage.test.tsx frontend/src/i18n/locale.tsx
git commit -m "支持查询结果展示历史与送入数据分析"
```

---

### Task 11: 设置页标签导航

**Files:**
- Modify: `frontend/src/features/settings/SettingsPage.tsx`
- Modify: `frontend/src/features/settings/SettingsPage.test.tsx`
- Modify: `frontend/src/i18n/locale.tsx`
- Modify: `frontend/src/design/polish.css`

- [ ] **Step 1: 写失败的测试**

`SettingsPage.test.tsx` 追加(mock 已有的 bridge 方法维持不变):

```tsx
it("shows database panel only on the databases tab", async () => {
  renderSettingsPage();
  expect(screen.queryByRole("heading", { name: "settingsDatabaseTitle" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab", { name: "settingsTabDatabases" }));
  expect(await screen.findByRole("heading", { name: "settingsDatabaseTitle" })).toBeInTheDocument();
});

it("keeps provider cards on the providers tab", async () => {
  renderSettingsPage();
  fireEvent.click(screen.getByRole("tab", { name: "settingsTabProviders" }));
  expect(await screen.findByRole("heading", { name: "settingsDatabaseTitle" })).not.toBeInTheDocument();
  expect(screen.getByRole("tabpanel")).toBeInTheDocument();
});
```

(`renderSettingsPage` 为该文件现有的渲染辅助,名字以文件实际为准;现有用例若直接假设所有面板同屏,需同步在各断言前先点击对应 tab。)

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location frontend; npm test -- SettingsPage.test.tsx`
Expected: FAIL(无 tablist)。

- [ ] **Step 3: 实现**

i18n 追加(zh / en 同步):

```ts
settingsTabProviders: "模型服务",
settingsTabGeneral: "通用",
settingsTabPermissions: "权限",
settingsTabDatabases: "数据库",
```
```ts
settingsTabProviders: "Providers",
settingsTabGeneral: "General",
settingsTabPermissions: "Permissions",
settingsTabDatabases: "Databases",
```

`SettingsPage.tsx`:

新增类型与 state(与其他 state 并列):

```tsx
type SettingsTab = "providers" | "general" | "permissions" | "databases";
const [activeTab, setActiveTab] = useState<SettingsTab>("providers");

const settingsTabs: { id: SettingsTab; labelKey: MessageKey }[] = [
  { id: "providers", labelKey: "settingsTabProviders" },
  { id: "general", labelKey: "settingsTabGeneral" },
  { id: "permissions", labelKey: "settingsTabPermissions" },
  { id: "databases", labelKey: "settingsTabDatabases" },
];
```

(`MessageKey` 从 i18n 导入。)

JSX 改造:header/alert/notice 保持原位;其后插入 tablist,再把原有面板按 tab 分组:

```tsx
<div className="bloomery-settings-tabs" role="tablist" aria-label={t("settingsTitle")}>
  {settingsTabs.map((tab) => (
    <button
      key={tab.id}
      type="button"
      role="tab"
      id={`settings-tab-${tab.id}`}
      aria-selected={activeTab === tab.id}
      aria-controls={`settings-panel-${tab.id}`}
      className={`bloomery-settings-tab ${activeTab === tab.id ? "is-active" : ""}`}
      onClick={() => setActiveTab(tab.id)}
    >
      {t(tab.labelKey)}
    </button>
  ))}
</div>

<div role="tabpanel" id={`settings-panel-${activeTab}`} aria-labelledby={`settings-tab-${activeTab}`}>
  {activeTab === "general" && (
    <>
      <div className="bloomery-settings-safety">…原安全提示 JSX 原样移入…</div>
      <ThemeSelect />
    </>
  )}
  {activeTab === "permissions" && (
    <PermissionRulesPanel rules={permissionRules} busyId={permissionBusyId} onRevoke={(rule) => void revokePermission(rule)} />
  )}
  {activeTab === "databases" && <DatabaseConnectionsPanel />}
  {activeTab === "providers" && (
    <>
      <section className="bloomery-settings-plan">…原 plan section 原样移入…</section>
      {loading ? <div className="bloomery-settings-loading">…原样…</div> : (
        <div className="bloomery-settings-grid">…原 editors.map 原样移入…</div>
      )}
    </>
  )}
</div>
```

`polish.css` 追加:

```css
.bloomery-settings-tabs {
  display: flex;
  gap: 8px;
  border-bottom: 1px solid var(--bloomery-line);
  padding-bottom: 0;
}

.bloomery-settings-tab {
  border: none;
  background: transparent;
  padding: 10px 16px;
  border-radius: var(--bloomery-radius-small) var(--bloomery-radius-small) 0 0;
  cursor: pointer;
  color: var(--bloomery-text-muted);
  border-bottom: 2px solid transparent;
}

.bloomery-settings-tab.is-active {
  color: var(--bloomery-text);
  border-bottom-color: var(--bloomery-accent);
  font-weight: 600;
}

[data-theme="dark"] .bloomery-settings-tab.is-active {
  color: var(--bloomery-text);
}
```

- [ ] **Step 4: 运行确认通过(含既有用例修复)**

Run: `Set-Location frontend; npm test -- SettingsPage.test.tsx`
Expected: PASS。既有用例因分 tab 需补点击步骤的,逐个修复(只加 `fireEvent.click(screen.getByRole("tab", {...}))` 前置动作,不改断言语义)。

- [ ] **Step 5: Commit**

```powershell
git add frontend/src/features/settings/SettingsPage.tsx frontend/src/features/settings/SettingsPage.test.tsx frontend/src/i18n/locale.tsx frontend/src/design/polish.css
git commit -m "设置页改为标签导航"
```

---

### Task 12: DatabaseConnectionsPanel 增强

**Files:**
- Modify: `frontend/src/features/settings/DatabaseConnectionsPanel.tsx`
- Modify: `frontend/src/features/settings/DatabaseConnectionsPanel.test.tsx`
- Modify: `frontend/src/i18n/locale.tsx`

- [ ] **Step 1: 写失败的测试**

`DatabaseConnectionsPanel.test.tsx` 追加(fixture 补 4 个健康字段 null):

```tsx
it("shows health badge after a test", async () => {
  vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([
    { ...connection, last_checked_at: "2026-08-18T09:00:00+08:00", last_latency_ms: 120, last_version: "Microsoft SQL Server 2022", last_error: null },
  ]);
  render(<DatabaseConnectionsPanel />);
  expect(await screen.findByText(/Microsoft SQL Server 2022/)).toBeInTheDocument();
  expect(screen.getByText(/settingsDatabaseLatency/)).toBeInTheDocument();
});

it("toggles a connection enabled state", async () => {
  vi.mocked(desktop.saveDatabaseConnection).mockResolvedValue(connection);
  render(<DatabaseConnectionsPanel />);
  const toggle = await screen.findByRole("switch", { name: "settingsDatabaseEnabled" });
  fireEvent.click(toggle);
  expect(desktop.saveDatabaseConnection).toHaveBeenCalledWith(
    expect.objectContaining({ id: connection.id, enabled: false })
  );
});

it("edits the timeout", async () => {
  vi.mocked(desktop.saveDatabaseConnection).mockResolvedValue(connection);
  render(<DatabaseConnectionsPanel />);
  fireEvent.change(await screen.findByLabelText("settingsDatabaseTimeout"), { target: { value: "20000" } });
  fireEvent.click(screen.getByRole("button", { name: "settingsDatabaseSave" }));
  expect(desktop.saveDatabaseConnection).toHaveBeenCalledWith(
    expect.objectContaining({ timeout_ms: 20000 })
  );
});

it("warns about duplicate host, port, and username", async () => {
  vi.mocked(desktop.listDatabaseConnections).mockResolvedValue([connection]);
  render(<DatabaseConnectionsPanel />);
  fireEvent.change(await screen.findByLabelText("settingsDatabaseHost"), { target: { value: connection.host } });
  fireEvent.change(screen.getByLabelText("settingsDatabasePort"), { target: { value: String(connection.port) } });
  fireEvent.change(screen.getByLabelText("settingsDatabaseUsername"), { target: { value: connection.username } });
  expect(await screen.findByText("settingsDatabaseDuplicate")).toBeInTheDocument();
});
```

(表单输入当前用 `<label><span>文案</span><input/></label>` 包裹结构,`findByLabelText` 可用;若不可达,给 input 加显式 `aria-label={t(...)}`,本步骤一并实现。)

- [ ] **Step 2: 运行确认失败**

Run: `Set-Location frontend; npm test -- DatabaseConnectionsPanel.test.tsx`
Expected: FAIL。

- [ ] **Step 3: 实现**

i18n 追加(zh / en 同步):

```ts
settingsDatabaseEnabled: "启用",
settingsDatabaseTimeout: "超时(毫秒)",
settingsDatabaseLatency: "延迟 {ms} 毫秒",
settingsDatabaseLastChecked: "上次检测",
settingsDatabaseDuplicate: "已存在相同主机、端口和用户名的连接",
```
```ts
settingsDatabaseEnabled: "Enabled",
settingsDatabaseTimeout: "Timeout (ms)",
settingsDatabaseLatency: "{ms} ms latency",
settingsDatabaseLastChecked: "Last checked",
settingsDatabaseDuplicate: "A connection with the same host, port, and username already exists",
```

`DatabaseConnectionsPanel.tsx` 修改:

1. `Draft` 增加 `timeout_ms: string;` 与 `enabled: boolean;`;`emptyDraft()` 补 `timeout_ms: "10000", enabled: true`;`edit()` 补 `timeout_ms: String(connection.timeout_ms), enabled: connection.enabled`。
2. 表单加超时输入(端口输入之后)与启用开关(表单 actions 前):

```tsx
<label><span>{t("settingsDatabaseTimeout")}</span>
  <input
    type="number" min="1000" max="60000" step="500"
    aria-label={t("settingsDatabaseTimeout")}
    value={draft.timeout_ms}
    onChange={(event) => setDraft({ ...draft, timeout_ms: event.target.value })}
    required
  />
</label>
```

3. 保存 payload 追加 `timeout_ms: Number(draft.timeout_ms) || undefined, enabled: draft.enabled`。
4. 连接卡片动作区加启用开关:

```tsx
<button
  type="button"
  role="switch"
  aria-checked={connection.enabled}
  aria-label={t("settingsDatabaseEnabled")}
  title={t("settingsDatabaseEnabled")}
  className="bloomery-icon-button"
  disabled={busy !== null}
  onClick={() =>
    void saveExisting(connection, !connection.enabled)
  }
>
  {connection.enabled ? <ToggleRight size={16} aria-hidden="true" /> : <ToggleLeft size={16} aria-hidden="true" />}
</button>
```

新增保存函数(复用 save 的 payload 构造):

```tsx
const saveExisting = async (connection: DatabaseConnectionSummary, enabled: boolean) => {
  setBusy(`toggle:${connection.id}`);
  setError(null);
  try {
    const saved = await desktop.saveDatabaseConnection({
      id: connection.id,
      display_name: connection.display_name,
      host: connection.host,
      port: connection.port,
      username: connection.username,
      password: undefined,
      timeout_ms: connection.timeout_ms,
      enabled,
    });
    setConnections((current) => updateConnection(current, saved));
  } catch (cause) {
    setError(errorMessage(cause, t("settingsDatabaseSaveError")));
  } finally {
    setBusy(null);
  }
};
```

5. 健康徽标(卡片 heading 之后):

```tsx
{connection.last_checked_at && (
  <p className={connection.last_error ? "bloomery-mcp-error" : "bloomery-mcp-health is-healthy"}>
    {connection.last_error
      ? `${t("settingsDatabaseLastChecked")}: ${connection.last_error}`
      : `${connection.last_version ?? ""} · ${t("settingsDatabaseLatency", { ms: connection.last_latency_ms ?? 0 })}`}
  </p>
)}
```

6. 重复提示(表单 actions 前):

```tsx
{connections.some((item) =>
  item.id !== draft.id &&
  item.host === draft.host.trim() &&
  item.port === (Number(draft.port) || 0) &&
  item.username === draft.username.trim()
) && <p className="bloomery-settings-alert" role="status">{t("settingsDatabaseDuplicate")}</p>}
```

lucide 导入补 `ToggleLeft, ToggleRight`。

- [ ] **Step 4: 运行确认通过**

Run: `Set-Location frontend; npm test -- DatabaseConnectionsPanel.test.tsx SettingsPage.test.tsx`
Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add frontend/src/features/settings/DatabaseConnectionsPanel.tsx frontend/src/features/settings/DatabaseConnectionsPanel.test.tsx frontend/src/i18n/locale.tsx
git commit -m "增强数据库连接面板的健康显示与编辑"
```

---

### Task 13: 全量验证与边界检查

**Files:**
- 无新文件;修复本计划引入的问题时只动相关文件。

- [ ] **Step 1: 前端全量**

Run: `Set-Location frontend; npm test`
Expected: Vitest 全部 PASS(注意 BloomeryApp/WorkbenchHome 等既有测试若因新导航按钮的布局断言失败,按最小改动适配)。

- [ ] **Step 2: 前端构建与边界**

Run: `Set-Location frontend; npm run build; npm run test:boundaries`
Expected: tsc/Vite 退出码 0;边界脚本通过(新代码无 `/api/`、无新依赖)。

- [ ] **Step 3: Rust 全量**

Run: `Set-Location src-tauri; cargo check; cargo test`
Expected: 无错误;`tests/architecture.rs` 通过(注意 db.rs 追加注册后仍满足行数预算)。

- [ ] **Step 4: 检查 diff 范围**

Run: `Set-Location ../; git log --oneline -12; git status --short`
Expected: 本计划的提交都在;工作树剩余改动仅为他人既有未提交内容(与开始时清单一致)。

- [ ] **Step 5: 收尾提交(如有修复)**

```powershell
git add <仅本计划相关文件>
git commit -m "修复数据库工作台集成回归"
```

---

## 验收清单(与规格对照)

- [ ] 一级导航出现「数据库」,位于知识库与数据分析之间(Task 8)
- [ ] 查询 guard 拒绝一切非 SELECT/WITH、多语句、注释伪装(Task 3)
- [ ] 查询作为后台任务运行,可取消、出现在任务列表、有 started_at/finished_at(Task 5)
- [ ] 结果存 `database_query_results` + 缓存 CSV;历史最近 10 条(Task 2/5/6)
- [ ] 「送入数据分析」复用 saveSteelDataset 并跳转分析页(Task 10)
- [ ] `test_database_connection` 持久化健康;连接卡片显示徽标(Task 1/6/12)
- [ ] `enabled=false` 的连接被提交/列库/列表命令拒绝(Task 6)
- [ ] 设置页四个标签;数据库标签含连接管理(Task 11)
- [ ] 前端测试/构建/边界 + Rust check/test 全绿(Task 13)
