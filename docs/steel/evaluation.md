# Steel Evaluation / 钢铁评测

The checked-in fixture is a smoke baseline for package loading and evidence-oriented behavior. It is not a claim that the agent is scientifically accurate on every steel task.

仓库中的 fixture 只用于验证领域包加载和证据优先行为，不代表智能体已经在所有钢铁任务上达到科学准确性。

## Current Cases / 当前用例

- Terminology: preserve `Q355B` and request product form, thickness, temperature, and standard context before asserting properties.
- Data mapping: map heat number / 炉号 to `heat_id` and preserve the source column.
- Citation: do not state a nominal property value without local evidence and a resolvable citation.

## Release Gates / 发布门槛

Before declaring the official package releasable, add licensed and reproducible evaluation sets for:

- terminology and bilingual alias resolution;
- local retrieval recall and citation validity;
- unit normalization and deterministic calculators;
- CSV/XLSX mapping and data-quality reporting;
- inference provenance and constrained optimization feasibility.

Evaluation output must record package version, source hashes, parser and model versions, provider capability, thresholds, and failures. Thresholds must not be weakened to hide a regression.

发布评测必须记录领域包版本、源文件摘要、解析器和模型版本、Provider 能力、门槛及失败样例，不能通过降低门槛掩盖回归。
