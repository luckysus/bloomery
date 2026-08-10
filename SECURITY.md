# Security Policy / 安全策略

**Bloomery is a local-first desktop application. Your data never leaves your machine unless you configure an external provider.**
**Bloomery 是本地优先的桌面应用。除非你主动配置外部 Provider，否则数据不会离开你的电脑。**

## Reporting vulnerabilities / 报告漏洞

- Report security issues privately through the repository's private vulnerability reporting channel (GitHub Security Advisories) or an issue titled `SECURITY:` if no private channel is available.
- 请通过仓库的私有漏洞报告通道（GitHub Security Advisories）提交安全问题；如不可用，再使用标题含 `SECURITY:` 的 issue。
- Include: affected version/commit, reproduction steps, and impact. 请附：受影响版本/提交、复现步骤与影响。

## Security model / 安全模型

See `docs/security-model.md` for the full threat model. Key guarantees enforced by tests:
完整威胁模型见 `docs/security-model.md`。测试强制的关键保证：

- **Secrets / 密钥**: API keys are stored only in Windows Credential Manager; Tauri commands never return secret values to the frontend (`secret_get` is not registered).
  API 密钥仅存于 Windows 凭据管理器；任何 Tauri 命令都不会把密钥值返回前端（未注册 `secret_get`）。
- **No cloud dependency / 无云依赖**: there is no login, no hosted backend, and no telemetry; architecture tests reject `/api/`, cloud task, and auth imports.
  无登录、无托管后端、无遥测；架构测试拒绝 `/api/`、云任务与认证导入。
- **Permissions / 权限**: tools are classified Automatic / ConfirmationRequired / Dangerous; non-automatic tools always request an explicit decision, and domain packages only expose allowlisted tools.
  工具分为自动/需确认/危险三级；非自动工具必须显式裁决，领域包仅暴露白名单工具。
- **Backups / 备份**: backup export excludes secrets; restore requires an explicit preview confirmation.
  备份导出排除密钥；恢复需要显式预览确认。
- **Domain packages / 领域包**: official packages are signature-verified with an embedded trust key; third-party packages install only as explicitly unsigned and are clearly marked.
  官方包使用内嵌信任键验签；第三方包只能以显式未签名方式安装并明确标记。
- **Compute worker / 计算 Worker**: the Python worker receives no credentials, listens on no port, and runs validated payloads only; packaged artifacts ship with SHA-256, SBOM, and an explicit unsigned marker until release signing.
  Python Worker 不接收凭据、不监听端口、只运行校验后的载荷；打包产物附 SHA-256、SBOM，并在发布签名前带显式未签名标记。

## Supported versions / 支持版本

Only the latest commit on `main` and the first signed stable release (when published) receive security support.
仅 `main` 最新提交与首个签名稳定版（发布后）获得安全支持。

## Out of scope / 不在范围

- Attacks requiring the user to install a tampered binary after ignoring signature warnings.
  用户忽略签名警告后安装被篡改二进制所需的攻击。
- Vulnerabilities of user-configured external providers (their services, keys, or networks).
  用户自行配置的外部 Provider 的漏洞（其服务、密钥或网络）。
