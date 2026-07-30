use super::chunk::Chunk;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    source_id: String,
    chunk: Chunk,
    vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VectorStore {
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoredChunk {
    pub source_id: String,
    pub text: String,
    pub score: f32,
}

impl VectorStore {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string(&self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn upsert(&mut self, source_id: &str, chunk: Chunk, vector: Vec<f32>) {
        if chunk.index == 0 {
            self.entries.retain(|e| e.source_id != source_id);
        }
        self.entries.push(Entry { source_id: source_id.to_string(), chunk, vector });
    }

    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<ScoredChunk> {
        let mut scored: Vec<ScoredChunk> = self
            .entries
            .iter()
            .map(|e| ScoredChunk {
                source_id: e.source_id.clone(),
                text: e.chunk.text.clone(),
                score: dot(query, &e.vector),
            })
            .collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        scored.truncate(top_k);
        scored
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

