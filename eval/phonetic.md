# Phonetic label and command evaluation

These cases measured the earlier app's pronunciation matching. They now run against the GPUI app's own
matching in `crates/agent/src/sound.rs` (#117), as the test `the_kept_pronunciation_cases_resolve`:
Chinese by pinyin without tones, English by the CMU Pronouncing Dictionary, Japanese by KANJIDIC2's kanji
readings. All 45 resolve as expected. The command cases below (negation, deletion blocked by sound) are
checked only for which label a word resolves to; what the board then does is the model's.

The fixtures in [phonetic-cases.json](phonetic-cases.json) are **synthetic transcript perturbations**, not recorded speech. They check the shared production resolver against exact-name lookup with glossary correction, then check explicit commands, required abstentions and delegation. Reported correctness includes expected abstentions; wrong matches count incorrect non-empty selections. These numbers do not measure ASR accuracy or guarantee an LLM's decisions. Audio model comparisons remain in [asr/](asr/README.md).

Examples include `健全 → 鉴权`, `cash → cache`, `公正 → 構成`, kana readings, mixed Japanese/English commands and `API = A P I, エーピーアイ`. Negative cases cover homophone collisions, multiple pointed targets, negation, unknown names, questions, whole-word boundaries and deletion with an inexact target. Raw-text retention and the board model's behavior require separate integration checks; these resolver fixtures do not establish them.

## Production behavior

The Rust implementation is `crates/agent/src/sound.rs`. The saved glossary can contain names and explicit aliases;
correction is applied to each recognized sentence before it reaches the board model. The original remains in
`rawText`. The model also receives the glossary and bounded hints about possible existing board labels.

Chinese uses tone-free pinyin, English uses the bundled CMU Pronouncing Dictionary, and Japanese uses bundled
KANJIDIC2 readings. Unknown names need explicit aliases; Japanese reading matches are limited to text with kana.
Ambiguous label matches are left unresolved. These tests check label matching, not whether a model correctly
executes a negated command, deletes a node or interprets a whole meeting.

## Recorded-speech acceptance

Use [the own-voice recorder and comparison](asr/own-voice/README.md) for #117. Record the 30 sentences yourself,
then run `cargo run --release -p boardcast-asr-eval -- own-voice .`. The comparison recognizes each recording once
and scores those same sentences before and after the production glossary correction, including harmful changes.
The reference and vocabulary are fixed before recording. No recordings have been supplied for the current
acceptance run yet; the 45 passing synthetic cases are not a measured improvement in ASR accuracy.
