use bloomery::rag::index::fts::{search as search_fts, FtsSearchRequest};
use bloomery::rag::index::lifecycle::{build_hnsw, open_hnsw, HnswVectorIndex};
use bloomery::rag::index::rebuild::{index_root, load_index_snapshot, IndexRebuildRequest};
use bloomery::rag::index::vector::{CandidateFilter, VectorIndex};
use bloomery::rag::model::{DocumentVersionId, KnowledgeBaseId};
use bloomery::rag::retrieve::rrf::{reciprocal_rank_fusion, RankedChunk};
use bloomery::rag::retrieve::{retrieve, HybridSearchRequest};
use bloomery::storage::migrations::migrate;
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

const CHUNK_COUNT: usize = 100_000;
const DIMENSION: usize = 64;
const WARMUP_ROUNDS: usize = 2;
const MEASURE_ROUNDS: usize = 5;
const RESULT_LIMIT: usize = 40;
const MINIMUM_RECALL: f64 = 0.95;
const MAX_TOTAL_P95_MS: f64 = 1_000.0;
const WORKSPACE: &str = "benchmark-local";
const BASE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const DOCUMENT_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const VERSION_ID: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const PROFILE_ID: &str = "11111111-1111-4111-8111-111111111111";
const MODEL_ID: &str = "benchmark-steel-v1";

const GRADES: [&str; 20] = [
    "Q235", "Q355", "Q390", "Q420", "Q460", "Q500", "304L", "316L", "42CrMo", "20MnCr5", "DP600",
    "DP780", "S235", "S355", "S460", "A36", "A572", "A992", "X70", "X80",
];
const PROCESSES: [&str; 17] = [
    "rolling",
    "annealing",
    "quenching",
    "tempering",
    "casting",
    "forging",
    "welding",
    "pickling",
    "galvanizing",
    "sintering",
    "cooling",
    "reheating",
    "descaling",
    "straightening",
    "coating",
    "drawing",
    "milling",
];
const DEFECTS: [&str; 19] = [
    "crack",
    "inclusion",
    "porosity",
    "scale",
    "delamination",
    "segregation",
    "blister",
    "buckle",
    "scratch",
    "pit",
    "void",
    "warp",
    "spall",
    "tear",
    "rust",
    "ripple",
    "dent",
    "fracture",
    "decarb",
];
const NOISE: [&str; 16] = [
    "sensor",
    "shift",
    "sample",
    "furnace",
    "coil",
    "batch",
    "operator",
    "ambient",
    "lubricant",
    "gauge",
    "maintenance",
    "inspection",
    "pressure",
    "humidity",
    "speed",
    "tension",
];
const QUERY_TOPICS: [(usize, usize, usize); 10] = [
    (1, 0, 0),
    (8, 2, 11),
    (15, 5, 3),
    (3, 11, 14),
    (18, 8, 6),
    (6, 16, 17),
    (12, 4, 2),
    (0, 7, 8),
    (10, 13, 5),
    (19, 15, 12),
];

type AnyResult<T> = Result<T, Box<dyn Error>>;

struct QueryCase {
    query: String,
    vector: Vec<f32>,
    relevant: HashSet<String>,
}

#[derive(Serialize)]
struct LatencySummary {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Serialize)]
struct RecallSummary {
    mean: f64,
    minimum: f64,
    relevant_per_query_min: usize,
    relevant_per_query_max: usize,
}

#[derive(Serialize)]
struct GateResult {
    minimum_recall_required: f64,
    total_p95_ms_maximum: f64,
    passed: bool,
}

#[derive(Serialize)]
struct ReferenceMachine {
    os: &'static str,
    architecture: &'static str,
    logical_cpus: usize,
    processor: String,
}

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    corpus_chunks: usize,
    corpus_sha256: String,
    embedding_dimension: usize,
    query_cases: usize,
    measured_queries: usize,
    setup_ms: f64,
    network_rerank_included: bool,
    reference_machine: ReferenceMachine,
    fts: LatencySummary,
    hnsw: LatencySummary,
    fusion: LatencySummary,
    total: LatencySummary,
    recall: RecallSummary,
    gate: GateResult,
}

