use bloomery::agent::protocol::{AgentEventData, AgentMessageRole, MessageDelta};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::{conversations, events, runs};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration as StdDuration, Instant};
use uuid::Uuid;

const WORKSPACE: &str = "benchmark-agent";
const CONVERSATION_ID: u128 = 0x10000000000000000000000000000001;
const USER_MESSAGE_ID: u128 = 0x20000000000000000000000000000001;
const ASSISTANT_MESSAGE_ID: u128 = 0x30000000000000000000000000000001;
const RUN_ID_BASE: u128 = 0x40000000000000000000000000000000;
const EVENT_ID_BASE: u128 = 0x50000000000000000000000000000000;
const CONVERSATION_MESSAGES: usize = 10_000;
const EVENTS_PER_ROUND: usize = 1_000;
const MEASURE_ROUNDS: usize = 5;
const MIN_EVENT_THROUGHPUT: f64 = 250.0;
const MAX_REPLAY_P95_MS: f64 = 3_000.0;
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
struct ThroughputSummary {
    events_per_round: usize,
    samples: usize,
    minimum_events_per_second: f64,
    median_events_per_second: f64,
    raw_events_per_second: Vec<f64>,
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
    minimum_event_throughput_required: f64,
    replay_p95_ms_maximum: f64,
    working_set_mb_maximum: f64,
    passed: bool,
}

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    conversation_messages: usize,
    conversation_content_sha256: String,
    event_count: usize,
    reference_machine: ReferenceMachine,
    event_append: ThroughputSummary,
    conversation_replay: LatencySummary,
    replayed_messages: usize,
    memory: MemorySummary,
    gate: GateResult,
}

fn main() -> AnyResult<()> {
    let result = run_benchmark()?;
    let output = std::env::var_os("BLOOMERY_BENCHMARK_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/agent-performance-benchmark.json"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&result)?;
    fs::write(&output, &json)?;
    println!("{json}");
    if result.gate.passed {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "agent performance gate failed: event throughput={} events/s, replay P95={} ms, working_set={:?} MB",
            result.event_append.minimum_events_per_second,
            result.conversation_replay.p95_ms,
            result.memory.gate_working_set_mb
        ))
        .into())
    }
}

fn run_benchmark() -> AnyResult<BenchmarkReport> {
    let mut connection = Connection::open_in_memory()?;
    migrate(&mut connection)?;
    let conversation_content_sha256 = seed_conversation(&mut connection)?;
    let before = process_memory();
    let event_append = measure_event_append(&mut connection)?;
    let (conversation_replay, replayed_messages) = measure_conversation_replay(&connection)?;
    let after = process_memory();
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
    let passed = event_append.minimum_events_per_second >= MIN_EVENT_THROUGHPUT
        && conversation_replay.p95_ms <= MAX_REPLAY_P95_MS
        && gate_working_set_mb.is_none_or(|value| value <= MAX_WORKING_SET_MB)
        && replayed_messages == CONVERSATION_MESSAGES;

    Ok(BenchmarkReport {
        schema_version: 1,
        conversation_messages: CONVERSATION_MESSAGES,
        conversation_content_sha256,
        event_count: EVENTS_PER_ROUND * MEASURE_ROUNDS,
        reference_machine: ReferenceMachine {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            logical_cpus: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
            processor: std::env::var("PROCESSOR_IDENTIFIER")
                .unwrap_or_else(|_| "unknown".to_string()),
        },
        event_append,
        conversation_replay,
        replayed_messages,
        memory: MemorySummary {
            before_working_set_mb: before.as_ref().and_then(|memory| memory.working_set_mb),
            after_working_set_mb,
            peak_working_set_mb,
            gate_working_set_mb,
        },
        gate: GateResult {
            minimum_event_throughput_required: MIN_EVENT_THROUGHPUT,
            replay_p95_ms_maximum: MAX_REPLAY_P95_MS,
            working_set_mb_maximum: MAX_WORKING_SET_MB,
            passed,
        },
    })
}

