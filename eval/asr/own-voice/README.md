# Your own voice

The shared corpus is other people's recordings. This set is 30 sentences read by whoever uses boardcast,
on their own microphone: 20 in Chinese with the English terms a technical meeting carries, including
board commands, and 10 in English. It measures recognition where it matters, and compares engines on
the same audio.

```sh
sh eval/asr/own-voice/record.sh        # about ten minutes; stops and resumes at any sentence
```

Recordings go to `recordings/`, which git ignores, so a voice never leaves the machine. `sentences.json`
is the reference text. Chinese is scored by character, English by word, after removing punctuation and case.
