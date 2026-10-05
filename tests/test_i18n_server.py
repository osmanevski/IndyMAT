"""Server language contract, security ordering, and real Octave command boundaries."""
import ast
import json
from pathlib import Path
import shutil
import shlex
import string
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
from email.message import Message
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from app import Handler, workspace_scalar
from backend.i18n import get_language, set_language, tr
from backend.kernel import Kernel
from backend.file_operations import exclusive_rename
from backend.locales.tr import TRANSLATIONS
from backend.workspace_actions import MatOverwriteConflict, variable_size


class LanguageTests(unittest.TestCase):
    def setUp(self):
        self.previous = get_language()
        set_language('en')

    def tearDown(self):
        set_language(self.previous)

    def test_default_english_in_new_process(self):
        result = subprocess.run([sys.executable, '-c',
            "from backend.i18n import get_language,tr; "
            "assert get_language()=='en'; assert tr('File not found.')=='File not found.'"],
            cwd=ROOT, capture_output=True, text=True, check=True)
        self.assertEqual(result.stdout, '')

    def test_fallback_placeholders_and_strict_language(self):
        self.assertEqual(tr('File not found.'), 'File not found.')
        set_language('tr')
        self.assertEqual(tr('File not found.'), 'Dosya bulunamadı.')
        self.assertEqual(tr('Untranslated {name}: {value:g}', name='A', value=2.0), 'Untranslated A: 2')
        self.assertEqual(tr('The {seconds:g}-second time limit was exceeded.', seconds=2.5), '2.5 saniyelik süre sınırı aşıldı.')
        self.assertEqual(tr('IndyMAT is running: {url}\nPress Ctrl+C to close.\n', url='local'),
                         'IndyMAT çalışıyor: local\nKapatmak için Ctrl+C.\n')
        with self.assertRaises(KeyError):
            tr('{missing}')
        for invalid in ('TR', ' tr', 'tr ', 'system', '', None, [], 1):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                set_language(invalid)
            self.assertEqual(get_language(), 'tr')

    def test_original_turkish_dynamic_messages(self):
        set_language('tr')
        self.assertEqual(tr('The file operation completed; the item is now at “{path}”. Could not apply breakpoints: {error}',
                            path='/home/İş.m', error='deneme'),
                         'Dosya işlemi tamamlandı; öğe artık “/home/İş.m” konumunda. Kesme noktaları uygulanamadı: deneme')
        with self.assertRaisesRegex(ValueError, '^Boyut geçersiz\.$'):
            variable_size(None)
        with self.assertRaisesRegex(ValueError, '^int8 değeri sınıf aralığında bir tam sayı olmalı\.$'):
            workspace_scalar('int8', 999)
        conflict = MatOverwriteConflict(tr('The MAT file already exists; approval is required. [sha256:{hash}] [approval:{approval}]',
                                          hash='a'*64, approval='b'*48), 'a'*64, 'b'*48)
        self.assertEqual(str(conflict), 'MAT dosyası zaten var; onay gerekir. [sha256:'+'a'*64+'] [approval:'+'b'*48+']')
        self.assertEqual(conflict.code, 'mat_overwrite_required')

    def test_catalog_covers_every_literal_source_and_preserves_fields(self):
        used = set()
        for path in [ROOT/'app.py', *(ROOT/'backend').glob('*.py')]:
            for node in ast.walk(ast.parse(path.read_text())):
                if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == 'tr'):
                    continue
                self.assertTrue(node.args and isinstance(node.args[0], ast.Constant) and isinstance(node.args[0].value, str),
                                f'Nonliteral translation key in {path.name}:{node.lineno}')
                source = node.args[0].value
                used.add(source)
                self.assertIn(source, TRANSLATIONS, f'{path.name}:{node.lineno}: {source}')
        self.assertEqual(used, set(TRANSLATIONS))
        def fields(text):
            return [(name, spec, conversion) for _, name, spec, conversion in string.Formatter().parse(text) if name is not None]
        for source, target in TRANSLATIONS.items():
            with self.subTest(source=source):
                self.assertEqual(sorted(fields(source)), sorted(fields(target)))

    def test_no_turkish_letter_literals_outside_locales(self):
        # No exceptions: compatibility machine identifiers such as "onceki"
        # have no Turkish letters and remain unchanged.
        for path in [ROOT/'app.py', *(ROOT/'backend').rglob('*.py')]:
            if path != ROOT/'app.py' and 'locales' in path.relative_to(ROOT/'backend').parts:
                continue
            for node in ast.walk(ast.parse(path.read_text())):
                if isinstance(node, ast.Constant) and isinstance(node.value, str):
                    self.assertFalse(set(node.value) & set('çğıöşüÇĞİÖŞÜ'), f'{path.name}:{node.lineno}: {node.value}')

    def test_messages_are_not_frozen_at_module_import(self):
        source = 'This platform cannot guarantee that the Trash path will not follow symbolic links. Checks before and after the operation cannot detect a temporary path change in between; Move to Trash is unavailable, and the item was not moved.'
        with patch('backend.file_operations.sys.platform', 'unsupported'):
            for language in ('tr', 'en'):
                set_language(language)
                with self.assertRaises(ValueError) as error:
                    exclusive_rename(0, 'source', 0, 'target', nofollow_any=True)
                self.assertEqual(str(error.exception), tr(source))

    def test_concurrent_translation_and_language_updates(self):
        failures = []
        def worker(language):
            for _ in range(500):
                set_language(language)
                value = tr('Could not start Octave: {error}', error='probe')
                if value not in ('Could not start Octave: probe', 'Octave başlatılamadı: probe'):
                    failures.append(value)
        threads = [threading.Thread(target=worker, args=(language,)) for language in ('en', 'tr', 'en', 'tr')]
        for thread in threads:thread.start()
        for thread in threads:thread.join()
        self.assertEqual(failures, [])


