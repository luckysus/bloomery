use crate::rag::model::{ChunkId, DocumentVersionId};
use std::collections::{HashMap, HashSet};

const MAX_FUSED_RESULTS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RankedChunk {
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FusedChunk {
    pub version_id: DocumentVersionId,
    pub chunk_id: ChunkId,
    pub lexical_rank: Option<usize>,
    pub dense_rank: Option<usize>,
    pub rrf_score: f64,
}

pub fn reciprocal_rank_fusion(
    lexical: &[RankedChunk],
    dense: &[RankedChunk],
    rrf_k: u32,
    limit: usize,
) -> Vec<FusedChunk> {
    let limit = limit.min(MAX_FUSED_RESULTS);
    if limit == 0 {
        return Vec::new();
    }

    let mut ranks = HashMap::new();
    merge_ranks(lexical, true, &mut ranks);
    merge_ranks(dense, false, &mut ranks);
    let mut fused = ranks
        .into_iter()
        .map(|(chunk, (lexical_rank, dense_rank))| FusedChunk {
            version_id: chunk.version_id,
            chunk_id: chunk.chunk_id,
            lexical_rank,
            dense_rank,
            rrf_score: score(rrf_k, lexical_rank) + score(rrf_k, dense_rank),
        })
        .collect::<Vec<_>>();
    fused.sort_by(|left, right| {
        right
            .rrf_score
            .total_cmp(&left.rrf_score)
            .then_with(|| left.chunk_id.as_str().cmp(right.chunk_id.as_str()))
            .then_with(|| {
                left.version_id
                    .to_string()
                    .cmp(&right.version_id.to_string())
            })
    });
    fused.truncate(limit);
    fused
}

fn merge_ranks(
    chunks: &[RankedChunk],
    lexical: bool,
    ranks: &mut HashMap<RankedChunk, (Option<usize>, Option<usize>)>,
) {
    let mut seen = HashSet::new();
    let mut rank = 0;
    for chunk in chunks {
        if !seen.insert(chunk) {
            continue;
        }
        rank += 1;
        let entry = ranks.entry(chunk.clone()).or_insert((None, None));
        if lexical {
            entry.0 = Some(rank);
        } else {
            entry.1 = Some(rank);
        }
    }
}

fn score(rrf_k: u32, rank: Option<usize>) -> f64 {
    rank.map_or(0.0, |rank| 1.0 / (f64::from(rrf_k) + rank as f64))
}
