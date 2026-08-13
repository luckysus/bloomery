# Bloomery Agent Performance Gate

## Result

Agent event persistence and large-conversation replay passed on 2026-08-13.

| Metric | Result | Gate |
| --- | ---: | ---: |
| Conversation messages replayed | 10,000 | exactly 10,000 |
| Event append minimum throughput | 19,558 events/s | >= 250 events/s |
| Event append median throughput | 20,673 events/s | reported |
| Conversation replay P95 | 11.43 ms | <= 3,000 ms |
| Peak working set | 20.49 MB | <= 300 MB |

The benchmark uses the real SQLite migration and repository paths used by
the local agent runtime. It persists 5,000 ordered agent events across five
rounds, replays a 10,000-message conversation five times, records raw
samples, and exits nonzero when a gate fails.

## Reference Machine

- OS: Windows 10 x86_64
- CPU: Intel64 Family 6 Model 165 Stepping 2, GenuineIntel
- Logical CPUs: 8
- Build: Cargo release benchmark, offline dependencies

## Reproduce

Run from the Bloomery repository root:

```powershell
powershell -File scripts/benchmark-agent-performance.ps1
```

The script writes the machine-readable result to
`src-tauri/target/agent-performance-benchmark.json`.
