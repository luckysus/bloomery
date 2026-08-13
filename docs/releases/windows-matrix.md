# Windows 发布验收矩阵

这份矩阵用于记录 Bloomery 在 Windows 10/11 上的真实安装生命周期证据。它不把源码测试或单次 smoke 当作完整兼容性证明。

## 覆盖范围

| 场景 | 脚本 | 当前状态 |
| --- | --- | --- |
| 全新安装、启动、卸载、数据保留 | `scripts/lifecycle-matrix.ps1 -RunInstallerSmoke` | 可执行，需真实安装包 |
| Unicode 安装路径 | `scripts/lifecycle-matrix.ps1` | 脚本使用临时 Unicode 安装路径 |
| 非默认数据目录 | `scripts/lifecycle-matrix.ps1` | 通过 `BLOOMERY_DATA_DIR` 注入临时目录 |
| 旧版 → 新版升级 | `scripts/lifecycle-matrix.ps1 -RunUpgradeDowngrade` | 可执行，需旧版和新版安装包 |
| 新版 → 旧版降级保护 | `scripts/lifecycle-matrix.ps1 -RunUpgradeDowngrade` | 可执行，需旧版和新版安装包 |
| Windows 10 | 本机或受控 Windows 10 runner | 当前有基础 smoke，完整矩阵待真实产物 |
| Windows 11 | 真实 Windows 11 或受控 runner | 尚无当前证据 |

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

正式签名包不使用 `-AllowUnsigned`。脚本会记录安装包 SHA-256、签名状态、各阶段数据库状态和数据保留哨兵；本次运行创建的 GUID 临时目录在结束时清理，用户数据库和仓库中的 `src-tauri/target/debug` 不会被清理。

## 关闭条件

每个操作系统版本至少需要保存：

1. 旧版安装并启动成功；
2. 升级到新版后数据库、凭据引用和领域包仍可用；
3. 新版安装后尝试降级时得到预期的数据库版本保护，而不是静默破坏数据；
4. 卸载后按照产品策略保留数据；
5. Unicode 用户路径和非默认数据路径均成功；
6. 原始 JSON 报告、安装包 SHA-256、版本号、提交 SHA 和 Windows 版本一起归档。

在 Windows 10 和 Windows 11 都具备上述证据前，Gate H 的 Windows 矩阵保持未完成。