fn main() -> AnyResult<()> {
    let root = std::env::temp_dir().join(format!("bloomery-retrieval-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root)?;
    let result = run_benchmark(&root);
    cleanup(&root);
    let report = result?;
    let output = std::env::var_os("BLOOMERY_BENCHMARK_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/retrieval-benchmark.json"));
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
            "retrieval gate failed: recall={}, total P95={} ms",
            report.recall.minimum, report.total.p95_ms
        ))
        .into())
    }
}

fn run_benchmark(root: &Path) -> AnyResult<BenchmarkReport> {
    let setup_started = Instant::now();
    let mut connection = Connection::open(root.join("benchmark.sqlite3"))?;
    migrate(&mut connection)?;
    let (cases, corpus_sha256) = seed_corpus(&mut connection)?;
    let request = IndexRebuildRequest {
        provider_profile_id: PROFILE_ID.to_string(),
        model_id: MODEL_ID.to_string(),
        dimension: DIMENSION as u32,
    };
    let snapshot = load_index_snapshot(&connection, WORKSPACE, &request)?;
    if snapshot.records.len() != CHUNK_COUNT {
        return Err(std::io::Error::other("benchmark corpus is incomplete").into());
    }
    let watermark = snapshot.watermark.clone();
    let root = index_root(root, &watermark);
    build_hnsw(&root, watermark.clone(), &snapshot.records)?;
    drop(snapshot);
    let index = open_hnsw(&root, &watermark)?;
    let setup_ms = elapsed_ms(setup_started.elapsed());

    for _ in 0..WARMUP_ROUNDS {
        for case in &cases {
            std::hint::black_box(total_search(&connection, &index, case)?);
        }
    }

    let base_id = KnowledgeBaseId::from_str(BASE_ID)?;
    let version_id = DocumentVersionId::from_str(VERSION_ID)?;
    let filter = CandidateFilter::new(vec![version_id]);
    let mut fts_times = Vec::new();
    let mut hnsw_times = Vec::new();
    let mut fusion_times = Vec::new();
    let mut total_times = Vec::new();
    let mut recalls = Vec::new();
    for _ in 0..MEASURE_ROUNDS {
        for case in &cases {
            let started = Instant::now();
            let lexical = search_fts(
                &connection,
                &FtsSearchRequest {
                    workspace_id: WORKSPACE.to_string(),
                    query: case.query.clone(),
                    knowledge_base_ids: vec![base_id],
                    limit: RESULT_LIMIT,
                },
            )?;
            fts_times.push(started.elapsed());

            let started = Instant::now();
            let dense = index.search(&case.vector, RESULT_LIMIT, &filter)?;
            hnsw_times.push(started.elapsed());

            let started = Instant::now();
            let lexical = lexical
                .into_iter()
                .map(|hit| RankedChunk {
                    version_id: hit.version_id,
                    chunk_id: hit.chunk_id,
                })
                .collect::<Vec<_>>();
            let dense = dense
                .into_iter()
                .map(|hit| RankedChunk {
                    version_id: hit.version_id,
                    chunk_id: hit.chunk_id,
                })
                .collect::<Vec<_>>();
            std::hint::black_box(reciprocal_rank_fusion(&lexical, &dense, 60, RESULT_LIMIT));
            fusion_times.push(started.elapsed());

            let started = Instant::now();
            let hits = total_search(&connection, &index, case)?;
            total_times.push(started.elapsed());
            let found = hits
                .iter()
                .filter(|hit| case.relevant.contains(hit.chunk_id.as_str()))
                .count();
            recalls.push(found as f64 / case.relevant.len() as f64);
        }
    }
    drop(index);

    let recall = summarize_recall(&cases, &recalls);
    let fts = summarize_latency(fts_times);
    let hnsw = summarize_latency(hnsw_times);
    let fusion = summarize_latency(fusion_times);
    let total = summarize_latency(total_times);
    let passed = recall.minimum >= MINIMUM_RECALL && total.p95_ms <= MAX_TOTAL_P95_MS;
    Ok(BenchmarkReport {
        schema_version: 1,
        corpus_chunks: CHUNK_COUNT,
        corpus_sha256,
        embedding_dimension: DIMENSION,
        query_cases: cases.len(),
        measured_queries: recalls.len(),
        setup_ms,
        network_rerank_included: false,
        reference_machine: ReferenceMachine {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            logical_cpus: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
            processor: std::env::var("PROCESSOR_IDENTIFIER")
                .unwrap_or_else(|_| "unknown".to_string()),
        },
        fts,
        hnsw,
        fusion,
        total,
        recall,
        gate: GateResult {
            minimum_recall_required: MINIMUM_RECALL,
            total_p95_ms_maximum: MAX_TOTAL_P95_MS,
            passed,
        },
    })
}

