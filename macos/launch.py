"""Open IndyMAT from its macOS application without opening Terminal."""
from pathlib import Path
import json
import os
import subprocess
import sys
import time
import urllib.request
from urllib.parse import urlsplit


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


def main():
    config = json.loads(Path(__file__).with_name('launcher.json').read_text())
    root = Path(config['project_root'])
    if not (root / 'app.py').is_file():
        raise RuntimeError(f'Proje klasörü bulunamadı: {root}. Uygulamayı yeniden kurun.')
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
                [sys.executable, str(root / 'app.py'), '--no-browser'],
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
    if '--print-url' in sys.argv:
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
