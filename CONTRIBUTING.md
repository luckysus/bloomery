# Contributing / 贡献指南

Thanks for helping Bloomery. 感谢帮助 Bloomery。

## Development setup / 开发环境

- Windows 10/11, Node 20/22/24 (pinned 24.14.0), stable Rust, Python 3.12+ with `uv`.
- The repository root contains the nested `bloomery` Git repository for the desktop client; run Git commands inside the intended repository and never stage Bloomery files from the root repository.
  仓库根目录包含桌面客户端的嵌套 `bloomery` Git 仓库；Git 命令必须在目标仓库内执行，切勿从根仓库暂存 Bloomery 文件。

From `bloomery/frontend`: `npm install`, `npm run dev`, `npm run test`, `npm run build`.
From `bloomery/src-tauri`: `cargo check`, `cargo test`, `cargo fmt --check`.
From `bloomery/compute-worker`: `uv sync --frozen --extra packaging`, `python -m pytest -q`, `powershell -File build.ps1`.

## Quality bar / 质量门槛

- Every behavior change ships with tests; architecture boundary tests are executable rules, not suggestions.
  每个行为变更都要带测试；架构边界测试是可执行规则，而非建议。
- Run the local gates before pushing: `scripts/test.ps1 -Stage contracts|frontend|rust`, `cargo fmt --check`, and `git diff --check`. CI runs the same gates on every push.
  推送前本地复跑门禁；CI 每次推送都会运行同一套门禁。
- Do not weaken evaluation thresholds, permission boundaries, or signature checks to make a test pass.
  不得为通过测试而削弱评测门槛、权限边界或签名校验。
- Commit messages: use concise Chinese summaries (e.g. `完成ONNX推理任务闭环`); stage explicit files, never `git add .`.
  提交信息使用简洁中文；明确暂存文件，禁止 `git add .`。

## What we accept / 接受什么

- Bug fixes with regression tests; documentation improvements in both languages; deterministic evaluation cases with recorded sources; extension guides and domain-package tooling that respect the permission model.
  带回归测试的缺陷修复；双语文档改进；带来源记录的确定性评测用例；尊重权限模型的扩展指南与领域包工具。

## What we cannot accept / 不能接受

- Restricted standards text, licensed datasets, or telemetry of any kind.
  受限标准文本、授权数据集或任何形式的遥测。
- Features that require a Bloomery-hosted backend, account, or private cloud.
  任何依赖 Bloomery 托管后端、账号或私有云的功能。
