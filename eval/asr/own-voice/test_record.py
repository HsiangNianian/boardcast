"""Exercise the interactive recorder with a fake capture process; no microphone is opened."""
import json
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import tempfile
import time
import unittest


class Recorder(unittest.TestCase):
    def record(self, platform, fail=False):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            shutil.copyfile(Path(__file__).with_name('record.sh'), root / 'record.sh')
            (root / 'sentences.json').write_text('[{"id":"zh-01","text":"一句话"}]')
            (root / 'recordings').mkdir()
            previous = root / 'recordings/zh-01.wav'
            previous.write_bytes(b'previous take')
            tools = root / 'bin'
            tools.mkdir()
            (tools / 'uname').write_text(f'#!/bin/sh\nprintf "{platform}\\n"\n')
            (tools / 'ffmpeg').write_text('''#!/usr/bin/env python3
import json, os, signal, sys, time, wave
open('args.json', 'w').write(json.dumps(sys.argv[1:]))
if os.environ['FAKE_FAIL'] == '1':
    print('fake capture ready', flush=True)
    sys.exit(1)
def stop(*_):
    with wave.open(sys.argv[-1], 'wb') as f:
        f.setnchannels(1); f.setsampwidth(2); f.setframerate(16000)
        f.writeframes(b'\\0\\0' * 1600)
    sys.exit(0)
signal.signal(signal.SIGINT, stop)
print('fake capture ready', flush=True)
while True: time.sleep(0.05)
''')
            for tool in tools.iterdir():
                tool.chmod(0o755)
            pid, terminal = pty.fork()
            if pid == 0:
                os.chdir(root)
                os.environ['PATH'] = str(tools) + os.pathsep + os.environ['PATH']
                os.environ['FAKE_FAIL'] = '1' if fail else '0'
                os.execvp('sh', ['sh', 'record.sh', 'zh-01'])
            output = b''
            sent = set()
            status = None
            ended = 0
            deadline = time.monotonic() + 10
            try:
                while time.monotonic() < deadline:
                    if select.select([terminal], [], [], 0.05)[0]:
                        try:
                            output += os.read(terminal, 65536)
                        except OSError:
                            pass
                    for marker in [b'Enter to start', b'fake capture ready']:
                        if marker in output and marker not in sent:
                            os.write(terminal, b'\n')
                            sent.add(marker)
                    ended, status = os.waitpid(pid, os.WNOHANG)
                    if ended:
                        break
                else:
                    self.fail(f'recorder did not exit: {output!r}')
            finally:
                os.close(terminal)
                if status is None or not ended:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
            args = json.loads((root / 'args.json').read_text())
            self.assertEqual(args[args.index('-f') + 1], 'avfoundation' if platform == 'Darwin' else 'pulse')
            self.assertEqual(args[args.index('-i') + 1], 'none:default' if platform == 'Darwin' else 'default')
            self.assertEqual(os.waitstatus_to_exitcode(status) == 0, not fail, output.decode(errors='replace'))
            self.assertEqual(previous.read_bytes() == b'previous take', fail)
            self.assertFalse(list((root / 'recordings').glob('.*.part.wav')))

    def test_linux(self):
        self.record('Linux')

    def test_mac(self):
        self.record('Darwin')

    def test_failed_retake_keeps_previous_recording(self):
        self.record('Darwin', fail=True)


if __name__ == '__main__':
    unittest.main()
