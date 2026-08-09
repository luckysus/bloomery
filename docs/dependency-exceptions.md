# Bloomery 依赖治理与例外登记

> 对应发布质量计划 `docs/superpowers/plans/2026-07-29-bloomery-release-quality.md` 的 **Task 4 · Step 1（基线扫描）** 与 **Step 2（解决依赖并登记例外）**。
> 本文档记录 Bloomery 桌面端（`src-tauri` Rust crate 与 `frontend` npm 包）经 `cargo deny` / `cargo audit` / `npm audit` 扫描后的处置结论与所有已接受例外。
>
> - 责任人（owner）：**luckysus**
> - 下次复审日期（review date）：**2026-11-08**
> - 扫描工具：cargo-deny 0.20.2、cargo-audit、npm audit
> - 门禁配置：`src-tauri/deny.toml`
> - 扫描原始输出：`artifacts/verification/cargo-deny.txt`、`artifacts/verification/cargo-audit.txt`、`artifacts/verification/npm-audit.txt`(`.json`)

## 1. 扫描结论总览

| 扫描项 | 命令 | 结果 | 结论 |
| --- | --- | --- | --- |
| cargo-deny（全量） | `cargo deny check` | exit 0：`advisories ok, bans ok, licenses ok, sources ok` | 通过 |
| cargo-audit | `cargo audit` | exit 0；vulnerabilities = 0；19 条 allowed warnings（17 unmaintained + 2 informational unsound） | 通过 |
| npm audit（frontend） | `npm audit` | high = 0，critical = 0，moderate = 0，low = 0 | 通过；`@babel/core` 已通过 `npm audit fix --package-lock-only` 更新锁定版本 |

## 2. 漏洞 remediation（已从根源清零）

升级依赖后，两处 quick-xml 拒绝服务（DoS）漏洞已被彻底移除，`cargo deny` 的 advisories 检查中不再登记任何漏洞类 ignore。

### 2.1 直接依赖 quick-xml：0.39 → 0.41

- `src-tauri/Cargo.toml`：`quick-xml = "0.39"` → `quick-xml = "0.41"`。
- `src-tauri/Cargo.lock`：`quick-xml` 锁定为 **0.41.0**。

### 2.2 传递依赖 quick-xml 0.39.4：经 plist 升级移除

- 执行 `cargo update -p plist`：`plist 1.9.0 → 1.10.0`。
- plist 1.10.0 依赖的 quick-xml 版本已进入 0.41 系列，因此 Cargo.lock 中原先由 plist 引入的 **quick-xml 0.39.4 被彻底移除**。
- 核实结果：`Cargo.lock` 中 `quick-xml` 仅剩 **0.41.0** 一个版本（`Select-String -Path src-tauri\Cargo.lock -Pattern "quick-xml"` 仅命中 0.41.0 的包定义及两处依赖引用，无 0.39.4）。

### 2.3 被清除的 advisory ID

| Advisory | crate | 类别 | 说明 | 修复版本 |
| --- | --- | --- | --- | --- |
| RUSTSEC-2026-0194 | quick-xml | denial-of-service | 检查起始标签重复属性名时的二次方时间复杂度，可被构造输入触发 CPU 耗尽 | `>= 0.41.0` |
| RUSTSEC-2026-0195 | quick-xml | denial-of-service | `NsReader` 命名空间声明无界分配，可被构造输入触发内存耗尽 | `>= 0.41.0` |

两条均为可用性影响（CVSS `A:H`）的 DoS 漏洞，升级至 0.41.0 后从根源清零，无需在 `deny.toml` 中登记任何漏洞类例外。

## 3. unmaintained 例外表（17 条）

`deny.toml` 的 `[advisories]` 使用 schema v2：`unmaintained = "all"`（扫描全部传递依赖）。schema v2 中 unmaintained 命中即按错误处理、无 `warn` 档位，因此将以下 17 条经评估可接受的 unmaintained 公告登记到 `ignore`，`cargo deny` 会以“已忽略”提示列出并保持 exit 0。

所有条目共同属性：**owner = luckysus**，**review date = 2026-11-08**，**reason = 无安全升级路径（上游已归档/停止维护），且均为间接传递依赖，非项目直接选型，风险为“停止维护”而非已知漏洞**。

