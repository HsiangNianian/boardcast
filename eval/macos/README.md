# Mac capture and long-meeting acceptance

This is a test plan and a local measurement tool, not a completed Mac benchmark. Use the build under review
and record its commit/package, Mac model, RAM, OS, speech engine, board model, power source and output device.
A virtual Mac CI package check does not establish microphone permission, sustained performance or battery use.

1. Start with headphones. Test microphone, system, both, then microphone again, without restarting the app.
   On first use test Allow and Deny for each permission, then allow it in System Settings and retry. Confirm
   that the selected source changes while listening and that a failed source does not leave the other one
   silently running. Repeat fast start/stop and source changes, then quit and inspect for remaining helpers.
   After noticing stop, each Apple recognizer has one five-second window for its in-flight request and
   queued sentences; a prompt final reply is kept. An exceeded window reports unfinished transcription,
   and the helper is released before the next capture. This bounds helper waiting, not total device shutdown.
2. With Apple's recognizer, explicitly choose Chinese for the microphone and Japanese for system audio.
   Play a known Japanese WAV in Finder Quick Look while speaking Chinese. Reverse the languages, then use
   the same language on both. Include alternation and overlap. Sources label where audio came from, not who
   spoke. Then repeat with Auto on both sources, and with Auto on one and a manual language on the other.
   Confirm each reported locale matches that source and its first utterance is transcribed. Auto needs the
   multilingual speech package and fixes each source's language until restarted; it does not switch mid-capture.
   Test a very short utterance followed immediately by stop, long opening silence, and an inconclusive or unsupported
   language: uncertain detection must ask for a manual choice, not silently select the OS language. “System language”
   still follows the OS locale; existing preferences are unchanged.
3. Repeat with bundled recognition. Test each source alone before the pair. Repeat with speaker output as a
   separate echo test; do not count microphone pickup of speaker output as successful system capture.
4. Run a 30-minute meeting with the local board model active, then repeat with the remote board model as a
   separate condition. Include continuous speech, pauses, corrections and a recording-source switch. Start
   the sampler below against the **boardcast process PID** shown by Activity Monitor:

   ```sh
   python3 eval/macos/watch.py --pid 12345 --minutes 30
   ```

   The sampler observes the running app and descendants every five seconds. It never starts recording.
   Results stay in ignored `eval/reports/macos/`. RSS is a process-tree sum and can double-count shared pages;
   it is not physical memory use. Battery percentages are observations, not a calibrated power measurement.
   Temperature remains unmeasured. An interrupted run or an exited app is explicitly incomplete.
5. Before stopping and after stopping, use Settings → **Copy audio diagnostics**, save each JSON locally,
   and match capture IDs to the source-switch sequence. Each source retains received samples, latest arrival,
   level, maximum/current backlog and bounded queue/ASR/UI-receipt timings. Inspect counts as well as p50/p95/max.
   Each source has its own recognition worker and a queue capped at 300 packets (`queued_packets_per_track`). The last 20 captures and last
   128 timing samples per source are retained. No transcript, audio, API key or
   full error response is included. Actual acoustic onset → first partial, sentence end → final and screen
   presentation are **not measured**; nulls must not be reported as zero latency. This path currently emits
   final captions; timings alone do not establish complete end-to-end latency. The first Auto ASR call includes
   language detection and native helper startup, so keep it separate from warmed recognition samples.
6. For a separately consented recording with human reference text, note missed/incorrect words, wrong source
   labels, duplicate echo and summary corrections. Keep original audio and transcript private. Verify closing
   a meeting with queued audio cannot put that audio into a newly opened meeting.

Report the two backends, languages, sources and cold/warm starts separately. Check the whole 30 minutes ran,
peak memory, whether queues grew, remaining helpers, audible dropouts, transcript/source errors and subjective
heat/noise. M1 8 GB and a larger-memory Mac are still separate #35 conditions; one M5 result cannot fill both.
Do not close #34/#69/#70/#71 from protocol tests or package CI alone. Per-source clock alignment, partial-caption
timing and actual two-source Apple capacity remain to be verified/finished; Auto's physical Mac path is unverified.

The sampler parser can be tested anywhere: `python3 eval/macos/test_watch.py`.

The two-source scheduling regression uses the verified `en-ami-00.wav` fixture, real VAD and two stand-in
speech helpers. Each helper waits for the other to receive a sentence: shared serial consumption fails this
check, while separate workers complete it. A second run injects a recognition error and checks both helpers
shut down through EOF. The blocking case failed on the serial implementation and passed with separate workers.
The Linux package job downloads three language fixtures, verifies their manifest sizes/hashes and runs this
test explicitly. It covers manual selection, microphone Auto with manual system language, and a failure:

```sh
BOARDCAST_VOICE_DIR="$PWD/resources/voice" \
BOARDCAST_AUDIO_TEST_WAV="$PWD/eval/asr/.data/en-ami-00.wav" \
cargo test -p boardcast-ui both_sources_can_recognize_while_the_other_is_waiting -- --ignored
```

This exercises the production capture-job lifecycle with WAV input; it does not open devices or measure Apple
recognition. Both tracks use the same English recording, so it is not a bilingual accuracy or clock-alignment test.

Auto reuses the Tauri probe of SenseVoice's [language/event metadata](https://github.com/k2-fsa/sherpa-onnx/blob/v2.0.1/sherpa-onnx/csrc/offline-recognizer-sense-voice-impl.h#L52-L58), accepting only nonempty speech tagged Chinese,
Japanese or English. The current ear supplies the first VAD-segmented sentence; up to five seconds are probed,
then the probe is released before starting Apple's selected locale. The full original sentence is sent once,
including audio beyond the probe window. The ear retains its source-relative timestamp and normal bounded buffers;
the probe adds no independent silence buffer. Short or mixed speech can select the wrong language, so manual
selection remains available. Missing models or inconclusive detection fail explicitly.

A separate regression runs verified Chinese, Japanese and English WAVs through three concurrent Auto instances
and simulated Apple helpers. It checks the actual selected locales, byte-for-byte forwarding of the complete
opening PCM and the next sentence, probe release, an opening sentence finalized after stop, and 40 seconds
of silence through the production ear without starting Apple or exceeding its audio retention bound:

```sh
BOARDCAST_VOICE_DIR="$PWD/resources/voice" \
BOARDCAST_AUTO_AUDIO="$PWD/eval/asr/.data" \
cargo test -p boardcast-audio auto_routes_real_opening_speech_and_preserves_the_pcm -- --ignored
```

That test runs real language detection, not Apple recognition. Native asset preparation, permissions, overlapping
source performance and bilingual transcription accuracy still require the Mac procedure above.

On 2026-11-11, local Linux verification passed 21 audio tests, 5 UI tests, the explicit three-language Auto test
and the extended two-source lifecycle test. The English settings UI was also exercised in an isolated Linux app:
an existing Japanese microphone / System language pair survived loading; choosing microphone Auto left system
audio unchanged; choosing Auto for both persisted through a full app close and reopen. A presence-only Apple
helper stub enabled these controls; no recording device or native Apple recognizer was opened.

The helper stop regression also checks a reply that returns promptly after stop, an in-flight helper that hangs,
and another queued sentence after the finalization deadline. The hanging-helper case failed on the earlier code
after waiting for its 12-second simulated hang; it now fails explicitly within the stop budget, without granting
each subsequent sentence another five seconds. Auto startup and transcription after stop share the same deadline;
a startup delay does not buy another five seconds. These are protocol/lifecycle checks, not physical latency figures.
