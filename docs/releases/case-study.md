# Reproducible steel case study / 可复现钢铁案例研究

This case study reproduces the deterministic steel workflow through one
repository-owned release entry point. The release check invokes that entry
point from the repository root and writes a redacted JSON execution report.
本案例研究通过仓库内统一的发布入口复现确定性钢铁工作流。发布检查会从
仓库根目录调用该入口，并写出不含密钥的 JSON 执行报告。

Run from the repository root in PowerShell / 在仓库根目录使用 PowerShell 运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/case-study.ps1 -Offline
```

The report is written to `artifacts/case-study/steel-case-study.json` by
default. The directory is ignored by Git and contains no credentials.

## Public input and provenance

The repository-owned input package is under
`case-studies/steel-release/`. Its CSV is synthetic demonstration data,
licensed under Apache-2.0, and contains no production measurements,
restricted standards text, personal data, or credentials.

`case-studies/steel-release/provenance.json` records the policy and SHA-256
digest. The manifest points to the repository's source ledger; the script
verifies that the ledger stays inside the repository, uses the same license
policy, and contains no redistributed restricted text. The verified dataset
and ledger digests are included in the redacted report. The case study is
therefore a software and release reproducibility demonstration, not evidence
about any real mill or production process.
报告默认写入 `artifacts/case-study/steel-case-study.json`。该目录已被 Git
忽略，报告不包含凭据。

## 1. Production data import / 生产数据导入

Command / 命令: `cargo test --offline --test steel_datasets -- --test-threads=1`
A CSV/XLSX fixture with duplicate headers, missing values, and invalid numbers
is previewed, mapped to canonical fields (`heat_id`, `temperature_c`,
`carbon_pct`, ...), and activated atomically. Expected: preview reports
missing/invalid counts; activation requires every numeric unit to have a
canonical field.
带重复列、缺失值与非法数字的 CSV/XLSX 夹具被预览、映射到规范字段并原子激活。预期：预览报告缺失/非法计数；激活要求每个数值单位都有规范字段。

## 2. Deterministic training / 确定性训练

Command / 命令（工作目录：`compute-worker/`）:
`compute-worker/.venv/Scripts/python.exe -m pytest tests/test_training.py tests/test_sklearn_training.py -q`
A linear artifact (`linear-regression.v1`) and scikit-learn artifacts
(`sklearn-pickle.v1`: ElasticNet, Random Forest, HistGradientBoosting) are
trained with train-only preprocessing, deterministic seeds, and an environment
lock (python + scikit-learn fingerprint hash). Expected: identical artifacts
for identical seeds; validation metrics recorded per split.
线性与 scikit-learn 产物以仅训练集预处理、确定性种子与环境锁训练。预期：相同种子产出相同产物；按划分记录验证指标。

## 3. Prediction with applicability / 带适用范围的预测

Command / 命令: `cargo test --offline --test compute_task scheduler_runs_prediction -- --test-threads=1`
The persisted linear artifact predicts `[[125.0]]` → `25.0` style outputs and
flags inputs outside the recorded applicability range. Expected: applicability
warning for out-of-range features; confidence null for linear artifacts.
持久化线性产物预测并标记超出记录适用范围的输入。预期：越界特征产生适用性告警；线性产物置信度为 null。

## 4. Constrained optimization / 约束优化

Command / 命令（分别在 `compute-worker/` 和 `src-tauri/`）:
`compute-worker/.venv/Scripts/python.exe -m pytest tests/test_optimization.py -q`；
`cargo test --offline --test compute_task scheduler_runs_optimization -- --test-threads=1`
Seeded TPE search minimizes the trained model under `temperature >= 4`; every
recommendation is re-evaluated through the model and hard constraints.
Expected: all recommendations feasible and at or above the constraint; same
seed reproduces the same recommendation set. Infeasible problems raise
`optimization_infeasible` with violation details.
带种子的 TPE 在 `temperature >= 4` 下最小化训练模型；每个推荐解都经模型与硬约束复核。预期：全部推荐可行且不低于约束；同种子复现同一推荐集；不可行问题抛 `optimization_infeasible` 并附违约明细。

## 5. ONNX export and parity / ONNX 导出与一致性

Command / 命令（分别在 `compute-worker/` 和 `src-tauri/`）:
`compute-worker/.venv/Scripts/python.exe -m pytest tests/test_onnx_export.py -q`；
`cargo test --offline --test compute_task scheduler_exports_onnx -- --test-threads=1`
The linear artifact is exported as a whitelisted ONNX graph (opset 13,
ir_version 10, operators within the inference whitelist). The exported model is
re-imported through `predict_onnx` and compared with the source artifact.
Expected: parity within 1e-4; operator whitelist and opset window enforced.
线性产物导出为白名单 ONNX 图（opset 13、ir_version 10、算子在推理白名单内）。导出模型经 `predict_onnx` 重新导入并与源产物对比。预期：1e-4 内一致；算子白名单与 opset 窗口被强制。

## 6. Evaluation thresholds / 评测门槛

Command / 命令（分别在 `compute-worker/` 和 `src-tauri/`）:
`compute-worker/.venv/Scripts/python.exe -m pytest tests/test_evaluations.py -q`；
`cargo test --offline --test steel_evaluations`
The versioned suite `domain-packs/steel/evaluations/steel-evaluations-v1.json`
runs calculators (IIW/Pcm reference vectors), dataset mapping, profiling,
terminology, inference, training reproducibility, and optimization feasibility
against thresholds of 1.0 (0.95 reserved for provider-run retrieval).
Expected: every rust/worker category meets its threshold; failures would be
recorded verbatim rather than hidden.
版本化评测集对计算器（IIW/Pcm 参考向量）、数据映射、剖析、术语、推理、训练可复现性与优化可行性按 1.0 门槛运行（provider 检索预留 0.95）。预期：每个 rust/worker 类别达标；失败将被逐条记录而非隐藏。

## 7. Packaging / 打包

Command / 命令: `powershell -File compute-worker/build.ps1 -SkipTests`
Produces `bloomery-compute-worker.exe` from the committed `uv.lock`, plus
`worker-artifact-manifest.json` (SHA-256, Python and package versions,
`signature: unsigned-explicit`), `worker-sbom.json`, and a checksum file.
Expected: the packaged executable answers hello/shutdown frames without system
Python (`python -m pytest tests/test_packaged_worker.py -q`).
从已提交 `uv.lock` 产出单文件 exe 及清单/SBOM/校验和。预期：打包 exe 在无系统 Python 环境下响应 hello/shutdown 帧。
