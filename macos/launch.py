"""Open IndyMAT from its macOS application without opening Terminal."""
from pathlib import Path
import argparse
import fcntl
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request
from urllib.parse import urlsplit


ENGINES = ('octave', 'rust')  # As in app.py: the first is the default.


def running_url(root):
    try:
        launch = json.loads((root / '.matlab-free' / 'launch.json').read_text())
        url = launch['url']
        parsed = urlsplit(url)
        if (parsed.scheme != 'http' or parsed.hostname != '127.0.0.1'
                or not parsed.port or not parsed.fragment):
            return None
        req = urllib.request.Request(
            f'http://127.0.0.1:{parsed.port}/api/state',
            headers={'X-MF-Token': parsed.fragment})
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with opener.open(req, timeout=2) as response:
            state = json.load(response)
        if 'status' in state:
            return url
    except (OSError, ValueError, KeyError, TypeError):
        pass
    return None


def server_holds_lock(root):
    # A server still loading has taken app.py's lock but does not answer yet.
    try:
        with (root / '.matlab-free' / 'server.lock').open() as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return True
    except OSError:
        pass
    return False


def rust_available(root):
    # The rules of kernel_executable() in backend/kernel_rust.py. This script runs before the
    # project is on the path, so it cannot import them; tests/test_launch_engine.py pins the two.
    explicit = os.environ.get('INDYMAT_R_KERNEL')
    if explicit:
        return bool(shutil.which(explicit))
    for profile in ('release', 'debug'):
        candidate = root / 'motor' / 'target' / profile / 'indy-kernel'
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return True
    return False


def saved_engine(root):
    # The rules of App._saved_engine() in app.py, also pinned by the test.
    try:
        saved = json.loads((root / '.matlab-free' / 'engine.json').read_text()).get('engine')
    except (OSError, ValueError, AttributeError):
        return ENGINES[0]
    return saved if saved in ENGINES else ENGINES[0]


def status(root):
    # Starts nothing and never carries the URL: its token stays out of anything but --print-url.
    rust = rust_available(root)
    engine = saved_engine(root)
    return {'running': bool(running_url(root)) or server_holds_lock(root),
            'engine': engine if rust or engine != 'rust' else ENGINES[0],
            'rust_available': rust}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--print-url', action='store_true')
    parser.add_argument('--status', action='store_true')
    parser.add_argument('--engine', choices=ENGINES)
    args = parser.parse_args()
    config = json.loads(Path(__file__).with_name('launcher.json').read_text())
    root = Path(config['project_root'])
    if not (root / 'app.py').is_file():
        raise RuntimeError(f'Proje klasörü bulunamadı: {root}. Uygulamayı yeniden kurun.')
    if args.status:
        print(json.dumps(status(root)))
        return
    url = running_url(root)
    if not url:
        runtime = root / '.matlab-free'
        runtime.mkdir(mode=0o700, exist_ok=True)
        runtime.chmod(0o700)
        env = os.environ.copy()
        env['PATH'] = '/opt/homebrew/bin:/usr/local/bin:' + env.get('PATH', '/usr/bin:/bin')
        log_path = runtime / 'launcher.log'
        log_fd = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        with os.fdopen(log_fd, 'ab', buffering=0) as log:
            child = subprocess.Popen(
                # Only a server started here takes the choice; a running one keeps its engine.
                [sys.executable, str(root / 'app.py'), '--no-browser']
                + (['--engine', args.engine] if args.engine else []),
                cwd=root, env=env, stdin=subprocess.DEVNULL,
                stdout=log, stderr=log, start_new_session=True,
                close_fds=True)
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            url = running_url(root)
            if url:
                break
            if child.poll() not in (None, 0):
                break
            time.sleep(.2)
        if not url:
            raise RuntimeError(f'IndyMAT başlatılamadı. Ayrıntılar: {log_path}')
    if args.print_url:
        # Private pipe to the native window; never write this URL to a log.
        print(url)
    else:
        subprocess.run(['/usr/bin/open', url], check=True)


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
