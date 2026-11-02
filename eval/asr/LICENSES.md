# ASR fixture attribution and licenses

The application code is MIT. These third-party recordings and transcripts retain their own licenses; they are **not relicensed as MIT**. The `asr-fixtures-v1` Release contains the files listed in `corpus.json`. The `asr-meetings-v1` Release contains only the three AISHELL-4 crops and AMI crop from `meetings.json`, plus that manifest and this notice. The `asr-meetings-v2` manifest reuses those unchanged v1 WAV assets and canonicalizes the locally converted MP3 WAV headers; no v1 assets are replaced. Earnings-22 and Japanese ministry audio are downloaded from their upstream hosts, never mirrored in our Release.

## AISHELL-4 — CC BY-SA 4.0

Credit: Yihui Fu, Luyao Cheng, Shubo Lv, Yukai Jv, Yuxiang Kong, Zhuo Chen, Yanxin Hu, Lei Xie, Jian Wu, Hui Bu, Xin Xu, Jun Du, Jingdong Chen; Beijing Shell Shell Technology, *AISHELL-4* (2021).

- Publisher and original data license: <https://www.openslr.org/111/>
- Mirror: <https://huggingface.co/datasets/AISHELL/AISHELL-4/tree/aada72727856313b19d4a030383c426364931dbf/test>. The mirror card's Apache-2.0 label does **not** replace the original data's CC BY-SA 4.0 terms.
- Recordings: `L_R003S01C02`, `M_R003S01C01`, `S_R003S03C01`; recorded scenario meetings on property management, kindergarten education and factory safety.
- Changes by boardcast contributors: continuous crops at the manifest's exact intervals, microphone channel 0, 16 kHz PCM16, no enhancement. Parse official TextGrid speaker intervals, remove nonlexical angle-bracket tags and ampersand markup, retain spoken fillers. Adapted audio and references remain CC BY-SA 4.0.

## Earnings-22 — text CC BY-SA 4.0; upstream audio only

Credit: Rev; Del Rio et al., *Earnings-22: A Practical Benchmark for Accents in the Wild* (2022).

- Publisher: <https://github.com/revdotcom/speech-datasets/tree/c05ab6fd8b4b627d123c922a22a39e993dd37635/earnings22>
- License: <https://github.com/revdotcom/speech-datasets/blob/c05ab6fd8b4b627d123c922a22a39e993dd37635/earnings22/LICENSE.md>. Its grant names transcripts and associated text files; we do not extend that grant to the audio.
- Recordings: `4462231` (Texas Instruments), `4474506` (Costco), `4485244` (KKR), `4483296` (Imperial Oil).
- Text changes: select continuous intervals from the publisher's forced-aligned manual references, retain unaligned lexical words, omit nonlexical annotation tokens, group adjacent words by speaker and shift times to clip-relative seconds. These adapted references remain CC BY-SA 4.0. The spoken-form normalizations are publisher supplied; no tested recognizer generates the reference.
- Audio is fetched directly from the pinned upstream MP3 URL and cropped/resampled locally. Neither original MP3s nor derived WAVs are uploaded to our Release. Local availability does not establish a separate redistribution grant.

## Japanese public-health committee — official public data terms

Credit/source: Japan Ministry of the Environment, FY2022 fifth asbestos health damage relief subcommittee, held online on 2023-03-31.

- Recording page: <https://www.env.go.jp/council/05hoken/page_00047.html>
- Official minutes: <https://www.env.go.jp/council/05hoken/page_00049.html>
- Website reuse terms: <https://www.env.go.jp/mail.html> (Public Data License 1.0, with the stated exceptions).
- Adaptation by boardcast contributors: the first 609 seconds of audio; corresponding opening paragraphs of the official minutes through the paragraph on the Supreme Court judgment's treatment of outdoor construction workers. Remove speaker labels and the nonspeech marker `（なし）`; do not rewrite the text into recognizer output. These are **edited minutes**, not a verbatim gold transcript.
- Audio is fetched from the ministry and converted locally; no ministry audio is mirrored in our Release. The publication page describes temporary audio availability pending minutes publication, so continued hosting is not guaranteed. An unavailable or changed source fails verification.
- These excerpts and conversions are adaptations by boardcast contributors, not official ministry-produced evaluation data. No endorsement is implied.

