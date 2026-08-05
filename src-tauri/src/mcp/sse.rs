use std::{sync::Arc, time::Duration};

use rmcp::transport::common::client_side_sse::SseRetryPolicy;

use super::McpError;

const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSseConfig {
    pub max_retries: Option<usize>,
    pub base_delay: Duration,
}

impl McpSseConfig {
    pub fn new(max_retries: Option<usize>, base_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
        }
    }

    pub fn delay_for_attempt(&self, attempt: usize) -> Option<Duration> {
        if self.max_retries.is_some_and(|max| attempt >= max) {
            return None;
        }
        if attempt >= u32::BITS as usize {
            return None;
        }
        self.base_delay
            .checked_mul(1u32 << attempt)
            .filter(|delay| *delay <= MAX_RETRY_DELAY)
    }

    pub(crate) fn validate(&self) -> Result<(), McpError> {
        if self.base_delay.is_zero() || self.base_delay > MAX_RETRY_DELAY {
            return Err(McpError::InvalidConfiguration(
                "MCP SSE base delay must be between 1ms and 60s".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn retry_policy(&self) -> Arc<dyn SseRetryPolicy> {
        Arc::new(BoundedBackoff {
            config: self.clone(),
        })
    }
}

impl Default for McpSseConfig {
    fn default() -> Self {
        Self {
            max_retries: Some(3),
            base_delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug)]
struct BoundedBackoff {
    config: McpSseConfig,
}

impl SseRetryPolicy for BoundedBackoff {
    fn retry(&self, current_times: usize) -> Option<Duration> {
        self.config.delay_for_attempt(current_times)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_does_not_overflow() {
        let config = McpSseConfig::new(Some(128), Duration::from_secs(60));
        assert!(config.delay_for_attempt(127).is_none());
    }
}
