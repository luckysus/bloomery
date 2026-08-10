# Bloomery Steel Domain And Compute Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the official steel domain package, production-data workbench, model inference, training, constrained optimization, and reproducible steel evaluations without private cloud dependencies.

**Architecture:** Domain knowledge and deterministic tools live in a signed declaration-only package. Rust owns data, permissions, tasks, and audit records. A signed self-contained Python worker performs mature scientific-computing operations through versioned stdio JSON-RPC, with no network listener, database ownership, or credentials.

**Tech Stack:** Rust, SQLite, CSV/XLSX, JSON-RPC, Python 3.12, uv lockfile, PyInstaller, pandas, scikit-learn, Optuna, ONNX Runtime, Windows Job Objects.

---

## Target files

```text
domain-packs/steel/{manifest.json,prompts,terminology,retrieval,mappings,evaluations,assets}
src-tauri/src/steel/{mod,units,calculators,datasets,analysis}.rs
src-tauri/src/compute/{mod,protocol,supervisor,artifacts}.rs
src-tauri/src/storage/migrations/0010_steel_data.sql
compute-worker/{pyproject.toml,uv.lock,bloomery_worker,tests,build.ps1}
src-tauri/tests/{steel,compute_worker}.rs
docs/steel/{data-model,models,optimization,evaluation}.md
```

### Task 1: Establish a licensed steel terminology source

**Files:** Create steel package manifest, terminology files, source ledger, and validation tests.

- [x] **Step 1: Write package tests** requiring unique canonical terms, aliases, category, units where relevant, source/license metadata, and no ambiguous alias without a disambiguation rule.
- [x] **Step 2: Run domain validation** and expect missing package.
- [x] **Step 3: Add versioned terminology** for grades, composition elements, properties, defects, standards, and steelmaking/refining/casting/heating/rolling stages. Include Chinese and English aliases; do not copy restricted standards text.
- [x] **Step 4: Run terminology and license-ledger tests** and expect pass.

**Progress evidence (2026-08-10):** The steel terminology source is
versioned and license-audited. `assets/terminology.json` schema 1.1.0
carries 32 authored terms across grades, composition elements, mechanical
properties, defects, standards, and the steelmaking/refining/casting/
heating/rolling stages with Chinese and English aliases, unit declarations
for quantitative categories, and per-term source references. The new
`assets/source-ledger.json` records every source with publisher, license,
and an explicit `restricted_text_redistributed: false` guarantee; standards
are referenced by identifier only. Both assets are SHA-256 pinned in the
manifest. `tests/steel_terminology.rs` enforces unique ids/canonicals,
alias uniqueness with disambiguation rules, category and process-stage
coverage, unit declarations, ledger resolution, and pinned-hash package
loading; all 6 tests pass alongside the existing package and domain
suites.

### Task 2: Implement deterministic steel calculators

**Files:** Create `steel/units.rs`, `calculators.rs`, package tool declarations, and `tests/steel.rs`.

- [ ] **Step 1: Write reference-vector tests** for composition/unit normalization and supported carbon-equivalent formulas. Every vector records formula version and source.
- [ ] **Step 2: Run `cargo test --test steel calculators`** and expect failure.
- [ ] **Step 3: Implement typed calculator inputs/outputs.** Reject missing required elements, invalid percentages, unknown units, and silent formula substitution. Return formula ID, expression, normalized inputs, result, and applicability note.
- [ ] **Step 4: Run calculator and tool-schema tests** and expect pass.

### Task 3: Import and validate production datasets

**Files:** Create `0010_steel_data.sql`, `steel/datasets.rs`, package mapping presets, and dataset tests.

- [ ] **Step 1: Write CSV/XLSX fixture tests** for encoding, sheet selection, duplicate columns, type inference, date/heat/coil IDs, units, missing values, invalid numbers, and 100k-row streaming behavior.
- [ ] **Step 2: Run `cargo test --test steel datasets`** and expect missing importer.
- [ ] **Step 3: Implement staged import.** Preview inferred mappings and quality report before persistence. Store original column name, canonical field, unit conversion, data type, missing/invalid counts, and source hash. Activation is atomic.
- [ ] **Step 4: Run dataset and migration tests** and expect pass.

### Task 4: Build Rust-side profiling and evidence tools

**Files:** Create `steel/analysis.rs`, register built-in tools, and extend steel tests.

