# Meeting search evaluation

How well search finds the passage a question is about, across all meetings at once.

```sh
cargo build --release -p boardcast-tools
target/release/boardcast-tools search-eval . <model.gguf> gemma|qwen3 --out eval/reports/search/<name>.json
```

The models go in `eval/search/.models/` (not committed). The meetings are the speech evaluation's manual
transcripts (`eval/asr/meetings.json` and `corpus.json`; the Japanese committee minutes are cut at their
sentences). `queries.json` holds 74 questions over eight of them, in Chinese, English and Japanese: 42
paraphrased, 15 by a name, number or term, and 17 in another language than the meeting. Each lists exact
words the right passage contains, each found once in its meeting. A hit counts at rank 1 (R@1), in the top
five (R@5) and by its reciprocal rank in the top ten (MRR@10).

## Results, 2026-11-12

168 passages, 56,947 characters, llama.cpp b11954 on the CPU. Best of each, word weight 0.1:

| | EmbeddingGemma 300M Q8_0 | Qwen3-Embedding 0.6B Q8_0 |
|---|---|---|
| R@1 / R@5 / MRR@10, full size | 0.743 / 0.959 / 0.824 (768) | 0.743 / 0.959 / 0.834 (1024) |
| R@5 at 512 / 256 dimensions | 0.946 / 0.919 | 0.946 / 0.946 |
| R@5 across languages | 0.882 (15 of 17) | 0.941 (16 of 17) |
| Indexing / a query | 1613 characters/s / 30 ms | 461 characters/s / 176 ms |
| File | 334 MB | 639 MB |
| License | Gemma Terms of Use | Apache 2.0 |

One question is 1.35 points, so recall is a tie. A word weight of 0.1 brings keyword questions from 93% to
100% at no cost elsewhere; more starts to cost paraphrases. EmbeddingGemma is used, at 512 dimensions with a
word weight of 0.1 (`crates/search/src/lib.rs`), for being half the size and three to six times faster.
