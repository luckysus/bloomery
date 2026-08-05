use super::lifecycle_io::{
    activate, generation, prepare_root, read_current, temp_generation, validate_generation,
    validate_record_ids, write_metadata, RecordMetadata, HNSW_BASENAME,
};
use super::vector::{
    validate_query, CandidateFilter, FlatVectorIndex, IndexError, IndexWatermark, VectorHit,
    VectorIndex,
};
use crate::rag::model::{ChunkId, DocumentVersionId};
use hnsw_rs::prelude::{AnnT, DistCosine, Hnsw, HnswIo};
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender};

#[derive(Debug, Clone)]
pub struct VectorRecord {
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub generation_id: String,
    pub generation_path: PathBuf,
}

#[derive(Debug)]
pub struct HnswVectorIndex {
    watermark: IndexWatermark,
    records: Vec<RecordMetadata>,
    sender: SyncSender<SearchMessage>,
}

#[derive(Debug)]
pub enum ActiveVectorIndex {
    Hnsw(HnswVectorIndex),
    Flat(FlatVectorIndex),
}

impl ActiveVectorIndex {
    pub fn is_flat(&self) -> bool {
        matches!(self, Self::Flat(_))
    }
}

impl VectorIndex for ActiveVectorIndex {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &CandidateFilter,
    ) -> Result<Vec<VectorHit>, IndexError> {
        match self {
            Self::Hnsw(index) => index.search(query, limit, filter),
            Self::Flat(index) => index.search(query, limit, filter),
        }
    }

    fn watermark(&self) -> &IndexWatermark {
        match self {
            Self::Hnsw(index) => index.watermark(),
            Self::Flat(index) => index.watermark(),
        }
    }
}

impl VectorIndex for HnswVectorIndex {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &CandidateFilter,
    ) -> Result<Vec<VectorHit>, IndexError> {
        validate_query(query, self.watermark.dimension)?;
        let limit = limit.min(500);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let allowed = self
            .records
            .iter()
            .enumerate()
            .filter_map(|(id, record)| filter.allows(record.version_id).then_some(id))
            .collect::<Vec<_>>();
        if allowed.is_empty() {
            return Ok(Vec::new());
        }
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(SearchMessage {
                query: query.to_vec(),
                limit,
                allowed,
                response: response_sender,
            })
            .map_err(|_| IndexError::new("index_worker_failed", "HNSW worker stopped"))?;
        let neighbours = response_receiver
            .recv()
            .map_err(|_| IndexError::new("index_worker_failed", "HNSW worker stopped"))??;
        let mut hits = neighbours
            .into_iter()
            .map(|(id, distance)| {
                let record = self.records.get(id).ok_or_else(|| {
                    IndexError::new("index_records_invalid", "HNSW returned an unknown record")
                })?;
                Ok(VectorHit {
                    version_id: record.version_id,
                    chunk_id: record.chunk_id.clone(),
                    distance,
                })
            })
            .collect::<Result<Vec<_>, IndexError>>()?;
        hits.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then_with(|| left.chunk_id.as_str().cmp(right.chunk_id.as_str()))
                .then_with(|| {
                    left.version_id
                        .to_string()
                        .cmp(&right.version_id.to_string())
                })
        });
        hits.truncate(limit);
        Ok(hits)
    }

    fn watermark(&self) -> &IndexWatermark {
        &self.watermark
    }
}

pub fn build_hnsw(
    root: &Path,
    watermark: IndexWatermark,
    records: &[VectorRecord],
) -> Result<BuildOutcome, IndexError> {
    watermark.validate()?;
    validate_records(&watermark, records)?;
    prepare_root(root)?;
    let generation_id = uuid::Uuid::new_v4().to_string();
    let temporary = temp_generation(root, &generation_id);
    fs::create_dir(&temporary).map_err(io_error)?;
    let mut pending = PendingDirectory::new(temporary.clone());
    let hnsw: Hnsw<'_, f32, DistCosine> = Hnsw::new(16, records.len().max(1), 16, 200, DistCosine);
    for (id, record) in records.iter().enumerate() {
        hnsw.insert((&record.vector, id));
    }
    hnsw.file_dump(&temporary, HNSW_BASENAME)
        .map_err(|error| IndexError::new("index_dump_failed", error.to_string()))?;
    let metadata = records
        .iter()
        .map(|record| RecordMetadata {
            version_id: record.version_id,
            chunk_id: record.chunk_id.clone(),
        })
        .collect::<Vec<_>>();
    write_metadata(&temporary, &generation_id, &watermark, &metadata)?;
    validate_generation(&temporary, &watermark)?;
    validate_hnsw_file(&temporary, records.len())?;

    let generation_path = generation(root, &generation_id);
    fs::rename(&temporary, &generation_path).map_err(io_error)?;
    pending.disarm();
    activate(root, &generation_id)?;
    Ok(BuildOutcome {
        generation_id,
        generation_path,
    })
}

