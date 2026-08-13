# Windows 发布验收矩阵

这份矩阵用于记录 Bloomery 当前 Windows 10 发布目标的真实安装生命周期证据。它不把源码测试或单次 smoke 当作完整兼容性证明。Windows 11 暂不属于当前发布阻断范围，待具备真实 Windows 11 环境后再执行同一套矩阵。

## 覆盖范围

| 场景 | 脚本 | 当前状态 |
| --- | --- | --- |
| 全新安装、启动、卸载、数据保留 | `scripts/lifecycle-matrix.ps1 -RunInstallerSmoke` | 当前 `1.0.0` 未签名工程包已通过 |
| Unicode 安装路径 | `scripts/lifecycle-matrix.ps1` | 已通过；目录名由 Unicode code point 构造，兼容 Windows PowerShell 5.1 |
| 非默认数据目录 | `scripts/lifecycle-matrix.ps1` | 已通过 `BLOOMERY_DATA_DIR` 注入临时目录 |
| 旧版 → 新版升级 | `scripts/lifecycle-matrix.ps1 -RunUpgradeDowngrade` | `0.1.0` → `1.0.0` 已通过 |
| 新版 → 旧版降级保护 | `scripts/lifecycle-matrix.ps1 -RunUpgradeDowngrade` | `1.0.0` → `0.1.0` 数据保护已通过 |
| Windows 10 | 本机 Windows 10 | 当前工程包完整矩阵已通过；正式签名包仍需重复验证 |
| Windows 11 | 真实 Windows 11 或受控 runner | 后续兼容性任务，当前不阻断 Windows 10 发布 |

## 执行方式

脚本要求显式提供两个安装包，防止用同一版本伪造升级/降级结果。安装、数据和临时运行目录固定在仓库 F 盘的 `artifacts/lifecycle-runs/` 下，该目录已被 Git 忽略：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\lifecycle-matrix.ps1 `
  -OldInstallerPath F:\release\Bloomery_old_setup.exe `
  -NewInstallerPath F:\release\Bloomery_new_setup.exe `
  -RunInstallerSmoke `
  -RunUpgradeDowngrade `
  -AllowUnsigned `
  -ReportPath F:\release\windows-lifecycle-matrix.json
```

发布检查也支持将跨版本矩阵作为显式门禁执行。它会先构建当前版本安装包，再把该安装包作为新版，与 `-OldInstallerPath` 指定的旧版执行旧版 → 新版 → 旧版流程：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\release-check.ps1 `
  -Package `
  -UpgradeDowngrade `
  -OldInstallerPath F:\release\Bloomery_old_setup.exe `
  -AllowDirty `
  -Offline
```

`-UpgradeDowngrade` 必须与 `-Package` 和 `-OldInstallerPath` 同时使用，避免矩阵误用不存在的当前包或隐式选择旧包。正式签名验证时不要传入 `-AllowUnsigned`，并同时使用 `-Signed -RequireSigned`。

矩阵还会读取两个安装包的产品版本，并拒绝同版本产物，即使它们的文件哈希不同也不能作为旧版和新版。当前仓库应用版本为 `1.0.0`，历史 `0.1.0` 安装包作为旧版；当前工程包的真实升级/降级运行已完成，原始报告为 `artifacts/windows10-lifecycle-1.0.0.json`。

正式签名包不使用 `-AllowUnsigned`。脚本会记录安装包 SHA-256、签名状态、各阶段数据库状态和数据保留哨兵；本次运行创建的 GUID 临时目录在结束时清理，用户数据库和仓库中的 `src-tauri/target/debug` 不会被清理。

## 关闭条件

每个操作系统版本至少需要保存：

1. 旧版安装并启动成功；
2. 升级到新版后数据库、凭据引用和领域包仍可用；
3. 新版安装后尝试降级时得到预期的数据库版本保护，而不是静默破坏数据；
4. 卸载后按照产品策略保留数据；
5. Unicode 用户路径和非默认数据路径均成功；
6. 原始 JSON 报告、安装包 SHA-256、版本号、提交 SHA 和 Windows 版本一起归档。

当前 Windows 10 发布目标已具备未签名工程包的上述证据；正式签名包生成后必须重复矩阵并归档新的报告。Windows 11 具备测试环境后，再作为后续兼容性版本重复执行上述矩阵。