- [ ] **Step 1: Write tests** for descriptive statistics, distributions, grouped summaries, correlations, outlier flags, missingness, and row-level evidence IDs.
- [ ] **Step 2: Run `cargo test --test steel analysis`** and expect failure.
- [ ] **Step 3: Implement bounded analysis.** Use numerically stable streaming statistics. Results include dataset version, filters, selected columns, sample count, exclusions, and evidence references; correlation is not described as causation.
- [ ] **Step 4: Run analysis and Agent tool tests** and expect pass.

### Task 5: Define the compute worker protocol

**Files:** Create Rust compute protocol/supervisor modules, Python protocol package, and contract fixtures.

- [ ] **Step 1: Write cross-language fixtures** for hello/capabilities, submit, progress, result, cancel, error, and shutdown. Include protocol and worker versions.
- [ ] **Step 2: Run Rust and Python contract tests** and expect missing implementations.
- [ ] **Step 3: Implement framed JSON-RPC over stdio.** Rust generates task workspaces and passes only validated paths. Worker receives no API keys, does not listen on a port, and reports artifacts by hash plus manifest.
- [ ] **Step 4: Run round-trip contract tests** with Unicode and large progress streams.

### Task 6: Supervise and constrain the local worker

**Files:** Create `compute/supervisor.rs`, `artifacts.rs`, worker bootstrap, and `tests/compute_worker.rs`.

- [ ] **Step 1: Write tests** for missing binary, bad signature/hash, protocol mismatch, crash, timeout, cancellation, stderr bounds, artifact traversal, shutdown, and restart.
- [ ] **Step 2: Run supervisor tests** and expect failure.
- [ ] **Step 3: Implement supervision.** Verify manifest/hash before launch, use a dedicated working directory, minimal environment, Windows Job Object process tree termination and resource limits, bounded logs, and task checkpoints.
- [ ] **Step 4: Run supervisor and persistent-task tests** and expect pass.

### Task 7: Implement reproducible model training

**Files:** Create worker dataset/training modules, model metadata schema, tests, and docs.

- [ ] **Step 1: Write deterministic training tests** for train/validation split, group/time leakage protection, preprocessing fit scope, seeds, missing data, regression metrics, feature importance, cancellation, and artifact reload.
- [ ] **Step 2: Run worker tests** and expect missing trainer.
- [ ] **Step 3: Implement supported pipelines.** Include linear/ElasticNet, Random Forest, HistGradientBoosting, and an explicitly installed XGBoost capability. Persist environment lock hash, data version/hash, field mapping, split policy, parameters, metrics, feature schema, and applicability range.
- [ ] **Step 4: Run Python tests and Rust end-to-end worker training test** and expect pass.

**Progress evidence (2026-08-10):** Added the first executable training slice
with a standard-library linear-regression pipeline and framed Worker operations
for training and prediction. It validates bounded numeric matrices, deterministic
random/group/time splits, training-only imputation and standardization, ridge
stabilization, regression metrics, feature importance, applicability ranges,
field mappings, deterministic artifact IDs, and artifact-backed prediction.
The Python Worker suite passes 12 tests. The Task remains open until the
remaining supported model families, cancellation, persistent artifact storage,
and Rust end-to-end task integration are complete.

**Progress evidence (2026-08-10):** Rust full offline verification now passes
the compute worker round-trip, training persistence, prediction persistence,
applicability metadata, protocol freshness, and all existing integration
targets. The analysis UI also exposes prediction task cancellation and retry
through the shared background-task bridge with bilingual state labels. ONNX
model lifecycle, constrained optimization, artifact packaging, and the
remaining worker cancellation and persistence requirements remain open.

### Task 8: Implement ONNX export/import and local inference

**Files:** Create worker inference/export modules, Rust model repository/tool adapter, and tests.

- [ ] **Step 1: Write tests** for valid model, unsupported operator, schema mismatch, out-of-range input, batch inference, cancellation, and numeric parity with the source model.
- [ ] **Step 2: Run inference tests** and expect failure.
- [ ] **Step 3: Implement ONNX model lifecycle.** Validate model hash, opset, input/output schemas, preprocessing manifest, and supported runtime before activation. Inference outputs model/version, normalized inputs, predictions, confidence information when available, and applicability warnings.
- [ ] **Step 4: Run parity and Agent tool tests** and expect pass.

