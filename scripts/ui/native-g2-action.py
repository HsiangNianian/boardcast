#!/usr/bin/env python3
"""Observed-token Cua actions against an explicitly identified disposable G2 app."""
import argparse, json, os, pathlib, subprocess

p = argparse.ArgumentParser()
p.add_argument('root', type=pathlib.Path)
p.add_argument('pid', type=int)
p.add_argument('window', type=int)
p.add_argument('action', choices=['snapshot', 'click', 'type', 'set', 'menu'])
p.add_argument('value', nargs='?', default='')
p.add_argument('--text', default='')
p.add_argument('--name', default='current')
p.add_argument('--session', default='boardcast-g2-final')
p.add_argument('--socket', default='cua.sock')
args = p.parse_args()
root = args.root.resolve()
if not root.name.startswith('boardcast-g2-') or os.readlink(f'/proc/{args.pid}/exe') != str(root / 'target/debug/boardcast'):
    raise SystemExit('Target is not the explicitly selected isolated G2 binary')
target = dict(pid=args.pid, window_id=args.window, session=args.session)

def call(name, payload):
    r = subprocess.run(['cua-driver', name, json.dumps(payload), '--socket', str(root / args.socket)], capture_output=True, text=True)
    if r.returncode: raise SystemExit(r.stderr or r.stdout)
    return json.loads(r.stdout)

state = call('get_window_state', {**target, 'max_elements': 230, 'timeout_ms': 5000,
    'include_screenshot': args.action == 'snapshot',
    **({'screenshot_out_file': str(root / (args.name + '.png'))} if args.action == 'snapshot' else {})})
(root / (args.name + '.json')).write_text(json.dumps(state, indent=2))
elements = state.get('elements', [])
if args.action == 'snapshot':
    for e in elements:
        if e.get('label'): print(e.get('role'), repr(e['label']), e.get('enabled', ''), e.get('element_token'))
elif args.action == 'menu':
    print(json.dumps(call('invoke_menu', {**target, 'path': args.value.split('/')})))
else:
    found = [e for e in elements if e.get('label') == args.value and (args.action in ('type', 'set') or e.get('actions'))]
    if len(found) != 1: raise SystemExit(f'Expected one observed target; found {len(found)}: {args.value}')
    payload = {**target, 'element_token': found[0]['element_token']}
    if args.action == 'type': payload.update(text=args.text, delivery_mode='foreground')
    if args.action == 'set': payload.update(value=args.text, delivery_mode='foreground')
    print(json.dumps(call({'type': 'type_text', 'set': 'set_value'}.get(args.action, 'click'), payload)))