class HeaderTests(unittest.TestCase):
    # Exercise the real Handler's validation, dispatch, and response mapping
    # in memory: the lane sandbox cannot bind sockets. http_i18n.cjs exercises
    # wire-level parsing against the gate's real server.
    def setUp(self):
        self.previous = get_language()
        set_language('en')
        self.kernel = SimpleNamespace(lock=threading.RLock())
        self.server = SimpleNamespace(app=SimpleNamespace(port=12345, token='test-token', kernel=self.kernel, workspace=None))

    def tearDown(self):
        set_language(self.previous)

    def request(self, headers=(), path='/api/missing', post=False):
        import io
        handler = Handler.__new__(Handler)
        handler.server = self.server
        handler.path = path
        handler.headers = Message()
        handler.headers['Host'] = '127.0.0.1:12345'
        handler.headers['X-MF-Token'] = 'test-token'
        for name, value in headers:handler.headers[name] = value
        handler.headers['Content-Length'] = '2'
        handler.rfile = io.BytesIO(b'{}')
        responses = []
        handler.send = lambda status, body, *args, **kwargs:responses.append((status, body))
        handler.handle_request(post)
        self.assertEqual(len(responses), 1)
        return responses[0]

    def test_headers_default_valid_invalid_duplicate_and_absent(self):
        self.assertEqual(self.request(), (404, {'error':'Not found'}))
        for language, error in [('tr', 'Bulunamadı'), ('en', 'Not found')]:
            self.assertEqual(self.request([('X-MF-Language', language)]), (404, {'error':error}))
            self.assertEqual(get_language(), language)
        for value in ('TR', 'tr ', 'en,tr', 'system', '', 'fr'):
            self.request([('X-MF-Language', value)])
            self.assertEqual(get_language(), 'en')
        self.request([('X-MF-Language', 'tr'), ('X-MF-Language', 'en')])
        self.assertEqual(get_language(), 'en')
        self.request([('X-MF-Language', 'tr')])
        self.request()
        self.assertEqual(get_language(), 'tr')
        self.assertEqual(self.request([('X-MF-Language', 'en')], post=True), (404, {'error':'Not found'}))

    def test_authentication_precedes_language_and_non_api_cannot_change_it(self):
        handler = Handler.__new__(Handler)
        handler.server = self.server
        for extra in ({'Host':'evil.invalid'}, {'Origin':'https://evil.invalid'},
                      {'Sec-Fetch-Site':'cross-site'}, {'X-MF-Token':'invalid'}):
            headers = Message()
            for key, value in {'Host':'127.0.0.1:'+str(self.server.app.port), 'X-MF-Token':'test-token', 'X-MF-Language':'tr', **extra}.items():
                headers[key] = value
            handler.headers = headers
            with self.assertRaises(PermissionError):handler.validate(api=True)
            self.assertEqual(get_language(), 'en')
        headers.replace_header('X-MF-Token', 'test-token')
        handler.validate(api=False)
        self.assertEqual(get_language(), 'en')
        for invalid in (' tr', 'tr ', 'TR'):
            headers.replace_header('X-MF-Language', invalid)
            handler.validate(api=True)
            self.assertEqual(get_language(), 'en')

    def test_conflict_metadata_in_http_response(self):
        with patch.object(Handler, 'post', side_effect=MatOverwriteConflict('legacy message', 'a'*64, 'b'*48)):
            status, body = self.request(post=True)
        self.assertEqual(status, 409)
        self.assertEqual(body, {'error':'legacy message', 'code':'mat_overwrite_required', 'expected_hash':'a'*64, 'approval':'b'*48})