## AliMeeting — CC BY-SA 4.0

Credit: Alibaba Group; Fan Yu, Shiliang Zhang, Yihui Fu, et al., *M2MeT: The ICASSP 2022 Multi-Channel Multi-Party Meeting Transcription Challenge* (2022).

- Publisher and original license: <https://www.openslr.org/119/>
- License: <https://creativecommons.org/licenses/by-sa/4.0/>
- Original audio: `Eval_Ali/Eval_Ali_far/audio_dir/R8001_M8004_MS801.wav` in the publisher's `Eval_Ali.tar.gz`.
- Changes: select microphone channel 0 from the original 16 kHz recording; crop the intervals recorded in the manifest; keep PCM16. No denoising, normalization or resampling.
- The reference text and coarse block times were obtained from the community derivative <https://huggingface.co/datasets/playwithmino/alimeeting-eval-8k/blob/main/manifests/eval_all.jsonl>. Its 8 kHz audio is not used. The text has **not** been checked against the official TextGrid and remains provisional.
- Redistributed audio crops and adapted reference text are supplied under CC BY-SA 4.0.

## AMI Meeting Corpus — CC BY 4.0

Credit: the AMI Consortium, *AMI Meeting Corpus*, meeting IB4001; official manual annotations v1.6.2.

- Publisher: <https://groups.inf.ed.ac.uk/ami/download/>
- License: <https://creativecommons.org/licenses/by/4.0/>
- Original audio: <https://groups.inf.ed.ac.uk/ami/AMICorpusMirror/amicorpus/IB4001/audio/IB4001.Mix-Headset.wav>
- Original annotations: <https://groups.inf.ed.ac.uk/ami/AMICorpusAnnotations/ami_public_manual_1.6.2.zip>
- Changes: crop the publisher's mixed-headset PCM16 recording; convert XML speaker segments to JSON; join punctuation for display. Original words, including fillers and truncated words, are retained. Exact source times are in the manifest.

## FLEURS Japanese — CC BY 4.0

Credit: Alexis Conneau, Min Ma, Simran Khanuja, et al., *FLEURS: Few-shot Learning Evaluation of Universal Representations of Speech* (2022), Google.

- Publisher: <https://huggingface.co/datasets/google/fleurs>
- Paper: <https://arxiv.org/abs/2205.12446>
- License: <https://creativecommons.org/licenses/by/4.0/>
- Subset: `ja_jp`, `test`; the first 12 recordings in the pinned Parquet revision specified in `corpus.json`.
- Changes: convert publisher WAVs to mono 16 kHz PCM16, keeping each recording separate. Reference text is `raw_transcription`. These are **human read-speech samples, not meeting recordings**. They are a small regression set, not a representative Japanese benchmark.

## ASCEND — CC BY-SA 4.0

Credit: Holly Lovenia, Samuel Cahyawijaya, Genta Indra Winata, et al., HKUST CAiRE, *ASCEND: A Spontaneous Chinese-English Dataset for Code-switching in Multi-turn Conversation* (2021).

- Publisher: <https://huggingface.co/datasets/CAiRE/ASCEND>
- Paper: <https://arxiv.org/abs/2112.06223>
- License: <https://creativecommons.org/licenses/by-sa/4.0/>
- Eight selected `test` clips from session 1; original file names and row IDs are recorded in the manifest. Audio and publisher transcripts are unchanged. All clips are from one session and are short probes, not broad code-switching coverage.

## Silence control

Generated by boardcast contributors: two seconds of zero-valued PCM16 samples, released under the project's MIT license. It tests hallucinations on empty input and is not a speech recording.

## Excluded material

The previously downloaded MagicData Japanese conversation is **not** included in this manifest, Release, CI cache, generated listening page or reports. Its contradictory redistribution/noncommercial terms remain unresolved. The previous IETF recording is also outside this versioned data pack.

Please retain this notice and each applicable license when redistributing these files, and describe any further modifications. No endorsement by the dataset authors is implied.
