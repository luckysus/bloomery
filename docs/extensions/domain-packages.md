# Domain Packages / 领域包

Bloomery domain packages are declaration-only extensions. They add prompts, terminology, retrieval policy, data mappings, evaluation cases, and static assets. They cannot install binaries, run scripts, start processes, or grant permissions.

Bloomery 领域包是声明式扩展，只能提供提示词、术语、检索策略、数据映射、评测用例和静态资源。它们不能安装二进制文件、执行脚本、启动进程或授予权限。

## Package Layout / 目录结构

```text
package/
  manifest.json
  signature.json             # optional for third-party packages
  assets/                    # JSON, Markdown, images, and other static files
  evaluations/               # data referenced by manifest evaluations
```

`manifest.json` is strict JSON. Unknown fields, duplicate assets, absolute paths, parent traversal, symlinks, executable extensions, and oversized resources are rejected before installation. An optional asset `sha256` is checked against the file content during loading.

`manifest.json` 使用严格 JSON。未知字段、重复资源、绝对路径、目录穿越、符号链接、可执行扩展名和超大资源都会在安装前拒绝。资源声明中的 `sha256` 会在加载时与实际文件内容校验。

## Trust Model / 信任模型

- `official_signed`: the package digest is signed with Ed25519 and the key id is present in Bloomery's embedded official trust store.
- `third_party_unsigned`: no signature is present. The package remains isolated and declaration-only, but the UI must show the unsigned warning.
- A signature with an unknown key, a mismatched digest, or invalid signature bytes is rejected. A package must never become official merely because its author or package id looks official.

- `official_signed`：包摘要由 Ed25519 签名，且 key id 位于 Bloomery 内置官方信任源中。
- `third_party_unsigned`：没有签名。包可以在隔离的声明式边界内使用，但界面必须展示未签名警告。
- 未知 key、摘要不匹配或签名格式错误都会拒绝安装。不能因为作者名或包 ID 看起来像官方内容就授予官方信任。

Development builds without a provisioned official public key trust no signed
package as official. A signed release must provide the 64-hex-character
`BLOOMERY_OFFICIAL_PUBLIC_KEY_2026` build variable; the Rust host embeds that
public value at compile time. Generate the key pair offline, publish the key id
and rotation policy, and keep the private key outside the repository and CI logs.

未注入正式公钥的开发构建不会把任何签名包标记为官方包。签名发布必须提供
64 位十六进制的 `BLOOMERY_OFFICIAL_PUBLIC_KEY_2026` 构建变量，Rust 主程序
会在编译时嵌入该公钥。必须离线生成密钥对，公开 key id 和轮换策略，并确保
私钥不进入仓库或 CI 日志。
## Signing / 签名

The signer computes Bloomery's deterministic package digest over every package file except `signature.json`. The signature envelope is:

```json
{
  "key_id": "bloomery-official-2026",
  "algorithm": "ed25519",
  "package_sha256": "<64 lowercase hex characters>",
  "signature": "<128 hex characters>"
}
```

The signature is Ed25519 over the UTF-8 bytes of `package_sha256`. Signing must happen in an offline release environment. The package is staged, validated, hashed, signed, and only then copied into the installed package root.

签名对象是 `package_sha256` 字符串的 UTF-8 字节，不是未经规范化的 ZIP 二进制内容。签名必须在离线发布环境完成，安装端会先暂存、验证、计算摘要、验签，最后才写入安装目录。

## Compatibility And Activation / 兼容性与激活

`compatibility.min_app_version` and `max_app_version` use `major.minor.patch`. Installed versions are retained so users can activate a previous validated version. The active version cannot be removed; preview removal first so the UI can display affected tools, MCP recommendations, and assets.

安装版本会保留，用户可以回滚到已验证版本。当前激活版本不可直接删除，删除前必须先获取影响预览，让界面展示受影响的工具、MCP 建议和资源数量。

The package allowlist is advisory metadata until a tool registration is bound to the domain runtime. It never overrides the global permission engine. Executable capabilities must be built-in or provided by an explicitly configured MCP server.

领域包的内置工具清单在工具注册与运行时绑定完成前只是声明元数据，不能替代全局权限引擎。可执行能力只能来自内置工具或用户明确配置的 MCP 服务。

## Example / 示例

The official source fixture is at `domain-packs/steel/`. It contains bilingual steel terminology, evidence-first retrieval presets, a production-data mapping preset, and small evaluation cases. It is intentionally unsigned in source control; release automation must add `signature.json` with the real release key.

官方源 fixture 位于 `domain-packs/steel/`，包含中英文钢铁术语、证据优先检索预设、生产数据映射预设和小型评测用例。源代码仓库中的 fixture 有意保持未签名，发布自动化必须使用正式发行密钥生成 `signature.json`。
