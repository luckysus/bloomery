# MCP Transports

Bloomery is a local-first MCP client. MCP servers are configured in the desktop
Extensions view and are connected directly by the Rust host. The Web application
and any private Bloomery service are not required.

## Supported transports

| Transport | Configuration | Typical server |
| --- | --- | --- |
| `stdio` | executable, arguments, working directory | A local MCP process |
| `streamable_http` | HTTP or HTTPS MCP endpoint | A current HTTP MCP server |
| `sse` | HTTP or HTTPS legacy SSE endpoint | An older MCP SSE server |

The legacy `sse` transport follows the endpoint announced by the initial SSE
connection and sends JSON-RPC messages to that endpoint. It accepts relative
or absolute same-origin endpoints only. A cross-origin endpoint, a URL with
embedded credentials, and protocol-header overrides are rejected.

## Credentials and environment

Bearer tokens and configured environment values are stored in Windows Credential
Manager. SQLite stores only the server configuration and the names of required
environment values. Secret values are never returned to the React frontend,
written to logs, or included in exports.

Use raw bearer tokens in the configuration field. Bloomery adds the `Bearer`
scheme to the request header. For `stdio`, the child process starts with a
cleared environment. Only configured secret values and the fixed runtime
allowlist `SystemRoot`, `windir`, `ComSpec`, `COMSPEC`, `PATHEXT`, `TEMP`, and
`TMP` may be passed to it; arbitrary inherited names such as `PATH`,
`OPENAI_API_KEY`, and `GH_TOKEN` are rejected.

## Reliability boundaries

All HTTP and SSE events have a bounded size. The SSE client parses UTF-8 event
frames, records event IDs, reconnects with `Last-Event-ID`, honors bounded
server retry intervals, and stops after the configured retry budget. Requests
use the server timeout configured in the desktop UI. Restart is an explicit
user action after a permanent failure.

Server checks return a small doctor result in addition to the raw error:
`missing_credential`, `timeout`, `invalid_transport`,
`process_start_failed`, or `connection_failed`. The UI displays the diagnosis
and suggested next action without exposing token or environment values.

Every discovered MCP tool is converted to Bloomery's typed tool registry.
Tools require local permission confirmation by default. A tool that declares
`readOnlyHint: true` is classified as low-risk automatic execution; a tool that
declares `destructiveHint: true` is classified as dangerous; unknown or
write-capable tools remain confirmation-required. An MCP server cannot grant
itself file, shell, network, secret, or domain permissions.

The chat runtime does not keep every enabled MCP schema in the model prompt.
When more than eight MCP tools are available, Bloomery selects the tools most
related to the current user message by tool id, name, and description, then
passes only that compact set to the agent loop. The Extensions view can still
show the full server tool catalog.

## MCP 传输

Bloomery 是本地优先的 MCP 客户端。用户在桌面端“扩展”页面配置 MCP
服务器，Rust 主机直接连接服务器，不依赖 Web 应用或私有云服务。

支持 `stdio` 本地进程、`streamable_http` 新版 HTTP MCP，以及 `sse` 旧版
Legacy SSE。Legacy SSE 只允许同源、无内嵌凭据的 endpoint，并会拒绝跨域
endpoint 和覆盖协议头的自定义头。

Bearer 令牌和环境变量值保存在 Windows Credential Manager。SQLite 只保存
配置和环境变量名称，前端、日志和导出文件都不会获得密钥值。`stdio`
子进程启动前会清空环境，只允许固定的运行时白名单
`SystemRoot`、`windir`、`ComSpec`、`COMSPEC`、`PATHEXT`、`TEMP` 和
`TMP`；`PATH`、`OPENAI_API_KEY`、`GH_TOKEN` 等任意继承名称会被拒绝。
服务器检查除了原始错误，还会返回轻量 doctor 诊断，例如缺少凭据、超时、
传输配置无效、本地进程启动失败或连接失败；前端只显示原因和建议动作，不显示
令牌或环境变量值。
发现的 MCP 工具统一进入 Bloomery 工具注册表。默认需要本地权限确认；
声明 `readOnlyHint: true` 的工具会归类为低风险自动执行，声明
`destructiveHint: true` 的工具会归类为危险操作，未知或可能写入的工具仍然需要确认。
MCP 服务器不能自行获得文件、Shell、网络、密钥或领域权限。

对话运行时不会把所有已启用 MCP 工具 schema 常驻塞进模型 prompt。可用 MCP
工具超过 8 个时，Bloomery 会按当前用户问题匹配工具 id、名称和描述，只把最相关的
一小组交给 agent loop；“扩展”页面仍可查看完整工具列表。