fn seed_conversation(connection: &mut Connection) -> AnyResult<String> {
    let conversation_id = Uuid::from_u128(CONVERSATION_ID).to_string();
    let user_message_id = Uuid::from_u128(USER_MESSAGE_ID).to_string();
    let timestamp = "2026-08-13T00:00:00Z";
    connection.execute(
        "INSERT INTO conversations
         (id, workspace_id, title, created_at, updated_at, pinned, archived)
         VALUES (?1, ?2, 'Large steel conversation', ?3, ?3, 0, 0)",
        params![conversation_id, WORKSPACE, timestamp],
    )?;
    connection.execute(
        "INSERT INTO messages
         (id, workspace_id, conversation_id, role, content, response_json, created_at)
         VALUES (?1, ?2, ?3, 'user', ?4, NULL, ?5)",
        params![
            user_message_id,
            WORKSPACE,
            conversation_id,
            "Explain the controlled cooling schedule for Q355B steel.",
            timestamp
        ],
    )?;

    let transaction = connection.transaction()?;
    let mut statement = transaction.prepare(
        "INSERT INTO messages
         (id, workspace_id, conversation_id, role, content, response_json, created_at)
         VALUES (?1, ?2, ?3, 'agent', ?4, NULL, ?5)",
    )?;
    let mut digest = Sha256::new();
    for index in 0..CONVERSATION_MESSAGES.saturating_sub(1) {
        let content = format!(
            "Heat H-{index:05}: Q355B rolling pass {}; cooling temperature {} C; yield strength {} MPa; evidence window {}.",
            index % 8,
            700 + index % 250,
            355 + index % 90,
            index % 32
        );
        digest.update(content.as_bytes());
        digest.update(b"\n");
        statement.execute(params![
            Uuid::from_u128(0x60000000000000000000000000000000 + index as u128).to_string(),
            WORKSPACE,
            conversation_id,
            content,
            format!(
                "2026-08-13T00:00:{:02}.{:09}Z",
                index / 1_000 % 60,
                index % 1_000
            )
        ])?;
    }
    drop(statement);
    transaction.commit()?;
    Ok(format!("{:x}", digest.finalize()))
}

fn measure_event_append(connection: &mut Connection) -> AnyResult<ThroughputSummary> {
    let conversation_id = Uuid::from_u128(CONVERSATION_ID);
    let user_message_id = Uuid::from_u128(USER_MESSAGE_ID);
    let assistant_message_id = Uuid::from_u128(ASSISTANT_MESSAGE_ID);
    let base_timestamp = DateTime::parse_from_rfc3339("2026-08-13T01:00:00Z")?.with_timezone(&Utc);
    let mut throughputs = Vec::with_capacity(MEASURE_ROUNDS);

    for round in 0..MEASURE_ROUNDS {
        let run_id = Uuid::from_u128(RUN_ID_BASE + round as u128);
        runs::create(
            connection,
            runs::NewAgentRun {
                id: run_id,
                workspace_id: WORKSPACE.to_string(),
                conversation_id,
                user_message_id,
                event_id: Uuid::from_u128(EVENT_ID_BASE + (round * (EVENTS_PER_ROUND + 1)) as u128),
                timestamp: base_timestamp + Duration::seconds(round as i64),
            },
        )?;
        let started = Instant::now();
        for index in 0..EVENTS_PER_ROUND {
            events::append(
                connection,
                WORKSPACE,
                run_id,
                Uuid::from_u128(
                    EVENT_ID_BASE + 1 + (round * (EVENTS_PER_ROUND + 1) + index) as u128,
                ),
                base_timestamp
                    + Duration::seconds(round as i64)
                    + Duration::microseconds(index as i64),
                AgentEventData::MessageDelta(MessageDelta {
                    message_id: assistant_message_id,
                    role: AgentMessageRole::Assistant,
                    delta: if index % 2 == 0 {
                        "Q355".to_string()
                    } else {
                        "B".to_string()
                    },
                }),
            )?;
        }
        let seconds = started.elapsed().as_secs_f64();
        throughputs.push(EVENTS_PER_ROUND as f64 / seconds.max(f64::EPSILON));
    }

    let mut sorted = throughputs.clone();
    sorted.sort_by(f64::total_cmp);
    Ok(ThroughputSummary {
        events_per_round: EVENTS_PER_ROUND,
        samples: throughputs.len(),
        minimum_events_per_second: sorted[0],
        median_events_per_second: sorted[sorted.len() / 2],
        raw_events_per_second: throughputs,
    })
}

fn measure_conversation_replay(connection: &Connection) -> AnyResult<(LatencySummary, usize)> {
    let conversation_id = Uuid::from_u128(CONVERSATION_ID).to_string();
    let mut timings = Vec::with_capacity(MEASURE_ROUNDS);
    let mut replayed_messages = 0;
    for _ in 0..MEASURE_ROUNDS {
        let started = Instant::now();
        let messages = conversations::list_messages(connection, WORKSPACE, &conversation_id)?;
        timings.push(started.elapsed());
        replayed_messages = messages.len();
        std::hint::black_box(messages);
    }
    Ok((summarize_latency(timings), replayed_messages))
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

fn summarize_latency(mut values: Vec<StdDuration>) -> LatencySummary {
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

fn percentile_ms(values: &[StdDuration], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(values.len() - 1);
    elapsed_ms(values[index])
}

fn elapsed_ms(value: StdDuration) -> f64 {
    value.as_secs_f64() * 1_000.0
}

fn bytes_to_mb(value: u64) -> f64 {
    value as f64 / 1_048_576.0
}
