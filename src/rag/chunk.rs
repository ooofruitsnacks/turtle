use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub text: String,
    pub index: usize,
    pub strategy: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChunkStrategy {
    Token { max_tokens: usize, overlap: usize },
    Sentence { max_chars: usize },
    Recursive { max_chars: usize },
    Semantic { max_chars: usize, similarity_floor: f32 },
}

pub fn auto_strategy(text: &str, path_hint: &str) -> ChunkStrategy {
    let is_markdown = path_hint.ends_with(".md");
    let is_code = path_hint.ends_with(".rs") || path_hint.ends_with(".odin");
    let has_headers = text.lines().any(|l| l.trim_start().starts_with('#'));

    if is_markdown && has_headers {
        ChunkStrategy::Recursive { max_chars: 1200 }
    } else if is_code {
        ChunkStrategy::Recursive { max_chars: 1500 }
    } else {
        ChunkStrategy::Sentence { max_chars: 800 }
    }
}

pub fn split(text: &str, strategy: ChunkStrategy) -> Vec<Chunk> {
    match strategy {
        ChunkStrategy::Token { max_tokens, overlap } => token_chunk(text, max_tokens, overlap),
        ChunkStrategy::Sentence { max_chars } => sentence_chunk(text, max_chars),
        ChunkStrategy::Recursive { max_chars } => recursive_chunk(text, max_chars),
        ChunkStrategy::Semantic { max_chars, similarity_floor } => {
            let _ = similarity_floor;
            sentence_chunk(text, max_chars)
        }
    }
}

fn token_chunk(text: &str, max_tokens: usize, overlap: usize) -> Vec<Chunk> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![];
    }
    let step = max_tokens.saturating_sub(overlap).max(1);
    let mut chunks = Vec::new();
    let mut i = 0;
    let mut idx = 0;
    while i < words.len() {
        let end = (i + max_tokens).min(words.len());
        chunks.push(Chunk {
            text: words[i..end].join(" "),
            index: idx,
            strategy: "token".to_string(),
        });
        idx += 1;
        if end == words.len() {
            break;
        }
        i += step;
    }
    chunks
}

fn sentence_chunk(text: &str, max_chars: usize) -> Vec<Chunk> {
    let sentences = split_sentences(text);
    pack_units(&sentences, max_chars, "sentence")
}

fn split_sentences(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(?s)(.*?[.!?])(\s+|$)").unwrap();
    let mut sentences: Vec<String> = re
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();

    let consumed_len: usize = sentences.iter().map(|s| s.len()).sum();
    if consumed_len < text.trim().len() {
        let remainder = text.trim();
        if !sentences.iter().any(|s| remainder.ends_with(s.as_str())) {
            sentences.push(remainder.to_string());
        }
    }
    sentences
}

fn pack_units(units: &[String], max_chars: usize, strategy_name: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut idx = 0;

    for unit in units {
        if !current.is_empty() && current.len() + 1 + unit.len() > max_chars {
            chunks.push(Chunk { text: current.trim().to_string(), index: idx, strategy: strategy_name.to_string() });
            idx += 1;
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(unit);
    }
    if !current.trim().is_empty() {
        chunks.push(Chunk { text: current.trim().to_string(), index: idx, strategy: strategy_name.to_string() });
    }
    chunks
}

fn recursive_chunk(text: &str, max_chars: usize) -> Vec<Chunk> {
    let separators = ["\n## ", "\n# ", "\n\n", "\n", ". "];
    let sections = recursive_split(text, &separators, max_chars);
    pack_units(&sections, max_chars, "recursive")
}

fn recursive_split(text: &str, separators: &[&str], max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars || separators.is_empty() {
        return vec![text.trim().to_string()];
    }

    let sep = separators[0];
    let rest = &separators[1..];

    let parts: Vec<&str> = if text.contains(sep) {
        text.split(sep).collect()
    } else {
        return recursive_split(text, rest, max_chars);
    };

    let mut out = Vec::new();
    for part in parts {
        if part.trim().is_empty() {
            continue;
        }
        if part.len() > max_chars {
            out.extend(recursive_split(part, rest, max_chars));
        } else {
            out.push(part.trim().to_string());
        }
    }
    out
}

pub async fn semantic_chunk_async(
    text: &str,
    max_chars: usize,
    similarity_floor: f32,
    embed_fn: impl Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<f32>>> + Send>>,
) -> anyhow::Result<Vec<Chunk>> {
    let sentences = split_sentences(text);
    if sentences.is_empty() {
        return Ok(vec![]);
    }

    let mut vectors = Vec::with_capacity(sentences.len());
    for s in &sentences {
        vectors.push(embed_fn(s.clone()).await?);
    }

    let mut chunks = Vec::new();
    let mut current_text = sentences[0].clone();
    let mut current_centroid = vectors[0].clone();
    let mut count = 1.0f32;
    let mut idx = 0;

    for i in 1..sentences.len() {
        let sim = cosine(&current_centroid, &vectors[i]);
        let would_exceed = current_text.len() + sentences[i].len() + 1 > max_chars;

        if sim < similarity_floor || would_exceed {
            chunks.push(Chunk { text: current_text.trim().to_string(), index: idx, strategy: "semantic".to_string() });
            idx += 1;
            current_text = sentences[i].clone();
            current_centroid = vectors[i].clone();
            count = 1.0;
        } else {
            current_text.push(' ');
            current_text.push_str(&sentences[i]);
            // Running average centroid — cheap approximation of the chunk's meaning.
            for (c, v) in current_centroid.iter_mut().zip(vectors[i].iter()) {
                *c = (*c * count + v) / (count + 1.0);
            }
            count += 1.0;
        }
    }
    if !current_text.trim().is_empty() {
        chunks.push(Chunk { text: current_text.trim().to_string(), index: idx, strategy: "semantic".to_string() });
    }
    Ok(chunks)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