### 3.1 gtk-rs GTK3 绑定（经 tauri → wry/tao/muda 引入，Linux GUI 栈）

GTK3 绑定整体已归档、不再维护。Bloomery 为 Windows-first 桌面端，Linux GUI 栈仅在跨平台构建时参与，Windows 目标不加载这些库。

| Advisory | crate | 版本 |
| --- | --- | --- |
| RUSTSEC-2024-0411 | gdkwayland-sys | 0.18.2 |
| RUSTSEC-2024-0412 | gdk | 0.18.2 |
| RUSTSEC-2024-0413 | atk | 0.18.2 |
| RUSTSEC-2024-0414 | gdkx11-sys | 0.18.2 |
| RUSTSEC-2024-0415 | gtk | 0.18.2 |
| RUSTSEC-2024-0416 | atk-sys | 0.18.2 |
| RUSTSEC-2024-0417 | gdkx11 | 0.18.2 |
| RUSTSEC-2024-0418 | gdk-sys | 0.18.2 |
| RUSTSEC-2024-0419 | gtk3-macros | 0.18.2 |
| RUSTSEC-2024-0420 | gtk-sys | 0.18.2 |

### 3.2 proc-macro-error（经 glib-macros → GTK3 栈引入）

| Advisory | crate | 版本 | 引入路径 |
| --- | --- | --- | --- |
| RUSTSEC-2024-0370 | proc-macro-error | 1.0.4 | glib-macros → gtk-rs GTK3 栈；维护者失联 |

### 3.3 bincode（经 hnsw_rs 引入，向量检索）

| Advisory | crate | 版本 | 引入路径 |
| --- | --- | --- | --- |
| RUSTSEC-2025-0141 | bincode | 1.3.3 | hnsw_rs 0.3.4；bincode 1.x 团队宣布永久停止维护 |

### 3.4 rust-unic 系列（经 tauri-utils → urlpattern → unic-ucd-ident 引入）

rust-unic 整体不再维护，作为 URL pattern 解析的间接依赖被引入。

| Advisory | crate | 版本 |
| --- | --- | --- |
| RUSTSEC-2025-0075 | unic-char-range | 0.9.0 |
| RUSTSEC-2025-0080 | unic-common | 0.9.0 |
| RUSTSEC-2025-0081 | unic-char-property | 0.9.0 |
| RUSTSEC-2025-0098 | unic-ucd-version | 0.9.0 |
| RUSTSEC-2025-0100 | unic-ucd-ident | 0.9.0 |

### 3.5 关于 cargo-audit 的额外告警

`cargo audit` 除上述 17 条 unmaintained 外，另报告 2 条 informational 级 **unsound** 公告：RUSTSEC-2024-0429（`glib` 的 `VariantStrIter` 迭代器实现）和 RUSTSEC-2026-0221（`event-listener` 的 `StackSlot` 跨线程标记）。共计 19 条 allowed warnings。这些为“提示（informational）”而非已知可利用漏洞，cargo audit 的 `vulnerabilities = 0`、exit 0；`cargo deny` 的 advisories 检查同样判定为 ok。无需登记为漏洞例外，但保留在审计记录中，后续上游有兼容升级时复审。

## 4. 重复版本例外（bans）

`deny.toml` 的 `[bans]` 设 `multiple-versions = "warn"`：重复版本以告警呈现、不阻断构建。仅对**设计性双版本**在 `skip` 中显式登记。

### 4.1 显式设计的双版本：reqwest

| skip 条目 | 版本 | 用途 |
| --- | --- | --- |
| `reqwest@0.12.28` | 0.12 | 主 HTTP 客户端，启用 `rustls-tls` 栈（`default-features = false`） |
| `reqwest@0.13.4` | 0.13 | 经 `rmcp` MCP 传输依赖引入（包别名 `reqwest-013`） |

两者为项目显式声明的双版本（见 `Cargo.toml` 中 `reqwest` 与 `reqwest-013`），属有意设计，故 skip。

### 4.2 其余重复版本：生态传递依赖，不可控

