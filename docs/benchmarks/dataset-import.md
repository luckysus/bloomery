# Bloomery 100k Steel Dataset Import Gate

## Result

Gate F/H dataset import passed on 2026-08-13.

| Metric | Result | Gate |
| --- | ---: | ---: |
| Source rows | 100,000 | 100,000 |
| Source columns | 12 | reported |
| Source size | 7,528,510 bytes | reported |
| Preview P95 | 396.7488 ms | <= 5,000 ms |
| Peak working set | 66.7422 MB | <= 300 MB |

The runner creates a deterministic steel production CSV and exercises the same `preview_dataset` path used by the desktop analysis workbench. It scans the full source, records the real row count, keeps the bounded preview rows in memory, infers column quality, and writes machine-readable raw timings.

## Reference Machine

- OS: Windows 10 x86_64
- CPU: Intel64 Family 6 Model 165 Stepping 2, GenuineIntel
- Logical CPUs: 8
- Build: Cargo release benchmark, offline dependencies

## Corpus

The generated CSV contains heat ID, grade, process, chemistry, temperature, mechanical properties, defect rate, and note fields.

- Source SHA-256: `5a66efb6b2e54369b0ab09c7c388f5e87e1b3a5d2ffb29ba9887c071db2f542f`
- Warm-up rounds: 1
- Measured rounds: 5
- Raw preview timings: 279.1758 ms, 283.5109 ms, 288.2637 ms, 292.4145 ms, 396.7488 ms
- Warnings: none

## Reproduce

Run from the Bloomery repository root:

```powershell
powershell -File scripts/benchmark-dataset-import.ps1
```

The script runs `cargo bench -j 1 --offline --bench dataset_import`, writes the machine-readable result to `src-tauri/target/dataset-import-benchmark.json`, and exits nonzero when the preview does not cover exactly 100,000 rows, P95 exceeds five seconds, or measured working set exceeds 300 MB.
