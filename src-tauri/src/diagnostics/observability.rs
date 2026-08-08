//! 进程级诊断脱敏设施。
//!
//! 集中登记本进程已知的密钥值，并保证离开进程的诊断输出（panic 消息、
//! 日志行、诊断 JSON）在输出前都经过 [`Redactor`] 脱敏。这是纵深防御：
//! 即使某条错误消息或 panic 意外拼接了凭据明文，登记过的密钥也会被替换成
//! `[REDACTED]`，而不会进入终端、日志或崩溃报告。
//!
//! 密钥本身仍只存放在 Windows 凭据管理器中；这里保存的明文副本仅驻留内存，
//! 从不写入 SQLite、备份或任何文件。

use crate::diagnostics::redaction::Redactor;
use crate::storage::secrets::SecretValue;
use std::panic::PanicHookInfo;
use std::sync::{Mutex, Once, OnceLock};

static SECRETS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static PANIC_HOOK: Once = Once::new();

fn registry() -> &'static Mutex<Vec<String>> {
    SECRETS.get_or_init(|| Mutex::new(Vec::new()))
}

/// 登记一个密钥，使后续诊断输出（panic、日志、诊断 JSON）都会将其脱敏。
///
/// 幂等：重复登记同一密钥不会累积。
pub fn register_secret(secret: &SecretValue) {
    let value = secret.expose().to_string();
    if value.is_empty() {
        return;
    }
    let mut secrets = registry().lock().unwrap_or_else(|error| error.into_inner());
    if !secrets.iter().any(|known| known == &value) {
        secrets.push(value);
    }
}

/// 使用当前登记的全部密钥构造一个 [`Redactor`]。
pub fn global_redactor() -> Redactor {
    let secrets = registry().lock().unwrap_or_else(|error| error.into_inner());
    secrets.iter().fold(Redactor::new(), |redactor, secret| {
        match SecretValue::new(secret.clone()) {
            Ok(value) => redactor.with_secret(&value),
            Err(_) => redactor,
        }
    })
}

/// 对任意诊断文本做脱敏（日志行、错误消息等）。
pub fn redact_line(value: &str) -> String {
    global_redactor().redact_text(value)
}

/// 对诊断 JSON 做递归脱敏（health/error JSON、备份导出的元数据等）。
pub fn redact_json(value: &serde_json::Value) -> serde_json::Value {
    global_redactor().redact_json(value)
}

/// 将 panic 信息格式化为一行并脱敏，供 panic hook 输出。
pub fn format_panic(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|value| (*value).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_string());
    let raw = match info.location() {
        Some(location) => format!(
            "panic at {}:{}:{}: {message}",
            location.file(),
            location.line(),
            location.column()
        ),
        None => format!("panic: {message}"),
    };
    redact_line(&raw)
}

/// 是否在 panic 时额外打印栈帧回溯。
///
/// 仅在 debug 构建（`cfg!(debug_assertions)`）或显式设置了
/// `BLOOMERY_PANIC_BACKTRACE` 环境变量时启用；release 且未设该变量时返回
/// `false`，保持仅输出一行脱敏 panic 消息的默认行为。
fn backtrace_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("BLOOMERY_PANIC_BACKTRACE").is_some()
}

/// 构造 panic hook 需要写入 stderr 的诊断行序列。
///
/// 第一行始终是经 [`format_panic`] 脱敏的 panic 消息——绝不把原始 panic
/// payload 交给标准库默认 hook 打印，避免绕过 [`Redactor`] 导致凭据明文泄漏。
/// 当 [`backtrace_enabled`] 为真时，追加一段栈帧回溯（不含原始 panic 消息
/// 文本），并同样经 [`redact_line`] 脱敏作为纵深防御。
pub fn format_panic_diagnostics(info: &PanicHookInfo<'_>) -> Vec<String> {
    let mut lines = vec![format_panic(info)];
    if backtrace_enabled() {
        let backtrace = std::backtrace::Backtrace::force_capture();
        lines.push(redact_line(&format!("panic backtrace:\n{backtrace}")));
    }
    lines
}

/// 安装脱敏 panic hook（进程级，幂等）。
///
/// 安装后，任何 panic 的输出都会先经过 [`format_panic`] 脱敏，再写入 stderr，
/// 避免凭据明文出现在崩溃报告中。debug 构建或设置 `BLOOMERY_PANIC_BACKTRACE`
/// 时，会额外打印脱敏后的栈帧回溯以恢复诊断能力。
pub fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            for line in format_panic_diagnostics(info) {
                eprintln!("{line}");
            }
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_secret_is_redacted_from_lines_and_json() {
        // 注意：这里不用 sk-/rk- 前缀的合成密钥，避免被离线安全门禁
        // （security-check.ps1 会扫描 src 目录）误报为真实凭据。
        let canary = "canary-diagnostic-secret-value-000";
        let secret = SecretValue::new(canary).unwrap();
        register_secret(&secret);
        register_secret(&secret); // 幂等

        let line = redact_line(&format!("provider failed using {canary}"));
        assert!(!line.contains(canary));
        assert!(line.contains("[REDACTED]"));

        let json = redact_json(&serde_json::json!({
            "message": format!("handshake {canary}"),
        }));
        assert!(!json.to_string().contains(canary));
    }
}
