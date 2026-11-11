# Summary and template acceptance — 2026-11-12

`boardcast-tools summary-eval <root> [--llm <url> | --deepseek] [--cases path] [--repeat N] [--label name]`
uses the production summary path on the synthetic meetings in `summary-cases.json`. It writes every resulting
summary, its failed checks, generation duration and the case, checker and production-source SHA-256 hashes beside a timestamped Markdown report. A failed
semantic check returns a nonzero exit status; the existing Eval workflow runs this check and uploads its evidence.
Generation duration includes the production summary calls and cleanup; it is not audio-to-screen latency.
Model labels accept version dots (such as `qwen3.5-4b-q4km-cpu`); path separators remain invalid.
The suite now has 15 cases, including mixed confirmed/tentative dates, a corrected date with a separate rehearsal,
and an explicitly conditional question beside an assigned task. Historical totals below retain their original case sets.

The current prompt follows later answers, partial answers and withdrawals, retains stated deadlines and excludes
automatic speaker letters from model input. Saved transcript indices stay unchanged; new timed captures enter
the summary and its fingerprint in audio order, while legacy untimed lines retain their order. A blank
template topic is an agenda, not evidence of discussion; only discussed topics belong in the summary. Cleanup
removes empty template placeholders from all four sections and deduplicates within a section, so a fact repeated
as a decision survives. Nonempty topic subsections are retained.

## Current options and earlier CI evidence

The [checker correction](summary-checker-2026-11-12.json) removes a confirmed false positive: an open topic
such as “移动版发布日期” asserts no date and needs no uncertainty keyword. A check with `each_with` instead applies
its assertions separately to each bullet, to-do or paragraph containing one of the selected terms. Here it
requires the existing uncertainty terms only in items mentioning Friday; a qualifier in another item cannot
make a definite date pass. The separate requirement to retain the mobile-release topic remains. Original
utterances and boards are unchanged; these word checks still do not establish complete semantic acceptance.

The actual old and new Rust checkers re-scored 75 frozen outputs without calling a model. Exactly one result
changes: the existing 9B report goes from **11/15 to 12/15** after removing that date-label false positive,
while retaining the named-owner failure. The 4B local report stays **12/15** and the `1bd7b05` CI summary stays
**36/45**, with the same three failed cases and both templates passing **6/6**. The regression fails before the
change and passes after; all 14 tools tests pass. Production inference is unchanged. The CI summary artifact is
available before its remaining board suites finish; it does not mean the whole Eval run passed.

The [latest frozen investigation](summary-options-2026-11-12.json) retains three unsuccessful alternatives.
A shorter rule set scores **1/4** on the same four screening cases as the original 4B control (**1/4**) and
changes Monday rehearsal to Sunday. Four separate section requests score **2/4**, using 16 calls; the English
quantity remains presupposed, Monday is still lost and a deployment task is added without an explicit assignment.
Neither prompt nor the multi-call architecture was adopted.

The existing Qwen3.5-9B option scores **12/15** with the old checks, but loses the explicitly named owner 小王
while retaining the task. The raw response already has this omission. The existing case now also requires that
owner in To-dos; the same frozen 9B outputs re-score to **11/15**, without regeneration. The earlier 4B output
retains 小王 and passes this added assertion. Source utterances and boards are unchanged. Manual review also
finds that the bare label “移动版发布日期” in Open questions is falsely rejected by the old uncertainty-keyword
check: it asserts no fixed date. That automated failure remains recorded explicitly; keyword counts alone do
not establish semantic acceptance. The 9B run also invents questions about unspecified owners, so no model
recommendation or default was changed. These are Linux CPU checks, not Mac performance measurements.

