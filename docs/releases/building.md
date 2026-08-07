# Building Bloomery Releases / 构建 Bloomery 发布包

This document describes the reproducible local and CI release entry points. The
repository remains an engineering build until every release gate in the
roadmap has current evidence.

本文档说明本地和 CI 使用的可复现发布入口。路线图中的所有正式发布门禁
获得当前证据之前，仓库仍然属于开发构建，不代表稳定正式版。

## Local verification / 本地验证

Run these commands from the repository root in PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-check.ps1 -WithE2E
```

When Cargo dependencies are already cached, `-Offline` avoids the machine's
network configuration while preserving the same checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Offline
```

`-Offline` only affects Cargo. Tauri may still need to download its platform
bundler, such as NSIS, on the first package build.

`-Offline` 只影响 Cargo。第一次打包时，Tauri 仍可能需要下载 NSIS 等平台
打包工具；这一步需要可用网络，但不要求修改系统代理配置。

## Windows candidate artifacts / Windows 候选产物

Build an unsigned NSIS and MSI candidate with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-release.ps1
```

For a single installer type, use `-Bundles nsis` or `-Bundles msi`. The script
runs the deterministic test suite first, executes the frontend build through
the Tauri hook, builds the Rust host, copies installers into a versioned
directory, and writes:

- `release-manifest.json`, containing the version, commit, artifact sizes and
  SHA-256 values;
- `SHA256SUMS.txt`, containing standard checksum lines.

The current local script passes `--no-sign` deliberately. These artifacts are
for engineering validation only and must not be presented as the signed public
release. Authenticode and Tauri updater signing keys belong in a protected
release environment and are never stored in this repository.

当前脚本明确使用 `--no-sign`，产物只用于工程验收，不能冒充签名正式版。
Authenticode 和 Tauri 更新签名私钥必须保存在受保护的发布环境中，不能进入
本仓库。

## CI / 持续集成

`.github/workflows/quality.yml` runs the deterministic suite on Windows for pull
requests and pushes to `main`. `.github/workflows/release.yml` runs the E2E and
unsigned candidate package checks on manual dispatch or a `v*` tag, then uploads
the candidate files as a workflow artifact. It does not create a public release
or bypass signing and release audit requirements.

`quality.yml` 会在 Windows 上对 Pull Request 和 `main` 推送执行确定性测试。
`release.yml` 会在手动触发或 `v*` 标签上执行 E2E 和未签名候选打包，并上传
候选文件；它不会自动创建公开发行版，也不会绕过签名和发布审计要求。

## Troubleshooting / 故障排查

- If the frontend hook reports a missing root `package.json`, verify that
  `src-tauri/tauri.conf.json` uses `npm --prefix frontend run build`.
- If NSIS or WiX cannot be downloaded, restore network access for the current
  process and rerun the command. Do not commit downloaded tools or place them
  in the source tree.
- If the output directory already exists, choose a new `-OutputDirectory`; the
  script refuses to overwrite release artifacts by default.
- A successful source test or unsigned installer does not prove the Windows
  upgrade, uninstall, signing, updater, security, SBOM or steel case-study gates.

- 如果前端钩子提示根目录缺少 `package.json`，检查
  `src-tauri/tauri.conf.json` 是否使用 `npm --prefix frontend run build`。
- 如果 NSIS 或 WiX 无法下载，为当前进程恢复网络后重新执行，不要把下载的
  工具提交到源码目录。
- 如果输出目录已经存在，请指定新的 `-OutputDirectory`；脚本默认拒绝覆盖
  发布产物。
- 源码测试或未签名安装包通过，并不代表 Windows 升级、卸载、签名、更新器、
  安全、SBOM 或钢铁案例研究门禁已经通过。