@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli is required')
class OctaveLanguageTests(unittest.TestCase):
    def setUp(self):
        self.previous = get_language()
        set_language('en')
        self.temp = tempfile.TemporaryDirectory()
        self.work = Path(self.temp.name)/'work';self.work.mkdir()
        self.kernel = Kernel(ROOT, Path(self.temp.name)/'jobs', self.work, executable=shutil.which('octave-cli'))
        self.idle()

    def tearDown(self):
        self.kernel.close();self.temp.cleanup();set_language(self.previous)

    def wait(self, predicate):
        deadline = time.monotonic()+20
        while time.monotonic()<deadline:
            state = self.kernel.snapshot()
            if predicate(state):return state
            time.sleep(.03)
        self.fail(str(self.kernel.snapshot()))

    def idle(self):
        state = self.wait(lambda s:s['status'] in ('idle','dead'))
        self.assertEqual(state['status'], 'idle', state)
        self.assertFalse(state['error'], state)
        return state

    def code(self, code):
        self.kernel.submit(code)
        return self.idle()

    def test_spawn_changes_clear_all_shadowing_and_reset(self):
        self.code("assert(strcmp(getenv('INDYMAT_LANGUAGE'),'en')); kept=73; f=@(x)x+2; setenv=17;")
        pid = self.kernel.proc.pid
        generation = self.kernel.generation
        set_language('tr')
        self.code("assert(strcmp((@getenv)('INDYMAT_LANGUAGE'),'tr')); assert(kept==73 && f(3)==5 && setenv==17);")
        self.assertEqual(self.kernel.proc.pid, pid)
        self.assertEqual(self.kernel.generation, generation)
        self.code("clear all; assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr')); kept=91;")
        set_language('en')
        self.code("assert(strcmp(getenv('INDYMAT_LANGUAGE'),'en')); assert(kept==91);")
        set_language('tr')
        inherited = Path(self.temp.name)/'spawn-language'
        launcher = Path(self.temp.name)/'octave-wrapper'
        launcher.write_text("#!/bin/sh\nprintf '%s' \"$INDYMAT_LANGUAGE\" > " + shlex.quote(str(inherited)) +
                            "\nexec " + shlex.quote(shutil.which('octave-cli')) + " \"$@\"\n")
        launcher.chmod(0o700)
        self.kernel.executable = str(launcher)
        self.kernel.reset();self.idle()
        self.assertEqual(inherited.read_text(), 'tr')
        self.code("assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr'));")

    def test_running_job_and_input_do_not_receive_extra_stdin(self):
        self.kernel.submit("pause(.5); assert(strcmp(getenv('INDYMAT_LANGUAGE'),'en')); kept=81;")
        set_language('tr')
        self.idle()
        self.code("assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr')); assert(kept==81);")
        self.kernel.submit("answer=input('probe: '); assert(answer==17); assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr'));")
        self.wait(lambda s:s.get('waiting_input'))
        set_language('en')
        time.sleep(.1)
        self.assertTrue(self.kernel.snapshot()['waiting_input'])
        self.kernel.input('17');self.idle()
        self.code("assert(strcmp(getenv('INDYMAT_LANGUAGE'),'en')); assert(kept==81);")

    def test_language_change_after_paused_return_keeps_continue_working(self):
        file = self.work/'language_return.m'
        file.write_text("function out=language_return()\nout=-1; kept=20;\nkeyboard;\nout=kept;\nendfunction\n")
        self.kernel.submit("answer=language_return(); assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr'));")
        paused = self.wait(lambda s:s['status']=='paused' and (s.get('debug') or {}).get('ready'))
        self.kernel.debug('eval', 'kept=77; return; kept=88;')
        self.wait(lambda s:s['status']=='paused' and s['debug'].get('ready') and s['debug']['serial']!=paused['debug']['serial'])
        set_language('tr')
        self.kernel.debug('continue')
        self.idle()
        self.code('assert(answer==-1);')

    def test_debug_pause_uses_next_debug_command_and_preserves_locals(self):
        file = self.work/'language_pause.m'
        file.write_text("function out=language_pause()\nsetenv=33; kept=77;\nout=kept+1;\nendfunction\n")
        self.kernel.breakpoint(file, 3, True);self.idle()
        self.kernel.submit('answer=language_pause();')
        paused = self.wait(lambda s:s['status']=='paused' and (s.get('debug') or {}).get('ready'))
        generation = self.kernel.generation
        set_language('tr')
        self.assertEqual(self.kernel.snapshot()['status'], 'paused')
        self.kernel.debug('eval', "assert(strcmp((@getenv)('INDYMAT_LANGUAGE'),'tr')); assert(setenv==33 && kept==77);")
        self.wait(lambda s:s['status']=='paused' and s['debug'].get('ready') and s['debug']['serial']!=paused['debug']['serial'])
        set_language('en')
        self.kernel.debug('continue');self.idle()
        self.code("assert(answer==78); assert(strcmp(getenv('INDYMAT_LANGUAGE'),'en'));")
        self.assertEqual(self.kernel.generation, generation)


if __name__ == '__main__':
    unittest.main(verbosity=2)
