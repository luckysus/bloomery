//! 进程级统一日志。
//!
//! 这是 Suna 桌面进程唯一的日志出口。所有经 `log` 宏记录的日志都会先经过
//! [`crate::diagnostics::observability::redact_line`] 脱敏，再写入 stderr；
//! 因此即使某条日志意外拼接了凭据明文，也会被替换成 `[REDACTED]`，而不会
//! 进入终端或日志文件。这是与 panic hook 相同的纵深防御。
//!
//! 默认级别为 `info`，可用 `RUST_LOG` 覆盖（例如 `RUST_LOG=suna=debug`）。
//! 初始化是幂等的：重复调用只会生效一次；已有 logger 时静默跳过。
//!
//! 注意：只有桌面进程（[`crate::app::run`]）会安装 logger。`src/bin/` 下的
//! 独立命令行工具不安装 logger，因此它们继续使用 `println!` / `eprintln!`
//! 直接输出，不要改用 `log` 宏。

use crate::diagnostics::observability::redact_line;
use std::io::Write;
use std::sync::Once;

static INSTALL: Once = Once::new();

/// 安装进程级日志（幂等）。
pub fn install() {
    INSTALL.call_once(|| {
        let mut builder =
            env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"));
        // 每条日志在写出前先脱敏，保证凭据不会进入终端或日志文件。
        builder.format(|buffer, record| {
            writeln!(
                buffer,
                "[{}] {}",
                record.level(),
                redact_line(&record.args().to_string())
            )
        });
        // try_init 在 logger 已存在时返回 Err；此处忽略即可，保持幂等。
        let _ = builder.try_init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent() {
        // 重复调用不得 panic，也不得替换已安装的 logger。
        install();
        install();
    }

    #[test]
    fn registered_secrets_are_redacted_from_log_lines() {
        use crate::diagnostics::observability::register_secret;
        use crate::storage::secrets::SecretValue;

        // 不使用 sk-/rk- 前缀的合成密钥，避免离线安全门禁误报。
        let canary = "canary-logging-secret-value-000";
        register_secret(&SecretValue::new(canary).unwrap());

        let line = redact_line(&format!("provider failed using {canary}"));
        assert!(!line.contains(canary));
        assert!(line.contains("[REDACTED]"));
    }
}