CI now runs summary acceptance before the longer board suites and uploads `summary-report` immediately,
including the model hash, outputs and failed checks. A summary failure still fails the job; both board suites
continue if the model server started and the run was not cancelled. The final combined artifact and all ASR
jobs remain. This avoids losing every summary result when a later code push cancels a run during board eval.
The workflow passes actionlint and condition checks; live results are tracked in [PR #206](https://github.com/HsiangNianian/boardcast/pull/206).

## Board references and evidence-selection probes

Board IDs restart on each board. Cleanup could turn `第 2 张板的 n1（计费）已经完成` into
`第 2 张板的 登录（计费）已经完成` by taking the first board's `n1` label. It now replaces an ID only
when every matching node or group has the same label. Conflicting references stay as written; cleanup does not
infer which board a free-text reference means. A regression fails before the fix and passes afterward, including
reversed board order and a shared ID whose labels agree. All 49 agent and 13 tools tests pass.

The [frozen follow-up](summary-grounding-2026-11-12.json) includes two rejected probes. Selecting verbatim source
excerpts preserves dates but produces incomplete tasks, invents or loses template topics, and asks an already
answered duration question again: **3/8**, after strengthening a check that previously scored it 4/8. A separate
1024-token thinking-budget probe preserves both missing dates but leaves the bare item `Two failure criteria`,
losing the uncertainty in the number: **2/3**, where the older checks incorrectly scored 3/3. Those three calls
took 108–122 seconds on the local CPU. Neither approach was added to production.

Checks now reject reopening the confirmed cache-duration question and reject that unqualified quantity item,
while accepting questions such as `Should there be two failure criteria?`. The original source utterances and
boards are unchanged; previous reports and original scores are retained. The final production short run remains
**12/15**, with both template cases passing. All 26 captured calls fit the measured context budget and stop normally.
The existing long-transcript **0/2** remains unresolved; it was not rerun for this cleanup-only production change.

## Output handling and longer transcripts

The latest follow-up rejects a nonempty reply with `finish_reason: length` instead of merging incomplete notes
or saving a cut-off summary. Empty reasoning-only replies retain their existing bounded retry. A generation
error now stays in the evaluation's JSON and Markdown report, with a failed command status. The app's existing
error path keeps the prior summary. This detects a failed generation; it does not recover the missing text.

Model-produced list items under To-dos now receive checkboxes when omitted, retaining text, indentation and
existing completion state. `None`/`无` placeholders and other sections do not become tasks. Empty main headings
also receive their placeholder when no meeting template is used. These fixes passed 48 agent and 13 tools
tests, the UI compile check, and a fresh model control containing Nora's Wednesday task.

The final short suite remains **12/15**. Two new [long-transcript cases](summary-long-transcript-cases.json),
split into four Chinese and seven English fragments, are **0/2**. The Chinese result places the agreed Thursday
launch only in Conclusions. English loses the completed migration and Sam's Tuesday task, and invents unresolved
owners. Run them separately from the default short CI suite:

```sh
cargo run --release -p boardcast-tools -- summary-eval . \
  --cases eval/summary-long-transcript-cases.json --llm http://127.0.0.1:18765 --label long-transcript
```

[Frozen reports, raw replies, source hashes and rejected probes](summary-output-2026-11-12.json) record two
attempts to make partial notes more faithful. One emitted cut-off lists; the other lost completed facts and
downgraded confirmed ten-minute retention to tentative. Both were reverted. A separate four-case self-review
probe passed only **1/4**, introducing a false Chinese two-item assertion; no second model pass was added.
The Chinese quantity checker now catches that observed wording. The longer checks were also strengthened after
manual review; source utterances were unchanged, and both frozen baseline and candidate score **0/2** when rescored.
The final production prompts and sampler are unchanged from `f356e5a`.

All 110 captured requests fit the measured input/output budget; four responses in the rejected experiment still
hit their output limit. Context fit alone does not prove that a response finished. The final source made 29 model
calls with normal stop reasons, including the fifteen short cases, two long cases and checkbox control. These
synthetic tests do not establish real-meeting reliability; #68 remains open.

## Large local summaries (2026-11-11)

Twelve boards with forty checks each reproduced a real local-server error: **10,891 prompt tokens exceeded
the 4,096-token context**, even with the old 1,500-character cutoff on every board. That cutoff also silently
discarded board tails. The draft now measures the full rendered prompt with the running model's tokenizer,
reserves reply space and reduces oversized source or accumulated notes in stages. It retains source order and
complete lines where possible, rejects a reduction that does not shorten the input, checks cancellation between
calls and bounds the empty-reply retry. A short original transcript stays verbatim when capacity permits.
Saved source material and fingerprints are unchanged.

The separate [context suite](summary-context-cases.json) has three large-board cases and a short English control:

```sh
cargo run --release -p boardcast-tools -- summary-eval . \
  --cases eval/summary-context-cases.json --llm http://127.0.0.1:18765 --label context
```

`--cases` is relative to the supplied root (absolute paths also work). Default CI keeps the fifteen short cases.
The final local context run is **3/4**; all three large inputs generate, but the Chinese Friday-drill case still
fails. Its last intermediate note invents unresolved next steps, and the final summary drops completed checks
and downgrades the stated Friday schedule. The corrected Thursday launch and Wednesday task are retained in
the other Chinese case; the English case preserves the conditional rollback question and Nora's Wednesday task.
These checks do not cover every defect: the Chinese correction puts completed checks under Decisions, and the
English control omits a To-do checkbox. The short suite remains **12/15**, with the same three factual failures.

[All reports, rejected intermediate attempts, raw model replies, final inputs and token checks](summary-context-2026-11-11.json)
are retained. All 83 captured model calls, including 33 on the final source, had matching preflight and actual
prompt-token counts, with prompt plus reserved output at most **3,924/4,096**. The final source passed 46 agent
tests, 12 tools tests and the UI compile check. These are synthetic board stress tests, not real long-meeting
acceptance; **#68 remains open**. No new model or sampler was adopted.

A separate [fact/decision/task prompt probe](summary-rejected-categories-2026-11-11.json) was rejected at 12/15.
The three baseline raw replies confirm that the missing dates and unconfirmed quantity originate in the model
reply, before cleanup. The production final-summary rules remain unchanged; only the partial-note instructions
add limits on inferring unseen context, group numbers and totals.

## Measured result

The latest completed CI text job, on the expanded 15-case suite at `caa1600`, is **36/45**. The Chinese Wednesday
deployment, English unconfirmed quantity and Chinese Monday rehearsal each fail all three repetitions. Both
template cases pass 6/6. [All 45 outputs and hashes](summary-ci-expanded-2026-11-11.json) are retained from
[the earlier run](https://github.com/HsiangNianian/boardcast/actions/runs/43036127338); the artifact also records
[the follow-up](https://github.com/HsiangNianian/boardcast/actions/runs/43042362889) and
[the context-budget run](https://github.com/HsiangNianian/boardcast/actions/runs/43051546186). Those three runs have
identical outputs and failures, with separate durations and report/source hashes. The
[output-handling follow-up](https://github.com/HsiangNianian/boardcast/actions/runs/43060105257) retains another
45 outputs: eighteen differ, including filled empty sections and several model wordings, with the same failed cases. This does not imply
that the whole Eval workflow passed.
The original twelve cases score 30/36 and the new three 6/9; the earlier English CI pass was not stable.
The latest source hashes reflect the output-handling change; its short inputs use the same final prompts.
Model hashes match the frozen local model, but identical runtime conditions are not established. The first three runs
precede the output-handling and checker follow-up above. The historical English answered-question output leaves
empty headings instead of `None`; that presentation defect is fixed above without changing its facts. The score
checks semantics, not every presentation rule. #68 remains open.

Twelve cases, three runs per model, using the production source before the cleanup follow-up below. Full outputs, checks and provenance are in
[`summary-2026-11-11.json`](summary-2026-11-11.json). The initial keyword checks reported 36/36 for both models;
manual inspection found two omissions in those checks. The same frozen outputs were rescored against the stronger
checks below without regenerating the summaries or changing their case inputs. The **30/36** result is the accepted local report for
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
the production prompts remain those used for the accepted report above. [Outputs and example patches](summary-rejected-2026-11-11.json)
are retained so these local improvements are not mistaken for overall fixes.
The thinking sampler follows [Qwen's model card](https://huggingface.co/Qwen/Qwen3.5-4B); the reasoning budget
is an experimental cap, not a model recommendation.

| Probe | Passed | Reason to reject |
| --- | ---: | --- |
| Thinking enabled, 256-token reasoning budget and the recommended thinking sampler | 7/12 | Lost tasks/dates, changed uncertainty and broke a custom-template result. |
| One plain Chinese/English input-output example | 10/12 | Fixed the original two failures but lost template headings, a task owner and a decision. |
| Example with an explicit decision, used only without a template | 10/12 | Kept an answered question open and treated an agreed duration as unresolved. |

A later Chinese-only rule separated directly stated facts from agreed decisions. It retained the Wednesday deployment:
on fourteen cases repeated three times, Qwen scored **39/42** and DeepSeek **42/42 after a check correction**. The
new mixed-date check initially rejected DeepSeek's correct statement that the mobile date was undetermined and Friday
should not be scheduled. It now allows that negation in Conclusions while requiring uncertainty in Open questions;
the original **41/42** score and all original failures are retained. Inputs were unchanged.

**This prompt was rejected.** An additional explicit conditional question with “小林周五前整理测试报告” produced an empty
To-dos section, putting the task only in Conclusions. The original production prompt retained the task in To-dos.
[The complete outputs, rule change, checks, hashes and before/after counterexample](summary-rejected-stated-facts-2026-11-11.json)
are preserved. Neither language's production prompt changed, and the candidate's scores are not current product scores.
The original prompt then ran the three new cases three times: **6/9**, with all three corrected-date outputs omitting
“一” from the Monday rehearsal (“周只做预演”). The corrected Tuesday deployment survives. This new failure remains open
alongside the original Wednesday omission and English quantity presupposition; the other two new cases passed 3/3 each.

Two three-case sampler screening probes also failed to preserve the dates: removing only the repetition penalty
passed **0/3**; using Qwen's recommended non-thinking sampler passed **1/3**, improving only that English quantity
output. Both still omitted Wednesday and the Monday rehearsal date. [All six outputs, inputs, source hashes and
sampler changes](summary-rejected-samplers-2026-11-11.json) are retained. Neither warranted a full-suite rerun;
the production sampler and its repetition guard remain unchanged. These screening scores are not product scores.

## CI result and empty-topic follow-up

At `0c89941`, [Eval](https://github.com/HsiangNianian/boardcast/actions/runs/43019666812) reached the summary
checks and failed at **27/36**. [The original CI outputs](summary-ci-2026-11-11.json) preserve the date omission,
the unconfirmed English quantity and a third failure: the Chinese retrospective emitted an empty `预算` topic
under Decisions in all three runs. The source hashes match the frozen local run; the CI run did not retain the
model-file hash, so these are separate observations, not a controlled comparison of identical runtimes.
The workflow now records that hash alongside its reports.

Cleanup previously removed empty template topics only from Conclusions. It now removes known template headings
containing only blank lines or `None`/`无` from all four sections, retaining real content and filling an emptied
main section. A regression covers Decisions, To-dos and Open questions, including a real task that must survive.
The 41 agent and 12 tools tests passed. Six fresh local generations (two template cases, three runs each) also
passed; [outputs and current source/model hashes](summary-template-cleanup-2026-11-11.json) are retained.
Manual inspection confirmed the discussed topics, facts, named task and deadline remain. These six runs are
template-only validation, not a new 36-case score; no prompt change addresses the two factual failures.

The full rerun at `1c1a519` then completed at **33/36**: [CI](https://github.com/HsiangNianian/boardcast/actions/runs/43026651384),
[original outputs and model hash](summary-ci-cleanup-2026-11-11.json). Both template cases passed all three runs;
manual inspection confirmed the topic facts, task owner/deadline and omitted budget topic. The English quantity
case also passed in this run, while the Chinese deployment date was omitted in all three repetitions. The model
hash matches the local frozen run, but this is not a controlled runtime comparison. The prompts and no-template
cleanup are unchanged, so the English result does not demonstrate a reliable fix for its earlier failures.
Keep the earlier failing outputs and #68 open. Four-platform Rust packaging/core checks and all ASR jobs/comparison
passed at this commit; the Eval workflow correctly remains failed on summary quality.

At `0c89941`, four-platform Rust packaging/core checks and all ASR jobs/comparison passed. The earlier
`c39109b` text run had stopped before summary model calls because the default label contains a dot;
`77007e5` fixed that validation and tests the actual CLI argument path. Build success alone does not establish
summary accuracy, and the quality check remains expected to flag the known factual failures.

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
