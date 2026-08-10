# Steel Evaluation / 钢铁评测

Bloomery ships a versioned evaluation suite at
`domain-packs/steel/evaluations/steel-evaluations-v1.json`, pinned by SHA-256
in the package manifest alongside the `steel-qa.jsonl` provider baseline.

Bloomery 提供版本化评测集 `steel-evaluations-v1.json`，与 `steel-qa.jsonl` 一起在领域包 manifest 中以 SHA-256 钉扎。

## Categories / 评测类别

| Category | Runner | Threshold | Status |
| --- | --- | --- | --- |
| calculators | Rust | 1.0 | Executed by `tests/steel_evaluations.rs` |
| dataset_mapping | Rust | 1.0 | Executed by `tests/steel_evaluations.rs` |
| dataset_profiling | Rust | 1.0 | Executed by `tests/steel_evaluations.rs` |
| terminology | Rust | 1.0 | Executed by `tests/steel_evaluations.rs` |
| inference | Worker | 1.0 | Executed by `compute-worker/tests/test_evaluations.py` |
| training_reproducibility | Worker | 1.0 | Executed by `compute-worker/tests/test_evaluations.py` |
| optimization_feasibility | Worker | 1.0 | Executed by `compute-worker/tests/test_evaluations.py` |
| retrieval | Provider | 0.95 | Deferred; must record provider/model/run_at |
| citation | Provider | 1.0 | Deferred; must record provider/model/run_at |
| terminology_qa | Provider | 1.0 | Deferred; must record provider/model/run_at |

Every calculator reference vector records its formula version
(`carbon-equivalent.iiw.v1` / `carbon-equivalent.pcm.v1`) and source. Unit
normalization is covered by mass-fraction vectors that must produce the same
result as their percent-mass twins.

每个计算器参考向量都记录公式版本与来源；质量分数向量必须与同组分的质量百分比向量得到相同结果，以覆盖单位归一化。

## Reporting Rules / 报告规则

- Reports record evaluation version, per-category score, threshold, pass
  counts, and verbatim failure details.
- Failures are recorded as-is; thresholds must never be weakened to hide a
  regression.
- Provider-dependent categories keep `provider`, `model`, and `run_at`
  recording fields and must not claim a score until executed against a named
  provider and model.
- The suite schema is versioned; unsupported schema versions are rejected.

报告必须记录评测版本、分类得分、门槛、通过数与逐条失败明细；不得通过降低门槛掩盖回归；Provider 类别在未用明确的 Provider 与模型执行前不得声称得分；评测 schema 带版本，不支持的版本直接拒绝。