**Progress evidence (2026-08-10):** The import/inference half of the ONNX
loop is closed. The Worker rejects hash mismatches, opsets outside 7-21,
non-default operator domains, and operators outside the explicit whitelist
before session activation, validates I/O schemas and preprocessing
manifests, runs chunked batch inference with staged progress frames, and
records applicability warnings plus manifest-declared applicability-distance
confidence. Rust persists ONNX tasks, pins model hashes through
`hash_onnx_model_file`, enforces the result contract, and passes the
end-to-end scheduler test. The analysis UI ships model selection, manifest
editing, task cancel/retry, and result display with confidence metadata.
The Python Worker suite passes 24 tests and the frontend suite passes 76.
Export, numeric parity with source models, the Agent tool adapter, and full
cooperative cancellation remain open.

### Task 9: Implement constrained process optimization

**Files:** Create worker optimization modules, Rust task/tool adapters, and tests.

- [x] **Step 1: Write tests** for bounds, equality/inequality constraints, fixed values, infeasible problems, single/multi-objective runs, deterministic seeds, cancellation, progress, and recommendation validation.
- [x] **Step 2: Run optimization tests** and expect failure.
- [x] **Step 3: Implement Optuna-backed search.** Support single-objective Bayesian/TPE and multi-objective NSGA-II. Re-evaluate every returned candidate through the active model and hard constraints; reject infeasible recommendations rather than hiding violations.
- [ ] **Step 4: Run worker, task recovery, and Agent integration tests** and expect pass.

**Progress evidence (2026-08-10):** The constrained optimization loop is
closed end to end for trained linear models. The Worker module
`optimization.py` validates the artifact, bounds, objectives, fixed values,
and linear equality/inequality constraints, runs constraint-aware TPE for
single-objective and NSGA-II for multi-objective searches with deterministic
seeds and cooperative cancellation, deterministically projects candidates
onto equality surfaces, and re-evaluates every recommendation through the
active model and hard constraints; infeasible problems raise
`optimization_infeasible` with violation details instead of hiding them.
Rust registers `compute_optimize_constrained`, builds the payload from the
completed training checkpoint, enforces the recommendation result contract,
and passes the end-to-end scheduler test with a hard inequality constraint.
The analysis UI adds direction, objectives, bounds, fixed values, one linear
constraint, trials/seed, task cancel/retry, and recommendation display.
The Python Worker suite passes 45 tests, the Rust offline suite passes,
and the frontend suite passes 81 tests. The Agent tool adapter for
optimization remains open.

### Task 10: Build versioned steel evaluations

**Files:** Create package evaluation cases, runner, golden outputs, and `docs/steel/evaluation.md`.

- [ ] **Step 1: Define licensed evaluation sets** for terminology, retrieval, citations, calculations, dataset mapping, profiling, inference, training, and optimization.
- [ ] **Step 2: Run the initial evaluator** and record failures rather than weakening thresholds.
- [ ] **Step 3: Add release thresholds** for exact calculators, citation validity, retrieval recall, mapping accuracy, model reproducibility, and optimization feasibility. Record provider/model versions for nondeterministic LLM cases.
- [ ] **Step 4: Run Gate F verification:** package validation/signature, all steel/worker tests, 100k-row import, ONNX parity, restart-safe training/optimization, no network/private server access, and evaluation thresholds.

### Task 11: Package the worker for Windows

**Files:** Create `compute-worker/build.ps1`, artifact manifest/signing scripts, and installer integration metadata.

- [ ] **Step 1: Build from locked dependencies** in an isolated environment and record Python/package/native library versions.
- [ ] **Step 2: Run the packaged executable on clean Windows** without system Python and execute hello, training, inference, cancel, and shutdown tests.
- [ ] **Step 3: Produce signed Full and add-on artifacts.** Full installer embeds the same worker artifact that the lightweight install can obtain from the public signed release source.
- [ ] **Step 4: Verify hashes, signatures, SBOM, notices, offline launch, and no private URL** before marking the worker releasable.

## Completion evidence

- Licensed steel terminology ledger and signed package.
- Reference-verified calculator outputs and production-data import reports.
- Cross-language protocol, supervision, crash/cancel/restart evidence.
- Reproducible model/ONNX/optimization artifacts with complete provenance.
- Versioned steel evaluation report meeting every release threshold.
