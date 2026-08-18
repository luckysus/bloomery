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