pub fn open_hnsw(root: &Path, expected: &IndexWatermark) -> Result<HnswVectorIndex, IndexError> {
    expected.validate()?;
    let (_, directory) = read_current(root)?;
    let records = validate_generation(&directory, expected)?;
    let sender = spawn_worker(directory, records.len())?;
    Ok(HnswVectorIndex {
        watermark: expected.clone(),
        records,
        sender,
    })
}

pub fn open_with_flat_fallback(
    connection: &Connection,
    root: &Path,
    expected: &IndexWatermark,
) -> Result<ActiveVectorIndex, IndexError> {
    match open_hnsw(root, expected) {
        Ok(index) => Ok(ActiveVectorIndex::Hnsw(index)),
        Err(_) => FlatVectorIndex::load(connection, expected).map(ActiveVectorIndex::Flat),
    }
}

fn validate_records(
    watermark: &IndexWatermark,
    records: &[VectorRecord],
) -> Result<(), IndexError> {
    if records.len() != watermark.chunk_count as usize {
        return Err(IndexError::new(
            "index_watermark_mismatch",
            "record count does not match the index watermark",
        ));
    }
    let mut identities = HashSet::new();
    for record in records {
        validate_record_ids(record.version_id, &record.chunk_id)?;
        if record.vector.len() != watermark.dimension as usize
            || record.vector.iter().any(|value| !value.is_finite())
            || record.vector.iter().all(|value| *value == 0.0)
        {
            return Err(IndexError::new(
                "index_vector_invalid",
                "HNSW vector has an invalid dimension or value",
            ));
        }
        if !identities.insert((record.version_id, record.chunk_id.clone())) {
            return Err(IndexError::new(
                "index_records_invalid",
                "HNSW records contain duplicate chunk identities",
            ));
        }
    }
    Ok(())
}

fn validate_hnsw_file(directory: &Path, expected_count: usize) -> Result<(), IndexError> {
    let mut io = HnswIo::new(directory, HNSW_BASENAME);
    let hnsw: Hnsw<'_, f32, DistCosine> = io
        .load_hnsw()
        .map_err(|error| IndexError::new("index_open_failed", error.to_string()))?;
    if hnsw.get_nb_point() != expected_count {
        return Err(IndexError::new(
            "index_watermark_mismatch",
            "HNSW point count does not match its watermark",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct SearchMessage {
    query: Vec<f32>,
    limit: usize,
    allowed: Vec<usize>,
    response: SyncSender<Result<Vec<(usize, f32)>, IndexError>>,
}

fn spawn_worker(
    directory: PathBuf,
    expected_count: usize,
) -> Result<SyncSender<SearchMessage>, IndexError> {
    let (sender, receiver) = mpsc::sync_channel::<SearchMessage>(8);
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("bloomery-hnsw".to_string())
        .spawn(move || {
            let mut io = HnswIo::new(&directory, HNSW_BASENAME);
            let hnsw: Hnsw<'_, f32, DistCosine> = match io.load_hnsw() {
                Ok(value) if value.get_nb_point() == expected_count => value,
                Ok(_) => {
                    let _ = ready_sender.send(Err(IndexError::new(
                        "index_watermark_mismatch",
                        "HNSW point count does not match its watermark",
                    )));
                    return;
                }
                Err(error) => {
                    let _ = ready_sender
                        .send(Err(IndexError::new("index_open_failed", error.to_string())));
                    return;
                }
            };
            if ready_sender.send(Ok(())).is_err() {
                return;
            }
            while let Ok(message) = receiver.recv() {
                let ef = (message.limit.saturating_mul(4)).max(32);
                let neighbours = hnsw
                    .search_filter(&message.query, message.limit, ef, Some(&message.allowed))
                    .into_iter()
                    .map(|neighbour| (neighbour.d_id, neighbour.distance))
                    .collect();
                let _ = message.response.send(Ok(neighbours));
            }
        })
        .map_err(|error| IndexError::new("index_worker_failed", error.to_string()))?;
    ready_receiver
        .recv()
        .map_err(|_| IndexError::new("index_worker_failed", "HNSW worker failed to start"))??;
    Ok(sender)
}

struct PendingDirectory {
    path: PathBuf,
    armed: bool,
}

impl PendingDirectory {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PendingDirectory {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn io_error(error: std::io::Error) -> IndexError {
    IndexError::new("index_io_failed", error.to_string())
}
