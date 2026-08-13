use bloomery::steel::{preview_dataset, DatasetPreviewRequest};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ROW_COUNT: usize = 100_000;
const COLUMN_COUNT: usize = 12;
const WARMUP_ROUNDS: usize = 1;
const MEASURE_ROUNDS: usize = 5;
const MAX_PREVIEW_P95_MS: f64 = 5_000.0;
const MAX_WORKING_SET_MB: f64 = 300.0;

type AnyResult<T> = Result<T, Box<dyn Error>>;

#[derive(Serialize)]
struct LatencySummary {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    raw_ms: Vec<f64>,
}

#[derive(Serialize)]
struct MemorySummary {
    before_working_set_mb: Option<f64>,
    after_working_set_mb: Option<f64>,
    peak_working_set_mb: Option<f64>,
    gate_working_set_mb: Option<f64>,
}

#[derive(Serialize)]
struct ReferenceMachine {
    os: &'static str,
    architecture: &'static str,
    logical_cpus: usize,
    processor: String,
}

#[derive(Serialize)]
struct GateResult {
    row_count_required: usize,
    preview_p95_ms_maximum: f64,
    working_set_mb_maximum: f64,
    passed: bool,
}

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    source_rows: usize,
    source_columns: usize,
    source_bytes: u64,
    source_sha256: String,
    preview_rows: usize,
    preview_columns: usize,
    preview_truncated: bool,
    sample_rows: usize,
    warnings: Vec<String>,
    reference_machine: ReferenceMachine,
    preview: LatencySummary,
    memory: MemorySummary,
    gate: GateResult,
}

fn main() -> AnyResult<()> {
    let root =
        std::env::temp_dir().join(format!("bloomery-dataset-import-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root)?;
    let result = run_benchmark(&root);
    cleanup(&root);
    let report = result?;
    let output = std::env::var_os("BLOOMERY_BENCHMARK_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/dataset-import-benchmark.json"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&output, &json)?;
    println!("{json}");
    if report.gate.passed {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "dataset import gate failed: preview P95={} ms, working_set={:?} MB",
            report.preview.p95_ms, report.memory.gate_working_set_mb
        ))
        .into())
    }
}

fn run_benchmark(root: &Path) -> AnyResult<BenchmarkReport> {
    let csv_path = root.join("steel-100k.csv");
    let source_sha256 = write_csv(&csv_path)?;
    let source_bytes = fs::metadata(&csv_path)?.len();
    let request = DatasetPreviewRequest {
        source_path: csv_path.to_string_lossy().into_owned(),
        sheet: None,
    };

    for _ in 0..WARMUP_ROUNDS {
        std::hint::black_box(preview_dataset(&request)?);
    }

    let before = process_memory();
    let mut timings = Vec::new();
    let mut final_preview = None;
    for _ in 0..MEASURE_ROUNDS {
        let started = Instant::now();
        let preview = preview_dataset(&request)?;
        timings.push(started.elapsed());
        final_preview = Some(preview);
    }
    let after = process_memory();
    let preview = final_preview.ok_or_else(|| std::io::Error::other("benchmark did not run"))?;
    let latency = summarize_latency(timings);
    let peak_working_set_mb = after
        .as_ref()
        .and_then(|memory| memory.peak_working_set_mb)
        .or_else(|| {
            before
                .as_ref()
                .and_then(|memory| memory.peak_working_set_mb)
        });
    let after_working_set_mb = after.as_ref().and_then(|memory| memory.working_set_mb);
    let gate_working_set_mb = peak_working_set_mb.or(after_working_set_mb);
    let passed = preview.row_count == ROW_COUNT
        && preview.column_count == COLUMN_COUNT
        && !preview.truncated
        && latency.p95_ms <= MAX_PREVIEW_P95_MS
        && gate_working_set_mb.is_none_or(|value| value <= MAX_WORKING_SET_MB);

    Ok(BenchmarkReport {
        schema_version: 1,
        source_rows: ROW_COUNT,
        source_columns: COLUMN_COUNT,
        source_bytes,
        source_sha256,
        preview_rows: preview.row_count,
        preview_columns: preview.column_count,
        preview_truncated: preview.truncated,
        sample_rows: preview.sample_rows.len(),
        warnings: preview.warnings,
        reference_machine: ReferenceMachine {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            logical_cpus: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
            processor: std::env::var("PROCESSOR_IDENTIFIER")
                .unwrap_or_else(|_| "unknown".to_string()),
        },
        preview: latency,
        memory: MemorySummary {
            before_working_set_mb: before.as_ref().and_then(|memory| memory.working_set_mb),
            after_working_set_mb,
            peak_working_set_mb,
            gate_working_set_mb,
        },
        gate: GateResult {
            row_count_required: ROW_COUNT,
            preview_p95_ms_maximum: MAX_PREVIEW_P95_MS,
            working_set_mb_maximum: MAX_WORKING_SET_MB,
            passed,
        },
    })
}

fn write_csv(path: &Path) -> AnyResult<String> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writeln!(
        writer,
        "heat_id,grade,process,carbon,manganese,silicon,temperature,yield_strength,tensile_strength,elongation,defect_rate,notes"
    )?;
    let mut digest = Sha256::new();
    for row in 0..ROW_COUNT {
        let line = format!(
            "H-{row:06},Q{},rolling,{:.3},{:.3},{:.3},{},{},{},{:.2},{:.4},batch-{}",
            235 + (row % 8) * 20,
            0.06 + (row % 40) as f64 / 1_000.0,
            0.8 + (row % 70) as f64 / 100.0,
            0.1 + (row % 20) as f64 / 100.0,
            850 + row % 300,
            320 + row % 180,
            450 + row % 220,
            16.0 + (row % 90) as f64 / 10.0,
            (row % 25) as f64 / 10_000.0,
            row % 512
        );
        digest.update(line.as_bytes());
        digest.update(b"\n");
        writeln!(writer, "{line}")?;
    }
    writer.flush()?;
    Ok(format!("{:x}", digest.finalize()))
}

#[derive(Clone)]
struct ProcessMemory {
    working_set_mb: Option<f64>,
    peak_working_set_mb: Option<f64>,
}

#[cfg(windows)]
fn process_memory() -> Option<ProcessMemory> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    (ok != 0).then(|| ProcessMemory {
        working_set_mb: Some(bytes_to_mb(counters.WorkingSetSize as u64)),
        peak_working_set_mb: Some(bytes_to_mb(counters.PeakWorkingSetSize as u64)),
    })
}

#[cfg(not(windows))]
fn process_memory() -> Option<ProcessMemory> {
    None
}

fn summarize_latency(mut values: Vec<Duration>) -> LatencySummary {
    values.sort_unstable();
    let raw_ms = values.iter().copied().map(elapsed_ms).collect::<Vec<_>>();
    LatencySummary {
        samples: values.len(),
        p50_ms: percentile_ms(&values, 0.50),
        p95_ms: percentile_ms(&values, 0.95),
        max_ms: values.last().copied().map(elapsed_ms).unwrap_or(0.0),
        raw_ms,
    }
}

fn percentile_ms(values: &[Duration], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(values.len() - 1);
    elapsed_ms(values[index])
}

fn elapsed_ms(value: Duration) -> f64 {
    value.as_secs_f64() * 1_000.0
}

fn bytes_to_mb(value: u64) -> f64 {
    value as f64 / 1_048_576.0
}

fn cleanup(root: &Path) {
    for _ in 0..3 {
        if fs::remove_dir_all(root).is_ok() || !root.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
