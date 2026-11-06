#!/usr/bin/env python3
"""Launch only a source-identified, disposable G2 build on an independent D-Bus."""
import argparse, ast, hashlib, json, os, pathlib, subprocess
parser = argparse.ArgumentParser()
parser.add_argument('root', type=pathlib.Path)
parser.add_argument('--instance', default='a')
args = parser.parse_args()
root = args.root.resolve()
if not root.name.startswith('boardcast-g2-') or not (root / 'source-hashes.json').is_file():
    raise SystemExit('Expected an isolated run with source-hashes.json')
if not args.instance.isalnum():
    raise SystemExit('Instance name must be alphanumeric')
config, cache = root / ('config-' + args.instance), root / ('cache-' + args.instance)
config.mkdir(exist_ok=True); cache.mkdir(exist_ok=True)
env = os.environ.copy()
env.update(XDG_DATA_HOME=str(root / 'data'), XDG_CONFIG_HOME=str(config), XDG_CACHE_HOME=str(cache), GDK_BACKEND='x11', LANG='en_US.UTF-8', LANGUAGE='en_US:en')
env.pop('BOARDCAST_MODEL', None)
address = subprocess.run(['gdbus', 'call', '--session', '--dest', 'org.a11y.Bus', '--object-path', '/org/a11y/bus', '--method', 'org.a11y.Bus.GetAddress'], capture_output=True, text=True)
if address.returncode == 0:
    env['AT_SPI_BUS_ADDRESS'] = ast.literal_eval(address.stdout)[0]
binary = root / 'target/debug/boardcast'
log = (root / ('app-' + args.instance + '.log')).open('w')
process = subprocess.Popen(['dbus-run-session', '--', str(binary)], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
(root / ('launch-' + args.instance + '.json')).write_text(json.dumps({'dbusWrapperPid': process.pid, 'root': str(root), 'binary': str(binary), 'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'instance': args.instance}, indent=2))
print('isolated launch', args.instance, process.pid, flush=True)
raise SystemExit(process.wait())
