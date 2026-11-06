#!/usr/bin/env python3
"""A disposable Xvfb + private D-Bus + Cua endpoint for native storage checks."""
import argparse, ast, json, os, pathlib, signal, subprocess, sys, time

p = argparse.ArgumentParser()
p.add_argument('root', type=pathlib.Path)
p.add_argument('--child', action='store_true')
p.add_argument('--display')
args = p.parse_args()
root = args.root.resolve()
if not root.name.startswith('boardcast-g2-') or not (root / 'source-hashes.json').is_file():
    raise SystemExit('Expected an explicitly owned G2 directory')

if not args.child:
    display = next(n for n in range(90, 110) if not pathlib.Path(f'/tmp/.X11-unix/X{n}').exists())
    env = os.environ.copy()
    env.update(DISPLAY=f':{display}', GDK_BACKEND='x11', XDG_SESSION_TYPE='x11', XDG_CURRENT_DESKTOP='Openbox', DESKTOP_SESSION='openbox', LIBGL_ALWAYS_SOFTWARE='1', WEBKIT_DISABLE_DMABUF_RENDERER='1')
    for key in ('WAYLAND_DISPLAY', 'WAYLAND_SOCKET', 'AT_SPI_BUS_ADDRESS', 'BOARDCAST_MODEL', 'XAUTHORITY'):
        env.pop(key, None)
    server = subprocess.Popen([str(root / 'tools/usr/bin/Xvfb'), f':{display}', '-screen', '0', '1280x900x24', '-nolisten', 'tcp', '-noreset'], env=env, stdout=(root / 'xvfb.log').open('w'), stderr=subprocess.STDOUT)
    try:
        for _ in range(100):
            if pathlib.Path(f'/tmp/.X11-unix/X{display}').exists(): break
            if server.poll() is not None: raise RuntimeError('Private Xvfb exited')
            time.sleep(.05)
        subprocess.run(['dbus-run-session', '--', sys.executable, str(pathlib.Path(__file__).resolve()), str(root), '--child', '--display', f':{display}'], env=env, check=True)
    finally:
        server.terminate(); server.wait()
    raise SystemExit()

os.environ.update(DISPLAY=args.display, XDG_DATA_HOME=str(root / 'data'), XDG_CONFIG_HOME=str(root / 'config-headless'), XDG_CACHE_HOME=str(root / 'cache-headless'), LANG='en_US.UTF-8', LANGUAGE='en_US:en',
    LD_LIBRARY_PATH=str(root / 'tools/usr/lib'), XDG_DATA_DIRS=str(root / 'tools/usr/share') + ':/usr/local/share:/usr/share')
for name in ('config-headless', 'cache-headless'): (root / name).mkdir(exist_ok=True)
address = subprocess.check_output(['gdbus', 'call', '--session', '--dest', 'org.a11y.Bus', '--object-path', '/org/a11y/bus', '--method', 'org.a11y.Bus.GetAddress'], text=True)
os.environ['AT_SPI_BUS_ADDRESS'] = ast.literal_eval(address)[0]
# Register accessibility before WebKit starts; late registration can leave its tree empty.
registry = subprocess.Popen(['/usr/lib/at-spi2-registryd'], stdout=(root / 'at-spi.log').open('w'), stderr=subprocess.STDOUT)
for _ in range(100):
    ready = subprocess.run(['gdbus', 'call', '--address', os.environ['AT_SPI_BUS_ADDRESS'], '--dest', 'org.freedesktop.DBus', '--object-path', '/org/freedesktop/DBus', '--method', 'org.freedesktop.DBus.NameHasOwner', 'org.a11y.atspi.Registry'], capture_output=True, text=True)
    if ready.returncode == 0 and ready.stdout.strip() == '(true,)': break
    time.sleep(.05)
else:
    if registry.poll() is None: registry.terminate(); registry.wait()
    raise RuntimeError('Private accessibility registry did not become ready')
export = {key: os.environ[key] for key in ('DISPLAY', 'GDK_BACKEND', 'XDG_SESSION_TYPE', 'XDG_CURRENT_DESKTOP', 'DESKTOP_SESSION', 'LIBGL_ALWAYS_SOFTWARE', 'WEBKIT_DISABLE_DMABUF_RENDERER', 'DBUS_SESSION_BUS_ADDRESS', 'AT_SPI_BUS_ADDRESS', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'LD_LIBRARY_PATH', 'XDG_DATA_DIRS', 'LANG', 'LANGUAGE')}
(root / 'headless-env.json').write_text(json.dumps(export, indent=2))
# A real WM supplies the EWMH active-window identity required by Cua's input guard.
wm = subprocess.Popen([str(root / 'tools/usr/bin/openbox'), '--config-file', str(root / 'tools/etc/xdg/openbox/rc.xml')], stdout=(root / 'openbox.log').open('w'), stderr=subprocess.STDOUT)
driver = subprocess.Popen(['cua-driver', 'serve', '--socket', str(root / 'cua-headless.sock')], stdout=(root / 'cua-headless.log').open('w'), stderr=subprocess.STDOUT)
app = subprocess.Popen([str(root / 'target/debug/boardcast')], stdout=(root / 'app-headless.log').open('w'), stderr=subprocess.STDOUT)
(root / 'headless-processes.json').write_text(json.dumps(dict(driver=driver.pid, app=app.pid, registry=registry.pid, windowManager=wm.pid, display=args.display), indent=2))
print('private display ready', args.display, 'app pid', app.pid, 'driver pid', driver.pid, flush=True)
try:
    driver.wait()
finally:
    if app.poll() is None: app.terminate(); app.wait()
    if wm.poll() is None: wm.terminate(); wm.wait()
    if registry.poll() is None: registry.terminate(); registry.wait()
