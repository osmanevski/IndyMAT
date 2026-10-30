"""macos/launch.py: the engine asked at launch reaches a new server only, and its rules match the server's."""
import fcntl
import importlib.util
import json
import os
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import app as application
from backend import kernel_rust

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('indymat_launch', ROOT / 'macos' / 'launch.py')
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)

# Stands in for app.py: records its arguments and answers /api/state like a running server.
FAKE_SERVER = """
import fcntl, http.server, json, sys
from pathlib import Path
here = Path(__file__).parent
runtime = here / '.matlab-free'
runtime.mkdir(exist_ok=True)
lock = open(runtime / 'server.lock', 'w')
fcntl.flock(lock, fcntl.LOCK_EX)
with (here / 'started.jsonl').open('a') as log:
    log.write(json.dumps(sys.argv[1:]) + chr(10))
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'{"status": "ready"}')
    def log_message(self, *args):
        pass
server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
launch = {'url': f'http://127.0.0.1:{server.server_address[1]}/#secret-token', 'pid': __import__('os').getpid()}
(runtime / 'launch.json').write_text(json.dumps(launch))
server.serve_forever()
"""


def clean_environment():
    return {key: value for key, value in os.environ.items() if key != 'INDYMAT_R_KERNEL'}


class Bundle(unittest.TestCase):
    def setUp(self):
        self.folder = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.folder, True)
        self.project = self.folder / 'project'
        self.project.mkdir()
        (self.project / 'app.py').write_text(FAKE_SERVER)
        resources = self.folder / 'Resources'
        resources.mkdir()
        shutil.copy2(ROOT / 'macos' / 'launch.py', resources / 'launch.py')
        (resources / 'launcher.json').write_text(json.dumps({'project_root': str(self.project), 'python': sys.executable}))
        self.script = resources / 'launch.py'
        self.addCleanup(self.stop_server)

    def run_launch(self, *arguments):
        return subprocess.run([sys.executable, str(self.script), *arguments], capture_output=True, text=True,
                              env=clean_environment(), timeout=60)

    def started(self):
        log = self.project / 'started.jsonl'
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def stop_server(self):
        try:
            pid = json.loads((self.project / '.matlab-free' / 'launch.json').read_text())['pid']
            os.kill(pid, signal.SIGTERM)
        except (OSError, ValueError, KeyError):
            pass

    def build(self, profile='release'):
        binary = self.project / 'motor' / 'target' / profile / 'indy-kernel'
        binary.parent.mkdir(parents=True)
        binary.write_text('')
        binary.chmod(binary.stat().st_mode | stat.S_IXUSR)

    def save(self, text):
        runtime = self.project / '.matlab-free'
        runtime.mkdir(exist_ok=True)
        (runtime / 'engine.json').write_text(text)


class EnginePassing(Bundle):
    def test_a_new_server_receives_the_choice(self):
        for engine in ('rust', 'octave'):
            with self.subTest(engine=engine):
                result = self.run_launch('--print-url', '--engine', engine)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('secret-token', result.stdout)
                self.assertEqual(self.started()[-1], ['--no-browser', '--engine', engine])
                self.stop_server()
                self.wait_until_free()

    def wait_until_free(self):
        for _ in range(100):
            if not launch.server_holds_lock(self.project):
                (self.project / '.matlab-free' / 'launch.json').unlink(missing_ok=True)
                return
            time.sleep(.05)
        self.fail('the stand-in server did not stop')

    def test_no_choice_passes_no_engine(self):
        self.assertEqual(self.run_launch('--print-url').returncode, 0)
        self.assertEqual(self.started(), [['--no-browser']])

    def test_a_running_server_is_attached_without_an_engine(self):
        first = self.run_launch('--print-url')
        self.assertEqual(first.returncode, 0, first.stderr)
        again = self.run_launch('--print-url', '--engine', 'rust')
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertEqual(again.stdout, first.stdout)
        self.assertEqual(self.started(), [['--no-browser']], 'no second server, and no engine reaches it')

    def test_an_unknown_engine_starts_nothing(self):
        self.assertNotEqual(self.run_launch('--print-url', '--engine', 'matlab').returncode, 0)
        self.assertEqual(self.started(), [])


