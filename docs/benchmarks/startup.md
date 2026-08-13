# Bloomery Startup and Idle Memory Gate

## Result

Cold start and idle memory passed on 2026-08-13.

| Metric | Result | Gate |
| --- | ---: | ---: |
| Cold-start P50 | 293.3562 ms | reported |
| Cold-start P95 | 339.3318 ms | <= 3,000 ms |
| Idle working-set P50 | 28.4492 MB | reported |
| Idle working-set P95 | 28.4570 MB | <= 300 MB |

The benchmark launches the actual release binary
`src-tauri/target/release/bloomery.exe`. Each round uses a fresh temporary
`APPDATA` and `LOCALAPPDATA` directory, waits for the real main window handle,
settles for three seconds, samples the process working set ten times, and
terminates the process tree before deleting the temporary profile.

## Reference Machine

- OS: Windows 10 x86_64
- CPU: Intel64 Family 6 Model 165 Stepping 2, GenuineIntel
- Logical CPUs: 8
- Rounds: 5
- Idle samples per round: 10
- Release binary SHA-256:
  `558f61882f294870e586795bbd07f96e0440d9ac4b809c4103a8398687bc3088`

## Reproduce

Run from the Bloomery repository root:

```powershell
powershell -File scripts/benchmark-startup.ps1
```

The script writes the machine-readable result to
`src-tauri/target/startup-performance-benchmark.json`.
