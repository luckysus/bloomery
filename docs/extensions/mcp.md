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
scheme to the request header. For `stdio`, only explicitly selected inherited
variables and configured values are passed to the child process.

## Reliability boundaries

All HTTP and SSE events have a bounded size. The SSE client parses UTF-8 event
frames, records event IDs, reconnects with `Last-Event-ID`, honors bounded
server retry intervals, and stops after the configured retry budget. Requests
use the server timeout configured in the desktop UI. Restart is an explicit
user action after a permanent failure.

Every discovered MCP tool is converted to Bloomery's typed tool registry and
requires local permission confirmation by default. An MCP server cannot grant
itself file, shell, network, secret, or domain permissions.

## MCP 传输

Bloomery 是本地优先的 MCP 客户端。用户在桌面端“扩展”页面配置 MCP
服务器，Rust 主机直接连接服务器，不依赖 Web 应用或私有云服务。

支持 `stdio` 本地进程、`streamable_http` 新版 HTTP MCP，以及 `sse` 旧版
Legacy SSE。Legacy SSE 只允许同源、无内嵌凭据的 endpoint，并会拒绝跨域
endpoint 和覆盖协议头的自定义头。

Bearer 令牌和环境变量值保存在 Windows Credential Manager。SQLite 只保存
配置和环境变量名称，前端、日志和导出文件都不会获得密钥值。发现的 MCP
工具统一进入 Bloomery 工具注册表，默认需要本地权限确认。
