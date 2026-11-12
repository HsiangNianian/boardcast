//! Finding what was said in past meetings by what it means, not only by its words.
//!
//! A meeting is cut into passages ([`chunk`]): runs of transcript lines, each topic of a board with what
//! hangs under it, and each part of the summary. An embedding model served by llama-server turns them into
//! vectors ([`embed`]), kept in an index beside the meeting ([`index`]). A query is ranked against every
//! passage by the cosine of its vector, together with how many of its words the passage shares
//! ([`rank`]), which keeps names, numbers and terms from being lost to meaning alone.
//!
//! These are tools for the interface to call. Nothing here decides what to look for on its own.

pub mod chunk;
pub mod embed;
pub mod index;
pub mod rank;
pub mod service;

pub use chunk::{Chunk, Said, Source};
pub use embed::{Embedder, Family};
pub use index::Index;
pub use rank::{rank, Hit};

/// The model meetings are searched with, chosen on eval/search/queries.json against Qwen3-Embedding-0.6B:
/// the same recall at half the size and a third of the time (eval/reports/search).
pub const MODEL: &str = "embeddinggemma-300m";
pub const FAMILY: Family = Family::Gemma;
/// Vectors are cut to this size: within a query of full size on the evaluation, at two thirds the space.
pub const DIMS: usize = 512;
/// How much a full match of the query's words adds to the cosine; more starts to cost paraphrases.
pub const WORD_WEIGHT: f32 = 0.1;
/// Below this score a passage is not shown at all, and nor is one further than `WITHIN_BEST` below the best
/// hit. On the evaluation together they keep 95% of right answers and let the best unrelated passage
/// through for 18% of questions, where a floor alone that keeps as many lets through 43%.
pub const MIN_SCORE: f32 = 0.30;
pub const WITHIN_BEST: f32 = 0.10;