其余重复版本（如 `base64` 0.21/0.22/0.23、`winnow` 0.7/1.0、`windows-sys`、`hashbrown`、`syn` 等）均为 tauri / zbus / gtk 等上游生态各自锁定不同版本所致，非本项目可控，且不构成安全或许可证风险，因此保持 `multiple-versions = "warn"`（仅告警），不逐条 skip。

## 5. 许可证说明（licenses）

`deny.toml` 的 `[licenses]` 使用 schema v2，allow-list 依据实际 `cargo deny` 扫描结果编制。全部依赖命中 allow-list，`licenses ok`。以下就需要特别说明的许可证给出接受理由。

| 许可证 | 代表 crate | 接受理由 |
| --- | --- | --- |
| MPL-2.0 | cssparser、selectors | 文件级 copyleft：仅要求对被修改的 MPL 源文件回馈，不传染到整体作品。Bloomery 不修改这些文件，仅链接使用，与 Apache-2.0 发布产物兼容。 |
| CDLA-Permissive-2.0 | webpki-roots | 宽松型社区数据许可证（根证书数据），需显式 allow，否则被判为 unknown。无 copyleft 传染。 |
| Unicode-3.0 | icu_*（icu_normalizer、icu_properties 等） | Unicode 数据文件许可证，宽松，允许自由再分发。 |
| Apache-2.0 WITH LLVM-exception | target-lexicon | 在 Apache-2.0 基础上附加 LLVM 例外，进一步放宽静态链接限制，兼容。 |
| BSL-1.0 | ryu | Boost 软件许可证，宽松，兼容。 |
| 含 OR 表达式的多重许可 | r-efi（`MIT OR Apache-2.0 OR LGPL-2.1-or-later`）、fiat-crypto、rustix、dunce、aho-corasick、adler2 等 | SPDX OR 表达式中存在被 allow 的选项（MIT / Apache-2.0 / CC0-1.0 / Unlicense / 0BSD 等），cargo-deny 自动命中允许项并排除受限选项（如 LGPL），无需登记 license exception。 |

因此 `deny.toml` 的 `[licenses].exceptions` 保持为空（`exceptions = []`）。

## 6. 领域资产与 vendored 代码台账

- **领域包许可证**：`domain-packs/steel/manifest.json` 的 `license` 字段为 **Apache-2.0**，与项目主许可证一致。
- **vendored / 复制的第三方源码**：全仓快速排查（`src-tauri/src`、`frontend/src`、`domain-packs` 等，排除 `node_modules` / `target` / `dist` / `.git`）**未发现** vendored、复制或整文件搬运的第三方源码目录（无 `vendor`/`third-party`/`external` 等目录）。**结论：无 vendored 代码。** 所有第三方代码均通过 Cargo / npm 声明式依赖引入，其许可证由 §5 的 cargo-deny 许可证策略统一治理。

## 7. Task 4 剩余项与生成入口

以下为 Task 4 Step 3/4 及相关收尾项，**不在本任务范围**，供后续任务承接：

- **Step 3 — 原创代码许可证迁移**：Bloomery 原创代码 MIT → Apache-2.0（需先核实唯一著作权归属，保留第三方 MIT/Apache 通知）。（注：`Cargo.toml` 与 `domain-packs/steel/manifest.json` 已标注 Apache-2.0，`LICENSE`/`NOTICE` 有改动，完整迁移与核验由该步骤统一处理。）
- **Step 4 — SBOM 与通知生成**：`scripts/generate-sbom.ps1` 使用 `cargo-cyclonedx` 与 npm 11 的 `npm sbom` 从锁定清单生成 Rust CycloneDX、前端 CycloneDX、前端 SPDX，以及 `THIRD_PARTY_NOTICES.txt`。
- **发布产物包含校验**：`scripts/build-release.ps1` 在复制 Windows 安装包后调用生成入口，因此 release manifest 会记录这些文件；Task 7 仍需在真实安装包和干净机器上做最终内容验证。
- **CI 门禁集成**：将 `cargo deny check`、`cargo audit`、`npm audit` 接入阻断式 CI（对应计划 Task 9）。

本地复核命令：

```powershell
powershell -File scripts/generate-sbom.ps1 -Offline -OutputDirectory artifacts/verification/sbom
```
