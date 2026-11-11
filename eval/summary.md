# Summary and template acceptance — 2026-11-11

`boardcast-tools summary-eval <root> [--llm <url> | --deepseek] [--repeat N] [--label name]`
uses the production summary path on the synthetic meetings in `summary-cases.json`. It writes every resulting
summary, its failed checks, generation duration and the case/source SHA-256 hashes beside a timestamped Markdown report. A failed
semantic check returns a nonzero exit status; the existing Eval workflow runs this check and uploads its evidence.
Generation duration includes the production summary calls and cleanup; it is not audio-to-screen latency.
Model labels accept version dots (such as `qwen3.5-4b-q4km-cpu`); path separators remain invalid.

The current prompt follows later answers, partial answers and withdrawals, retains stated deadlines and excludes
automatic speaker letters from model input. Saved transcript lines and their fingerprints stay unchanged. A blank
template topic is an agenda, not evidence of discussion; only discussed topics belong in the summary. Cleanup
removes empty template placeholders and deduplicates within a section, so a fact repeated as a decision survives.

## Measured result

Twelve cases, three runs per model, using the final production source. Full outputs, checks and provenance are in
[`summary-2026-11-11.json`](summary-2026-11-11.json). The initial keyword checks reported 36/36 for both models;
manual inspection found two omissions in those checks. The same frozen outputs were rescored against the stronger
checks below. Case inputs and production source are unchanged. The **30/36** result is the accepted report for
Qwen, not the earlier 36/36.

| Check group | Qwen3.5-4B Q4_K_M | DeepSeek Flash |
| --- | ---: | ---: |
| Chinese unconfirmed quantity stays open; confirmed Wednesday deployment retained | 0/3 | 3/3 |
| English unconfirmed quantity is not presupposed in an open question | 0/3 | 3/3 |
| Answered question closes, Chinese and English | 6/6 | 6/6 |
| Tentative answer, withdrawal, unnamed owner/deadline, immediate action | 12/12 | 12/12 |
| Partially answered question, Chinese and English | 6/6 | 6/6 |
| Retrospective and custom template summaries | 6/6 | 6/6 |
| **Total** | **30/36** | **36/36** |

The local run used llama.cpp **b11954**, CPU with four threads, a 4096-token context, temperature 0, repeat penalty
1.05 and thinking disabled. The remote run used `deepseek-flash`, temperature 0.2, thinking enabled, high reasoning
effort. The artifact stores the local model hash and hashes of both production source files and the checked cases.

Qwen still omits the explicitly stated Wednesday deployment in the Chinese case and asks “What are the two failure
criteria?” in the English case, treating the unconfirmed number as a premise. These remain failures for **#68**.
Extra prompt constraints and a shorter prompt introduced other omissions, so they were not adopted. The CI summary
check is expected to expose remaining model failures; a passing build does not close #68.

The earlier 17/24 and 20/24 measurements used eight cases and weaker checks; they are historical measurements,
not directly comparable to the twelve-case result above. Three repeated runs of a deterministic local model are
not three independent meetings. DeepSeek's passing result covers these short synthetic conversations only.

## Rejected follow-up probes

Each probe below ran all twelve cases once on the same local model and CPU server. None was adopted;
the production source still matches the accepted report above. [Outputs and example patches](summary-rejected-2026-11-11.json)
are retained so these local improvements are not mistaken for overall fixes.
The thinking sampler follows [Qwen's model card](https://huggingface.co/Qwen/Qwen3.5-4B); the reasoning budget
is an experimental cap, not a model recommendation.

| Probe | Passed | Reason to reject |
| --- | ---: | --- |
| Thinking enabled, 256-token reasoning budget and the recommended thinking sampler | 7/12 | Lost tasks/dates, changed uncertainty and broke a custom-template result. |
| One plain Chinese/English input-output example | 10/12 | Fixed the original two failures but lost template headings, a task owner and a decision. |
| Example with an explicit decision, used only without a template | 10/12 | Kept an answered question open and treated an agreed duration as unresolved. |

At `c39109b`, four-platform packaging and core checks passed, as did all three ASR jobs and their comparison.
The text job stopped before calling the summary model because the default label contains a dot. `77007e5`
fixes that validation and tests the actual CLI argument path; a rerun must reach the semantic checks before
it can provide a CI summary result. Build success alone does not establish summary accuracy.

## Human review

Read each distinct output against every source turn, checking facts from all topics, question certainty (including
presuppositions inside questions), later answers/withdrawals, unresolved parts, owners and stated dates. For a
template, verify heading placement, factual content under each heading and omission of topics never discussed.
Do not infer success solely from keywords: a check can miss omitted information or a wrong relationship.

The existing board Eval also ran two pre-filled template cases once per model. Both models kept all template topics and attached each spoken fact to the correct parent. DeepSeek item F1 was 1.00 for both; Qwen was 1.00 for the Chinese retrospective and 0.67 for the English custom template because its latency measurement was a `note`, not an `item`. Manual graph inspection confirmed the parent was correct. These final graphs are included in the evidence JSON; the score was not relabeled as perfect.

The template GUI was exercised in an isolated Linux app: choose Retrospective, see its pre-filled topics, save those
topics as a custom template, and create another meeting from it. Audio settings were exercised with a capability
stub, without opening a microphone or invoking Apple's recognizer. Mac microphone, physical permissions and
long-meeting behavior are outside this result; follow [the Mac acceptance plan](macos/README.md).
