"""python3 scripts/office-plan-demo.py <meeting.json> eval/asr/corpus.json assets/demos/en-office-plan.json

Builds the web demo's recorded sample from a meeting the app drew while the `en-ami` recording was fed to it
(AMI IB4001, 2:50 to 8:02), and that recording's manual transcript in `corpus.json` for the times.

- The meeting's first 19 lines were a microphone test before the recording, and are left out; its node ids are
  renumbered to start from n1, as the sample starts from an empty board.
- Lines added by hand and a line the meeting has twice are dropped; the removal made by hand at the end
  goes with the last line said, so the board ends as it did.
- A turn longer than ten seconds is cut at its sentence ends, as the app would hear it, timed by length, and
  each op of its reply goes to the first part naming what the op writes.

The audio is the same recording, re-encoded:
  ffmpeg -i eval/asr/.data/en-ami.wav -ac 1 -ar 22050 -codec:a libmp3lame -b:a 40k landing/public/media/en-office-plan.mp3
(`asr-eval download . --suite quick` fetches the WAV.) The generated files are committed; builds need neither.
"""
import json, re, sys

meeting, corpus, out_path = sys.argv[1:4]
p = json.load(open(meeting))
ref = [x for x in json.load(open(corpus))['samples'] if x['id'] == 'en-ami'][0]['reference']
norm = lambda s: re.sub(r'[^a-z0-9]', '', s.lower())
renum = lambda ops: re.sub(r'\bn(\d+)\b', lambda m: 'n%d' % (int(m.group(1)) - 8), ops) if ops else ops
LONG = 10.0

def split(line):
    """A long turn as the app would hear it: cut at sentence ends, timed by length, each reply op
    put on the first part that names what it writes."""
    parts = [s for s in re.split(r'(?<=[.?!])\s+', line['text']) if s.strip()]
    if line['end'] - line['start'] <= LONG or len(parts) < 2:
        return [line]
    total = sum(len(s) for s in parts)
    t, chunks = line['start'], []
    for s in parts:
        d = (line['end'] - line['start']) * len(s) / total
        chunks.append({'speaker': line['speaker'], 'text': s, 'ms': line['ms'], 'ops': [], 'start': round(t, 3), 'end': round(t + d, 3)})
        t += d
    at = 0
    for op in (line['ops'] or '').split('\n'):
        if not op:
            continue
        # What the op writes is its last label; an earlier one names where it goes.
        labels = re.findall('「([^」]*)」', op)
        words = re.findall(r'[a-z0-9_]{3,}', labels[-1].lower()) if labels else []
        hit = next((k for k in range(at, len(chunks)) if any(w in chunks[k]['text'].lower() for w in words)), None) if words else None
        at = hit if hit is not None else at
        chunks[at]['ops'].append(op)
    for c in chunks:
        c['ops'] = '\n'.join(c['ops']) or None
    return chunks

lines, ri = [], 0
for l in p['lines'][19:83]:
    if l.get('manual'):
        continue
    j = next((k for k in range(ri, len(ref)) if norm(ref[k]['text']) == norm(l['text'])), None)
    if j is None:  # the same line said twice in the meeting file
        continue
    ri = j + 1
    ops = None if l.get('ops') in (None, '-') else renum(l['ops'])
    lines.append({'speaker': l['speaker'], 'text': l['text'], 'ms': round(l.get('ms', 800)), 'ops': ops, 'start': round(ref[j]['start'], 3), 'end': round(ref[j]['end'], 3)})
# The removal made by hand at the end ("del it") goes with the last line said.
last = lines[-1]
last['ops'] = (last['ops'] + '\n' if last['ops'] else '') + renum(p['lines'][83]['ops'])
lines = [c for l in lines for c in split(l)]
sample = {
    'id': 'en-office-plan',
    'name': 'A real research-group meeting planning who sits where, from the AMI Meeting Corpus (IB4001), with its recording',
    'lang': 'en',
    'audio': 'en-office-plan.mp3',
    'credit': 'Recording and transcript: AMI Meeting Corpus, meeting IB4001, CC BY 4.0',
    'lines': lines,
}
with open(out_path, 'w') as out:
    json.dump(sample, out, ensure_ascii=False, indent=1)
    out.write('\n')
print(len(lines), 'lines;', sum(1 for l in lines if l['ops']), 'with replies; first reply at', next(l['end'] for l in lines if l['ops']))
