use super::{
    http::McpHttpConfig,
    sse::McpLegacySseConfig,
    stdio::{self, McpStderrCapture, McpStderrSnapshot, McpStdioConfig},
    McpClient, McpClientConfig, McpError,
};
use std::{future::Future, time::Duration};

const MCP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub enum McpTransportConfig {
    Stdio(McpStdioConfig),
    Http(McpHttpConfig),
    LegacySse(McpLegacySseConfig),
}

pub struct McpSupervisor {
    client: Option<McpClient>,
    transport: McpTransportConfig,
    client_config: McpClientConfig,
    stderr: Option<McpStderrCapture>,
}

impl McpSupervisor {
    pub async fn connect(
        transport: McpTransportConfig,
        client_config: McpClientConfig,
    ) -> Result<Self, McpError> {
        let (client, stderr) = connect_transport(transport.clone(), client_config.clone()).await?;
        Ok(Self {
            client: Some(client),
            transport,
            client_config,
            stderr,
        })
    }

    pub fn client(&self) -> Result<&McpClient, McpError> {
        self.client
            .as_ref()
            .ok_or_else(|| McpError::Transport("MCP supervisor is not connected".to_string()))
    }

    pub async fn stderr_snapshot(&self) -> Option<McpStderrSnapshot> {
        let stderr = self.stderr.as_ref()?;
        stderr.wait().await;
        Some(stderr.snapshot().await)
    }

    pub async fn restart(&mut self) -> Result<(), McpError> {
        if let Some(client) = self.client.take() {
            shutdown_with_timeout(client.shutdown(), MCP_SHUTDOWN_TIMEOUT).await?;
        }
        let (client, stderr) =
            connect_transport(self.transport.clone(), self.client_config.clone()).await?;
        self.client = Some(client);
        self.stderr = stderr;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), McpError> {
        if let Some(client) = self.client.take() {
            shutdown_with_timeout(client.shutdown(), MCP_SHUTDOWN_TIMEOUT).await?;
        }
        Ok(())
    }
}

async fn shutdown_with_timeout<F>(shutdown: F, timeout: Duration) -> Result<(), McpError>
where
    F: Future<Output = Result<(), McpError>>,
{
    tokio::time::timeout(timeout, shutdown)
        .await
        .map_err(|_| McpError::Timeout)?
}

async fn connect_transport(
    transport: McpTransportConfig,
    client_config: McpClientConfig,
) -> Result<(McpClient, Option<McpStderrCapture>), McpError> {
    match transport {
        McpTransportConfig::Stdio(config) => {
            let spawned = stdio::spawn(config)?;
            let stderr = spawned.stderr.clone();
            let client = McpClient::connect(spawned.into_transport(), client_config).await?;
            Ok((client, Some(stderr)))
        }
        McpTransportConfig::Http(config) => {
            let client = super::http::McpHttpConfig::connect(config, client_config).await?;
            Ok((client, None))
        }
        McpTransportConfig::LegacySse(config) => {
            let client = config.connect(client_config).await?;
            Ok((client, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::shutdown_with_timeout;
    use crate::mcp::McpError;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn shutdown_timeout_does_not_wait_for_an_unresponsive_server() {
        let started = Instant::now();
        let result = shutdown_with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<(), McpError>(())
            },
            Duration::from_millis(10),
        )
        .await;

        assert!(matches!(result, Err(McpError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