fn total_search(
    connection: &Connection,
    index: &HnswVectorIndex,
    case: &QueryCase,
) -> AnyResult<Vec<bloomery::rag::retrieve::RetrievedChunk>> {
    Ok(retrieve(
        connection,
        index,
        &HybridSearchRequest {
            workspace_id: WORKSPACE.to_string(),
            query: case.query.clone(),
            query_vector: case.vector.clone(),
            knowledge_base_ids: vec![KnowledgeBaseId::from_str(BASE_ID)?],
            lexical_limit: RESULT_LIMIT,
            dense_limit: RESULT_LIMIT,
            candidate_limit: RESULT_LIMIT,
            rrf_k: 60,
        },
    )?)
}

fn seed_corpus(connection: &mut Connection) -> AnyResult<(Vec<QueryCase>, String)> {
    connection.execute(
        "INSERT INTO knowledge_bases (id, workspace_id, name, created_at, updated_at)
         VALUES (?1, ?2, 'Steel benchmark', 'now', 'now')",
        params![BASE_ID, WORKSPACE],
    )?;
    connection.execute(
        "INSERT INTO knowledge_source_documents
         (id, workspace_id, knowledge_base_id, display_name, source_kind,
          active_version_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'deterministic-steel-corpus', 'text', ?4, 'now', 'now')",
        params![DOCUMENT_ID, WORKSPACE, BASE_ID, VERSION_ID],
    )?;
    connection.execute(
        "INSERT INTO knowledge_document_versions
         (id, workspace_id, document_id, content_sha256, mime_type, parser, parser_version,
          chunk_policy_version, embedding_profile_id, embedding_model_id, embedding_dimension,
          expected_asset_count, expected_chunk_count, created_at, activated_at)
         VALUES (?1, ?2, ?3, ?4, 'text/plain', 'benchmark', '1', 'steel-v1', ?5, ?6,
                 ?7, 0, ?8, 'now', 'now')",
        params![
            VERSION_ID,
            WORKSPACE,
            DOCUMENT_ID,
            "a".repeat(64),
            PROFILE_ID,
            MODEL_ID,
            DIMENSION as i64,
            CHUNK_COUNT as i64
        ],
    )?;

    let mut cases = QUERY_TOPICS
        .iter()
        .map(|&(grade, process, defect)| QueryCase {
            query: format!(
                "{} {} {}",
                GRADES[grade], PROCESSES[process], DEFECTS[defect]
            ),
            vector: topic_vector(grade, process, defect),
            relevant: HashSet::new(),
        })
        .collect::<Vec<_>>();
    let mut corpus_digest = Sha256::new();
    let transaction = connection.transaction()?;
    {
        let mut chunks = transaction.prepare(
            "INSERT INTO knowledge_chunks
             (id, workspace_id, version_id, ordinal, text, source_location_json,
              content_sha256, policy_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'steel-v1', 'now')",
        )?;
        let mut fts = transaction.prepare(
            "INSERT INTO knowledge_chunks_fts
             (workspace_id, knowledge_base_id, document_id, version_id, chunk_id,
              title_path, source_name, grade_aliases, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'deterministic-steel-corpus', ?7, ?8)",
        )?;
        let mut vectors = transaction.prepare(
            "INSERT INTO knowledge_vectors
             (id, workspace_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_blob, vector_sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'steel-v1', ?7, ?8, 'now')",
        )?;
        let mut embeddings = transaction.prepare(
            "INSERT INTO knowledge_chunk_embeddings
             (workspace_id, version_id, chunk_id, provider_profile_id, model_id, dimension,
              normalized_text_sha256, policy_version, vector_key, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'steel-v1', ?8, 'now')",
        )?;
        for ordinal in 0..CHUNK_COUNT {
            let grade = ordinal % GRADES.len();
            let process = (ordinal / GRADES.len()) % PROCESSES.len();
            let defect = (ordinal / (GRADES.len() * PROCESSES.len())) % DEFECTS.len();
            let chunk_id = format!("chunk-{ordinal:06}");
            let vector_key = format!("vector-{ordinal:06}");
            let hash = format!("{:064x}", ordinal + 1);
            let text = format!(
                "{} {} process control for {}. {} {} batch {} temperature {}.",
                GRADES[grade],
                PROCESSES[process],
                DEFECTS[defect],
                NOISE[(ordinal * 7) % NOISE.len()],
                NOISE[(ordinal * 11 + 3) % NOISE.len()],
                ordinal,
                700 + ordinal % 600
            );
            let location = format!(
                r#"{{"kind":"heading","path":["Benchmark","{}"]}}"#,
                PROCESSES[process]
            );
            let vector = corpus_vector(grade, process, defect, ordinal);
            let blob = vector
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            let vector_sha256 = format!("{:x}", Sha256::digest(&blob));
            chunks.execute(params![
                chunk_id,
                WORKSPACE,
                VERSION_ID,
                ordinal as i64,
                text,
                location,
                hash
            ])?;
            fts.execute(params![
                WORKSPACE,
                BASE_ID,
                DOCUMENT_ID,
                VERSION_ID,
                chunk_id,
                PROCESSES[process],
                GRADES[grade].to_ascii_lowercase(),
                text
            ])?;
            vectors.execute(params![
                vector_key,
                WORKSPACE,
                PROFILE_ID,
                MODEL_ID,
                DIMENSION as i64,
                hash,
                blob,
                vector_sha256
            ])?;
            embeddings.execute(params![
                WORKSPACE,
                VERSION_ID,
                chunk_id,
                PROFILE_ID,
                MODEL_ID,
                DIMENSION as i64,
                hash,
                vector_key
            ])?;
            for (case, &(case_grade, case_process, case_defect)) in
                cases.iter_mut().zip(QUERY_TOPICS.iter())
            {
                if (grade, process, defect) == (case_grade, case_process, case_defect) {
                    case.relevant.insert(chunk_id.clone());
                }
            }
            corpus_digest.update(chunk_id.as_bytes());
            corpus_digest.update(text.as_bytes());
            corpus_digest.update(vector_sha256.as_bytes());
        }
    }
    transaction.commit()?;
    connection.execute_batch("ANALYZE; PRAGMA optimize;")?;
    if cases.iter().any(|case| case.relevant.is_empty()) {
        return Err(std::io::Error::other("benchmark query has no relevant chunks").into());
    }
    Ok((cases, format!("{:x}", corpus_digest.finalize())))
}

