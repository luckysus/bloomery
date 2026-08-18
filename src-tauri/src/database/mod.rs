pub mod catalog;
pub mod query;

use crate::storage::repositories::database_connections::DatabaseConnectionRecord;
use tiberius::{AuthMethod, Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncReadCompatExt;

pub const DEFAULT_PORT: u16 = 1433;
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

pub type SqlClient = Client<tokio_util::compat::Compat<TcpStream>>;

pub async fn connect(
    record: &DatabaseConnectionRecord,
    password: &str,
) -> Result<SqlClient, String> {
    let mut config = Config::new();
    config.host(&record.host);
    config.port(record.port);
    // 空库名 = 连登录默认库（sa 默认 master），由用户在对话里自选库
    if !record.database_name.trim().is_empty() {
        config.database(record.database_name.trim());
    }
    config.authentication(AuthMethod::sql_server(&record.username, password));

    let address = (record.host.as_str(), record.port);
    let timeout = std::time::Duration::from_millis(record.timeout_ms);
    let tcp = tokio::time::timeout(timeout, TcpStream::connect(address))
        .await
        .map_err(|_| {
            format!(
                "connect to {} timed out after {}ms",
                record.host, record.timeout_ms
            )
        })?
        .map_err(|error| format!("cannot reach {}:{} ({})", record.host, record.port, error))?;
    tcp.set_nodelay(true).map_err(|error| error.to_string())?;
    Client::connect(config, tcp.compat())
        .await
        .map_err(|error| format!("SQL Server login failed: {error}"))
}

pub async fn server_version(client: &mut SqlClient) -> Result<String, String> {
    let stream = client
        .query("SELECT @@VERSION", &[])
        .await
        .map_err(|error| error.to_string())?;
    let rows = stream
        .into_first_result()
        .await
        .map_err(|error| error.to_string())?;
    rows.into_iter()
        .next()
        .and_then(|row| row.get::<&str, _>(0).map(|value| value.to_string()))
        .ok_or_else(|| "SQL Server returned no version".to_string())
}

pub async fn table_names(client: &mut SqlClient) -> Result<Vec<String>, String> {
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