class Status(Bundle):
    def status(self):
        result = self.run_launch('--status')
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_nothing_running_and_nothing_built(self):
        self.assertEqual(self.status(), {'running': False, 'engine': 'octave', 'rust_available': False})
        self.assertEqual(self.started(), [], 'asking starts nothing')
        self.assertFalse((self.project / '.matlab-free').exists(), 'asking writes nothing')

    def test_the_last_choice_is_preselected(self):
        self.build()
        self.save('{"engine": "rust"}')
        self.assertEqual(self.status(), {'running': False, 'engine': 'rust', 'rust_available': True})

    def test_a_saved_rust_without_its_binary_preselects_octave(self):
        self.save('{"engine": "rust"}')
        self.assertEqual(self.status(), {'running': False, 'engine': 'octave', 'rust_available': False})

    def test_the_cli_binary_alone_does_not_make_rust_available(self):
        binary = self.project / 'motor' / 'target' / 'release' / 'indy'
        binary.parent.mkdir(parents=True)
        binary.write_text('')
        binary.chmod(0o755)
        self.assertFalse(self.status()['rust_available'])

    def test_a_running_server_is_reported_and_its_url_stays_private(self):
        self.assertEqual(self.run_launch('--print-url').returncode, 0)
        result = self.run_launch('--status')
        self.assertTrue(json.loads(result.stdout)['running'])
        self.assertNotIn('secret-token', result.stdout + result.stderr)

    def test_a_server_still_loading_counts_as_running(self):
        runtime = self.project / '.matlab-free'
        runtime.mkdir()
        import fcntl
        with (runtime / 'server.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.assertTrue(self.status()['running'])
        self.assertFalse(self.status()['running'])

    def test_a_missing_project_is_an_error_with_a_message(self):
        (self.project / 'app.py').unlink()
        result = self.run_launch('--status')
        self.assertEqual(result.returncode, 1)
        self.assertIn(str(self.project), result.stderr)


class SameRulesAsTheServer(unittest.TestCase):
    def setUp(self):
        self.folder = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.folder, True)
        # kernel_executable() also looks beside its own file; keep this checkout's build out of it.
        patcher = patch.object(kernel_rust, '__file__', str(self.folder / 'no-checkout' / 'backend' / 'kernel_rust.py'))
        patcher.start()
        self.addCleanup(patcher.stop)
        environment = patch.dict(os.environ, clean_environment(), clear=True)
        environment.start()
        self.addCleanup(environment.stop)

    def agree(self, root, expected):
        try:
            kernel_rust.kernel_executable(root)
            server = True
        except RuntimeError:
            server = False
        self.assertEqual(server, expected)
        self.assertEqual(launch.rust_available(root), expected)

    def binary(self, root, profile, name='indy-kernel', mode=0o755):
        path = root / 'motor' / 'target' / profile / name
        path.parent.mkdir(parents=True)
        path.write_text('')
        path.chmod(mode)
        return path

    def case(self, name):
        root = self.folder / name
        root.mkdir()
        return root

    def test_nothing_built(self):
        self.agree(self.case('empty'), False)

    def test_release_build(self):
        root = self.case('release')
        self.binary(root, 'release')
        self.agree(root, True)

    def test_debug_build(self):
        root = self.case('debug')
        self.binary(root, 'debug')
        self.agree(root, True)

    def test_cli_binary_only(self):
        root = self.case('cli')
        self.binary(root, 'release', 'indy')
        self.agree(root, False)

    def test_not_executable(self):
        root = self.case('plain')
        self.binary(root, 'release', mode=0o644)
        self.agree(root, False)

    def test_a_directory_with_that_name(self):
        root = self.case('directory')
        (root / 'motor' / 'target' / 'release' / 'indy-kernel').mkdir(parents=True)
        self.agree(root, False)

    def test_the_explicit_setting(self):
        root = self.case('explicit')
        with patch.dict(os.environ, {'INDYMAT_R_KERNEL': sys.executable}):
            self.agree(root, True)
        with patch.dict(os.environ, {'INDYMAT_R_KERNEL': '/elsewhere/indy-kernel'}):
            self.agree(root, False)
        with patch.dict(os.environ, {'INDYMAT_R_KERNEL': ''}):
            self.agree(root, False)

    def test_the_engines_and_the_saved_choice(self):
        self.assertEqual(launch.ENGINES, application.ENGINES)
        root = self.case('saved')
        runtime = root / '.matlab-free'
        runtime.mkdir()
        server = application.App.__new__(application.App)
        server.engine_file = runtime / 'engine.json'
        self.assertEqual(launch.saved_engine(root), server._saved_engine())
        for text in ('{"engine": "rust"}', '{"engine": "octave"}', '{"engine": "matlab"}', '[]', 'broken', ''):
            with self.subTest(text=text):
                server.engine_file.write_text(text)
                self.assertEqual(launch.saved_engine(root), server._saved_engine())


