pub mod chunk;
pub mod embed;
pub mod store;

use anyhow::Result;
use chunk::{Chunk, ChunkStrategy};
use embed::Embedder;
use std::path::{Path, PathBuf};
use store::{ScoredChunk, VectorStore};

pub struct RagPipeline {
    embedder: Embedder,
    store: VectorStore,
    store_path: PathBuf,
}

impl RagPipeline {
    pub fn new(ollama_base_url: &str, embed_model: &str, project_dir: &Path) -> Self {
        let store_path = project_dir.join(".turtle_vectors.json");
        Self {
            embedder: Embedder::new(ollama_base_url, embed_model),
            store: VectorStore::load(&store_path),
            store_path,
        }
    }

    pub async fn ingest(&mut self, source_id: &str, text: &str, strategy: Option<ChunkStrategy>) -> Result<usize> {
        let strategy = strategy.unwrap_or_else(|| chunk::auto_strategy(text, source_id));

        let chunks: Vec<Chunk> = if let ChunkStrategy::Semantic { max_chars, similarity_floor } = strategy {
            let embedder = self.embedder.clone();
            chunk::semantic_chunk_async(text, max_chars, similarity_floor, move |s: String| {
                let e = embedder.clone();
                Box::pin(async move { e.embed(&s).await })
            })
            .await?
        } else {
            chunk::split(text, strategy)
        };

        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        if texts.is_empty() {
            return Ok(0);
        }
        let vectors = self.embedder.embed_batch(&texts).await?;

        for (chunk, vector) in chunks.into_iter().zip(vectors.into_iter()) {
            self.store.upsert(source_id, chunk, vector);
        }
        self.store.save(&self.store_path);
        Ok(texts.len())
    }

    pub async fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<ScoredChunk>> {
        let qvec = self.embedder.embed(query).await?;
        Ok(self.store.search(&qvec, top_k))
    }

    pub fn render_context(chunks: &[ScoredChunk]) -> String {
        chunks
            .iter()
            .map(|c| format!("[{} | score {:.2}]\n{}", c.source_id, c.score, c.text))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

