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

Add `-Performance` when the 100k-chunk local retrieval gate should run as part
of the release check. The benchmark is intentionally opt-in because its first
build and corpus setup are expensive:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-check.ps1 -Performance
```

When Cargo dependencies are already cached, `-Offline` avoids the machine's
network configuration while preserving the same checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Offline
```

`-Offline` only affects Cargo. Tauri may still need to download its platform
bundler, such as NSIS, on the first package build.

For a local engineering validation that also installs, launches, uninstalls,
and checks data retention for the newly built NSIS installer, add
`-Package -InstallerSmoke -AllowDirty` to `release-check.ps1`. The smoke gate
uses a temporary directory and does not represent a signed public release.

Run the reproducible steel case study independently when validating the domain
workflow or its report:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/case-study.ps1 -Offline
```

The report is written under `artifacts/case-study/` and is ignored by Git.

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

`release-check.ps1 -Package` builds the candidate before running the packaged
Windows lifecycle verification, so a clean CI runner does not depend on a
previous artifact directory.

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
unsigned candidate package checks on manual dispatch or a `v*` tag. On a
semantic-version tag, its protected signed job runs only after the signing
environment is approved; the publish job then downloads that exact signed
artifact set and creates the GitHub Release. It never publishes an unsigned
candidate as a public release.

`quality.yml` 会在 Windows 上对 Pull Request 和 `main` 推送执行确定性测试。
`release.yml` 会在手动触发或 `v*` 标签上执行 E2E 和未签名候选打包，并上传
候选文件。在语义版本标签上，受保护的签名 job 只有在发布环境审批后才会
运行；随后 publish job 下载同一批已验证的签名产物并创建 GitHub Release。
未签名候选包不会被公开发布。

## Signed updater builds / 签名更新构建

The in-app updater is wired through the official Tauri updater and process
plugins. Local engineering builds stay unsigned by default. Use the protected
release environment and `scripts/build-release.ps1 -Signed` only after the
domain-package public key (`BLOOMERY_OFFICIAL_PUBLIC_KEY_2026`), updater
metadata, and signing key have been provisioned. The domain public key must be
exactly 64 hexadecimal characters and is embedded at compile time; the script
refuses signed builds without it. See `docs/releases/updater.md`; private
signing material must never enter the repository or an artifact.

## Troubleshooting / 故障排查

- If the frontend hook reports a missing root `package.json`, verify that
  `src-tauri/tauri.conf.json` runs `npm run build` with `cwd` set to
  `../frontend`.
- If NSIS or WiX cannot be downloaded, restore network access for the current
  process and rerun the command. Do not commit downloaded tools or place them
  in the source tree.
- If the output directory already exists, choose a new `-OutputDirectory`; the
  script refuses to overwrite release artifacts by default.
- A successful source test or unsigned installer does not prove the Windows
  upgrade, uninstall, signing, updater, security, SBOM or steel case-study gates.

- 如果前端钩子提示根目录缺少 `package.json`，检查
  `src-tauri/tauri.conf.json` 是否使用 `npm run build`，并将 `cwd` 设置为
  `../frontend`。
- 如果 NSIS 或 WiX 无法下载，为当前进程恢复网络后重新执行，不要把下载的
  工具提交到源码目录。
- 如果输出目录已经存在，请指定新的 `-OutputDirectory`；脚本默认拒绝覆盖
  发布产物。
- 源码测试或未签名安装包通过，并不代表 Windows 升级、卸载、签名、更新器、
  安全、SBOM 或钢铁案例研究门禁已经通过。