class ServerLock(unittest.TestCase):
    def test_a_status_probe_does_not_turn_a_new_server_away(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'server.lock'
            probe, server = path.open('w'), path.open('w')
            fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
            timer = threading.Timer(.04, probe.close)
            timer.start()
            application.take_server_lock(server)
            timer.join()
            server.close()

    def test_a_server_that_stays_is_still_found(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'server.lock'
            with path.open('w') as running, path.open('w') as second:
                fcntl.flock(running, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    application.take_server_lock(second)


class NativeStartup(unittest.TestCase):
    def test_reload_cannot_start_a_second_launch_while_the_page_loads(self):
        source = (ROOT / 'macos' / 'App.swift').read_text()
        start = source[source.index('func startServer'):source.index('func showFailure')]
        self.assertNotIn('starting = false', start)
        finish = source[source.index('didFinish navigation'):source.index('didFailProvisionalNavigation')]
        self.assertIn('starting = false', finish)

    def test_a_splash_brought_back_during_its_fade_is_visible(self):
        source = (ROOT / 'macos' / 'Splash.swift').read_text()
        self.assertIn('self.isHidden = true } else { self.alphaValue = 1 }', source)


class SplashLook(unittest.TestCase):
    def test_the_colours_are_the_app_header_colours(self):
        style = (ROOT / 'static' / 'style.css').read_text()
        def tokens(selector):
            block = re.search(r'^' + re.escape(selector) + r'\{(.*?)^\}', style, re.S | re.M).group(1)
            return dict(re.findall(r'(--[\w-]+):(#[0-9a-fA-F]{6})', block))
        light, dark = tokens(':root'), tokens('body.dark')
        splash = (ROOT / 'macos' / 'Splash.swift').read_text()
        found = re.findall(r'color\(light: 0x(\w{6}), dark: 0x(\w{6})\)\s*// (--[\w-]+)', splash)
        self.assertEqual({name for _, _, name in found}, {'--chrome', '--text', '--muted', '--line'})
        for in_light, in_dark, name in found:
            with self.subTest(token=name):
                self.assertEqual('#' + in_light.lower(), light[name].lower())
                self.assertEqual('#' + in_dark.lower(), dark[name].lower())

    def test_the_logo_is_the_horizontal_lockup_in_both_variants(self):
        name = 'indymat-logo-horizontal-{}-1024.png'
        self.assertIn('indymat-logo-horizontal-\\(variant)-1024.png', (ROOT / 'macos' / 'Splash.swift').read_text())
        self.assertIn('indymat-logo-horizontal-{variant}-1024.png', (ROOT / 'macos' / 'build_app.py').read_text())
        for variant in ('black', 'white'):
            self.assertTrue((ROOT / 'docs' / 'brand' / 'logo' / 'png' / name.format(variant)).is_file())


@unittest.skipUnless(sys.platform == 'darwin' and Path('/usr/bin/swiftc').exists(), 'needs the macOS Swift compiler')
class NativeSources(unittest.TestCase):
    def test_the_native_window_type_checks(self):
        arch = 'arm64' if os.uname().machine == 'arm64' else 'x86_64'
        with tempfile.TemporaryDirectory() as cache:
            result = subprocess.run(['/usr/bin/swiftc', '-typecheck', '-swift-version', '5', '-target', f'{arch}-apple-macos12.0',
                                     '-module-cache-path', cache,
                                     *(str(ROOT / 'macos' / name) for name in ('App.swift', 'Splash.swift', 'main.swift'))],
                                    capture_output=True, text=True, timeout=300)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
