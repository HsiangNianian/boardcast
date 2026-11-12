//! A meeting's passages and their vectors, kept in one file beside the meeting.
//!
//! The file is a short header, the passages as JSON, then the vectors as little-endian f32. Its fingerprint
//! names the model, the vector size, the way passages are cut and the meeting's content, so an index made
//! any other way, or for an older version of the meeting, is simply made again.

use crate::chunk::Chunk;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MAGIC: &[u8; 8] = b"BCSRCH01";
/// Bumped whenever passages are cut differently, so every index is made again.
pub const CHUNKING: u32 = 1;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Header {
    fingerprint: String,
    dims: usize,
    /// The meeting's name when its passages were embedded, which the model read with each of them.
    #[serde(default)]
    title: String,
    chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    pub fingerprint: String,
    pub dims: usize,
    pub title: String,
    pub chunks: Vec<Chunk>,
    /// `chunks.len() * dims` values, each vector at unit length.
    pub vectors: Vec<f32>,
}

/// What an index depends on: the model, its vector size, the cutting and the content.
pub fn fingerprint(model: &str, dims: usize, content: &str) -> String {
    let hash: String = Sha256::digest(content.as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect();
    format!("{model}/{dims}/c{CHUNKING}/{hash}")
}

impl Index {
    pub fn new(fingerprint: String, dims: usize, chunks: Vec<Chunk>, vectors: Vec<Vec<f32>>) -> Self {
        Self { fingerprint, dims, title: String::new(), chunks, vectors: vectors.into_iter().flatten().collect() }
    }

    pub fn titled(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// Whether this index was made by the same model, at the same size and cut the same way as one with
    /// `fingerprint`, whatever the content: then its vectors can be reused for passages that did not change.
    pub fn same_kind(&self, fingerprint: &str) -> bool {
        let kind = |f: &str| f.rsplit_once('/').map(|(k, _)| k.to_string());
        kind(&self.fingerprint).is_some() && kind(&self.fingerprint) == kind(fingerprint)
    }

    pub fn vector(&self, i: usize) -> &[f32] {
        &self.vectors[i * self.dims..(i + 1) * self.dims]
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let header = serde_json::to_vec(&Header { fingerprint: self.fingerprint.clone(), dims: self.dims, title: self.title.clone(), chunks: self.chunks.clone() })?;
        let mut bytes = Vec::with_capacity(16 + header.len() + self.vectors.len() * 4);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(header.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&header);
        for v in &self.vectors {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        // Written aside and moved into place, so a reader never sees half a file.
        let partial = path.with_extension("index.partial");
        fs::write(&partial, &bytes).with_context(|| format!("writing {}", partial.display()))?;
        fs::rename(&partial, path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        if bytes.len() < 16 || &bytes[..8] != MAGIC {
            bail!("{} is not a search index", path.display());
        }
        let len = u64::from_le_bytes(bytes[8..16].try_into()?) as usize;
        let body = bytes.get(16..16 + len).context("the index is cut short")?;
        let header: Header = serde_json::from_slice(body)?;
        let raw = &bytes[16 + len..];
        if raw.len() != header.chunks.len() * header.dims * 4 {
            bail!("{} has {} bytes of vectors for {} passages", path.display(), raw.len(), header.chunks.len());
        }
        let vectors = raw.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
        Ok(Self { fingerprint: header.fingerprint, dims: header.dims, title: header.title, chunks: header.chunks, vectors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Source;

    #[test]
    fn an_index_comes_back_as_it_was_saved() {
        let dir = std::env::temp_dir().join(format!("boardcast-search-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let chunks = vec![
            Chunk { source: Source::Transcript { first: 0, last: 2 }, text: "上线改到周二".into() },
            Chunk { source: Source::Summary { heading: "To-dos".into() }, text: "canary".into() },
        ];
        let index = Index::new(fingerprint("m", 2, "content"), 2, chunks, vec![vec![0.6, 0.8], vec![1.0, 0.0]]).titled("Release review");
        let path = dir.join("a.index");
        index.save(&path).unwrap();
        assert_eq!(Index::load(&path).unwrap(), index);
        assert_eq!(index.vector(1), &[1.0, 0.0]);
        assert_ne!(fingerprint("m", 2, "content"), fingerprint("m", 2, "changed"));
        assert!(index.same_kind(&fingerprint("m", 2, "changed")));
        assert!(!index.same_kind(&fingerprint("m", 3, "content")));
    }
}
