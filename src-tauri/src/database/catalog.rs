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

pub async fn table_names(
    client: &mut SqlClient,
    database: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(name) = database {
        let use_statement = format!("USE {}", escape_identifier(name)?);
        client
            .simple_query(use_statement)
            .await
            .map_err(|error| format!("cannot switch to database {name}: {error}"))?;
    }
    crate::database::table_names(client).await
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
        .await
        .map_err(|error| format!("query failed: {error}"))?
        .map(|metadata| {
            metadata
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
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
                .map(|index| {
                    row.try_get::<&str, _>(index)
                        .ok()
                        .flatten()
                        .map(str::to_string)
                        .or_else(|| {
                            row.try_get::<i64, _>(index)
                                .ok()
                                .flatten()
                                .map(|value| value.to_string())
                        })
                        .or_else(|| {
                            row.try_get::<i32, _>(index)
                                .ok()
                                .flatten()
                                .map(|value| value.to_string())
                        })
                        .or_else(|| {
                            row.try_get::<f64, _>(index)
                                .ok()
                                .flatten()
                                .map(|value| value.to_string())
                        })
                        .or_else(|| {
                            row.try_get::<bool, _>(index).ok().flatten().map(|value| {
                                if value {
                                    "1".to_string()
                                } else {
                                    "0".to_string()
                                }
                            })
                        })
                })
                .collect()
        })
        .collect();
    Ok(QueryRows { columns, rows })
}

/// 最小 CSV 单元格转义;供查询结果缓存文件使用。
pub fn csv_cell(value: Option<&str>) -> String {
    match value {
        None => String::new(),
        Some(text) => {
            if text.contains(',')
                || text.contains('"')
                || text.contains('\n')
                || text.contains('\r')
            {
                format!("\"{}\"", text.replace('"', "\"\""))
            } else {
                text.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_identifier_brackets_and_doubles_closing_brackets() {
        assert_eq!(
            escape_identifier("SteelWorks").expect("plain"),
            "[SteelWorks]"
        );
        assert_eq!(escape_identifier(" my]db ").expect("trimmed"), "[my]]db]");
    }

    #[test]
    fn escape_identifier_rejects_empty() {
        assert!(escape_identifier("").is_err());
        assert!(escape_identifier("   ").is_err());
    }

    #[test]
    fn escape_identifier_rejects_long_names() {
        assert!(escape_identifier(&"a".repeat(129)).is_err());
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
