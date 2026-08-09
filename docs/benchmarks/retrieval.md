# Bloomery 100k Local Retrieval Gate

## Result

Gate C retrieval passed on 2026-08-09.

| Metric | Result | Gate |
| --- | ---: | ---: |
| Recall minimum | 1.000 | >= 0.950 |
| FTS P95 | 4.75 ms | reported |
| HNSW P95 | 15.95 ms | reported |
| RRF fusion P95 | 0.10 ms | reported |
| Total local candidate retrieval P95 | 26.19 ms | <= 1000 ms |

The maximum measured total latency was 29.59 ms. Network reranking was disabled and excluded from every latency value.

## Reference Machine

- OS: Windows 10 x86_64
- CPU: Intel64 Family 6 Model 165 Stepping 2, GenuineIntel
- Logical CPUs: 8
- Build: Cargo release benchmark, offline dependencies
- Setup time: 205.92 seconds for SQLite corpus creation, snapshot validation, HNSW build, atomic activation, reopen, and checksum validation

## Corpus

The runner deterministically creates 100,000 steel-domain chunks from 20 grades, 17 processes, 19 defects, and independent operational noise terms. Each chunk has a deterministic 64-dimensional vector with separate grade, process, defect, and low-amplitude noise dimensions.

- Corpus SHA-256: `7a08b95e6a08a041d2875715547c61cfe1a14c96a5646ae685377ab80d542dc7`
- Query cases: 10 grade/process/defect combinations
- Relevant chunks per query: 15-16
- Measured queries: 50 after two warm-up rounds
- Candidate limit: 40 lexical, 40 dense, 40 fused
- RRF k: 60

Recall is the fraction of each query's known relevant chunk IDs present in the final fused candidate set. The reported latency is warm local retrieval: SQLite filtering and FTS, resident HNSW search, deterministic RRF, and authoritative source fetch. It excludes corpus setup, embedding provider calls, and network reranking.

## Reproduce

Run from the Bloomery repository root:

```powershell
powershell -File scripts/benchmark-retrieval.ps1
```

The script runs `cargo bench -j 1 --offline --bench retrieval`, writes the machine-readable result to `src-tauri/target/retrieval-benchmark.json`, and exits nonzero when minimum recall is below 0.95 or total local P95 exceeds one second.