fn topic_vector(grade: usize, process: usize, defect: usize) -> Vec<f32> {
    let mut vector = vec![0.0; DIMENSION];
    vector[grade] = 1.0;
    vector[GRADES.len() + process] = 1.0;
    vector[GRADES.len() + PROCESSES.len() + defect] = 1.0;
    vector
}

fn corpus_vector(grade: usize, process: usize, defect: usize, ordinal: usize) -> Vec<f32> {
    let mut vector = topic_vector(grade, process, defect);
    let noise_start = GRADES.len() + PROCESSES.len() + DEFECTS.len();
    for offset in 0..(DIMENSION - noise_start) {
        let value = ((ordinal as u64 * (offset as u64 + 17) + 31) % 997) as f32;
        vector[noise_start + offset] = value / 99_700.0;
    }
    vector
}

fn summarize_latency(mut values: Vec<Duration>) -> LatencySummary {
    values.sort_unstable();
    LatencySummary {
        samples: values.len(),
        p50_ms: percentile_ms(&values, 0.50),
        p95_ms: percentile_ms(&values, 0.95),
        max_ms: values.last().copied().map(elapsed_ms).unwrap_or(0.0),
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

fn summarize_recall(cases: &[QueryCase], values: &[f64]) -> RecallSummary {
    RecallSummary {
        mean: values.iter().sum::<f64>() / values.len() as f64,
        minimum: values.iter().copied().fold(1.0, f64::min),
        relevant_per_query_min: cases
            .iter()
            .map(|case| case.relevant.len())
            .min()
            .unwrap_or(0),
        relevant_per_query_max: cases
            .iter()
            .map(|case| case.relevant.len())
            .max()
            .unwrap_or(0),
    }
}

fn elapsed_ms(value: Duration) -> f64 {
    value.as_secs_f64() * 1_000.0
}

fn cleanup(root: &Path) {
    for _ in 0..3 {
        if fs::remove_dir_all(root).is_ok() || !root.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
