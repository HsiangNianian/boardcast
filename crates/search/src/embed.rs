//! Turning text into vectors with an embedding model behind llama-server's `/v1/embeddings`.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;

/// Which kind of embedding model it is. Each was trained to read a query and a passage differently, and
/// is far weaker given them bare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Google's EmbeddingGemma: a task prefix on the query, a title on the passage.
    Gemma,
    /// Qwen3-Embedding: an instruction on the query, the passage as it is.
    Qwen3,
}

impl Family {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "gemma" | "embeddinggemma" => Some(Family::Gemma),
            "qwen3" | "qwen" => Some(Family::Qwen3),
            _ => None,
        }
    }

    pub fn query(self, q: &str) -> String {
        match self {
            Family::Gemma => format!("task: search result | query: {q}"),
            Family::Qwen3 => format!("Instruct: Given a question about past meetings, retrieve the part of a meeting that answers it\nQuery:{q}"),
        }
    }

    /// A passage, with the meeting's name where the model takes one.
    pub fn passage(self, title: &str, text: &str) -> String {
        match self {
            Family::Gemma => format!("title: {} | text: {text}", if title.trim().is_empty() { "none" } else { title.trim() }),
            Family::Qwen3 => text.to_string(),
        }
    }
}

/// A client for one embedding server. Vectors are cut to `dims` and scaled back to unit length: both models
/// are trained so that the first dimensions carry the most, so a shorter vector stays useful and costs less.
pub struct Embedder {
    url: String,
    pub family: Family,
    pub dims: usize,
    agent: ureq::Agent,
}

/// How many texts go to the server at once.
const BATCH: usize = 16;

impl Embedder {
    pub fn new(base: &str, family: Family, dims: usize) -> Self {
        // Local only: proxies from the environment must not catch it.
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(120)).build();
        Self { url: format!("{}/v1/embeddings", base.trim_end_matches('/')), family, dims, agent }
    }

    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(BATCH) {
            let reply: Value = self.agent.post(&self.url).send_json(json!({ "input": batch })).context("asking the embedding server")?.into_json()?;
            let mut data = reply["data"].as_array().ok_or_else(|| anyhow!("no embeddings in the reply: {reply}"))?.clone();
            data.sort_by_key(|d| d["index"].as_u64().unwrap_or(0));
            if data.len() != batch.len() {
                bail!("asked for {} embeddings, got {}", batch.len(), data.len());
            }
            for d in data {
                let v: Vec<f32> = d["embedding"].as_array().ok_or_else(|| anyhow!("an embedding is missing"))?.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
                out.push(cut(&v, self.dims));
            }
        }
        Ok(out)
    }

    pub fn query(&self, q: &str) -> Result<Vec<f32>> {
        Ok(self.embed(&[self.family.query(q)])?.remove(0))
    }
}

/// The first `dims` values at unit length.
pub fn cut(v: &[f32], dims: usize) -> Vec<f32> {
    let v = &v[..dims.min(v.len())];
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / norm).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_vector_is_unit_length() {
        let v = cut(&[3.0, 4.0, 12.0], 2);
        assert_eq!(v.len(), 2);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn each_family_reads_queries_its_own_way() {
        assert!(Family::Gemma.query("x").starts_with("task: search result"));
        assert_eq!(Family::Gemma.passage("", "t"), "title: none | text: t");
        assert!(Family::Qwen3.query("x").ends_with("Query:x"));
        assert_eq!(Family::Qwen3.passage("Title", "t"), "t");
    }
}
