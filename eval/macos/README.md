# Mac capture and long-meeting acceptance

This is a test plan and a local measurement tool, not a completed Mac benchmark. Use the build under review
and record its commit/package, Mac model, RAM, OS, speech engine, board model, power source and output device.
A virtual Mac CI package check does not establish microphone permission, sustained performance or battery use.

1. Start with headphones. Test microphone, system, both, then microphone again, without restarting the app.
   On first use test Allow and Deny for each permission, then allow it in System Settings and retry. Confirm
   that the selected source changes while listening and that a failed source does not leave the other one
   silently running. Repeat fast start/stop and source changes, then quit and inspect for remaining helpers.
2. With Apple's recognizer, explicitly choose Chinese for the microphone and Japanese for system audio.
   Play a known Japanese WAV in Finder Quick Look while speaking Chinese. Reverse the languages, then use
   the same language on both. Include alternation and overlap. Sources label where audio came from, not who
   spoke. “System language” follows the OS locale; automatic spoken-language detection is still outstanding.
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
   The last 20 captures and last 128 timing samples per source are retained. No transcript, audio, API key or
   full error response is included. Actual acoustic onset → first partial, sentence end → final and screen
   presentation are **not measured**; nulls must not be reported as zero latency. This path currently emits
   final captions; timings alone do not establish complete end-to-end latency.
6. For a separately consented recording with human reference text, note missed/incorrect words, wrong source
   labels, duplicate echo and summary corrections. Keep original audio and transcript private. Verify closing
   a meeting with queued audio cannot put that audio into a newly opened meeting.

Report the two backends, languages, sources and cold/warm starts separately. Check the whole 30 minutes ran,
peak memory, whether queues grew, remaining helpers, audible dropouts, transcript/source errors and subjective
heat/noise. M1 8 GB and a larger-memory Mac are still separate #35 conditions; one M5 result cannot fill both.
Do not close #34/#69/#70/#71 from protocol tests or package CI alone. Per-source clock alignment, automatic
language detection, partial-caption timing and actual two-source Apple capacity remain to be verified/finished.

The sampler parser can be tested anywhere: `python3 eval/macos/test_watch.py`.
