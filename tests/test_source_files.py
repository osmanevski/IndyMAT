"""Saved entry capability, fallback, exact mapping and transactional retention (no engine)."""
import hashlib
from contextlib import contextmanager
import json
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from backend import source_files
from backend.files import Workspace
from backend.kernel import Kernel
from backend.source_adapter import AdapterProfile, PackageSupport
from backend.source_files import SourceFile, read_file_source, adapt_file_source, map_file_errors

PROFILE = AdapterProfile(True, package=PackageSupport(True, '/verified/string.m', 'fixture', ''))


class SavedSourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.work = self.root/'work'
        self.work.mkdir()
        self.workspace = Workspace(self.work, allowed_root=self.root)
        self.path = self.work/'entry.m'

    def tearDown(self):
        for entry in self.root.glob('runtime/*/entry'): entry.chmod(0o700)
        self.temp.cleanup()

    def source(self, text='r="ab"+"cd";', bom=False):
        data = (b'\xef\xbb\xbf' if bom else b'')+text.encode('utf-8')
        self.path.write_bytes(data)
        return read_file_source(self.workspace, str(self.path), hashlib.sha256(data).hexdigest())

    def kernel(self):
        k = Kernel.__new__(Kernel)
        k.lock = threading.RLock()
        k.state = {'status':'idle','job':'previous','output':'kept'}
        k.breakpoints = {}
        k.root = self.root
        k.workspace = self.work
        k.executable = 'octave-cli'
        k.runtime = self.root/'runtime'
        k.runtime.mkdir()
        k.job = 'previous'
        k.generation = 17
        k.proc = SimpleNamespace(poll=lambda: None)
        k.events = None
        k._retained_file_jobs = set()
        k._file_request_count = k._file_staged_bytes = 0
        k._source_package = lambda **kw: PROFILE.package
        k._write_command = lambda *args: None
        return k

    def submit(self, k, source):
        with patch('backend.kernel.threading.Thread'):
            return k.submit(mode='file', argument=source.path, source_file=source)

    def test_bom_hash_snapshot_and_utf16_context(self):
        source = self.source('% 😀\r\nr="x";\r\n', bom=True)
        self.path.write_text('changed')
        self.assertTrue(source.saved_bytes.startswith(b'\xef\xbb\xbf'))
        self.assertEqual(source.document, '% 😀\r\nr="x";\r\n')
        self.assertEqual(source.context()['span'], {'start_utf16':0,'end_utf16':len(source.document.encode('utf-16-le'))//2})
        self.assertEqual(source.context()['profile'], 'matlab')
        with self.assertRaises(FileExistsError): read_file_source(self.workspace, source.path, source.sha256)

    def test_boundary_private_paths_and_bad_hash(self):
        self.source()
        for digest in [None, '', 'a'*63, 'A'*64, True]:
            with self.assertRaises(ValueError): read_file_source(self.workspace, str(self.path), digest)
        with self.assertRaises(PermissionError): read_file_source(self.workspace, '/etc/outside.m', 'a'*64)
        self.workspace.source_private_roots = (self.work,)
        with self.assertRaises(PermissionError): read_file_source(self.workspace, str(self.path), 'a'*64)

    def test_descriptor_rejects_symlink_swap_and_same_bytes_replacement(self):
        source=self.source()
        parent=source_files._parent
        calls=0
        @contextmanager
        def replace_before_recheck(path):
            nonlocal calls
            calls+=1
            if calls==2:
                replacement=path.with_suffix('.replacement')
                replacement.write_bytes(source.saved_bytes)
                replacement.replace(path)
            with parent(path) as value:yield value
        with patch.object(source_files,'_parent',replace_before_recheck), self.assertRaises(FileExistsError):
            read_file_source(self.workspace,source.path,source.sha256)
        target=self.work/'target.m'
        target.write_bytes(source.saved_bytes)
        @contextmanager
        def symlink_after_boundary(path):
            path.unlink()
            path.symlink_to(target)
            with parent(path) as value:yield value
        with patch.object(source_files,'_parent',symlink_after_boundary), self.assertRaises(OSError):
            read_file_source(self.workspace,source.path,source.sha256)

    def test_server_issued_capability_only(self):
        with self.assertRaises(ValueError): adapt_file_source({}, PROFILE)
        with self.assertRaises(ValueError): adapt_file_source(SourceFile(str(self.path), 'r="x";', 'a'*64, b'r="x";'), PROFILE)

    def test_declarations_hierarchies_debug_and_lexical_fallback(self):
        for text in ['function y=entry()\ny="x";\nend', 'r="x";\nfunction y=f()\ny=1;\nend', 'classdef entry\nend']:
            self.assertEqual(adapt_file_source(self.source(text), PROFILE).diagnostics[0].code, 'file-declaration')
        for folder in ['private', '+pkg', '@cls']:
            target=self.work/folder;target.mkdir()
            self.path=target/'entry.m'
            self.assertEqual(adapt_file_source(self.source(), PROFILE).diagnostics[0].code, 'file-hierarchy')
        self.path=self.work/'entry.m'
        self.assertEqual(adapt_file_source(self.source(), PROFILE).diagnostics[0].code, 'file-hierarchy')
        (self.work/'private'/'entry.m').unlink()
        (self.work/'private').rmdir()
        self.assertEqual(adapt_file_source(self.source(), PROFILE, debugging=True).diagnostics[0].code, 'file-debug')
        result=adapt_file_source(self.source('r="x"; disp "hello"'), PROFILE)
        self.assertEqual(result.status, 'fallback')
        self.assertEqual(result.generated_text, self.path.read_text())

    def test_exact_entry_only_mapping_and_native_line_only(self):
        source=self.source('% 😀\nr="ş"; missing;\n')
        adapted=adapt_file_source(source, PROFILE)
        physical='/private/runtime/job/entry/entry.m'
        generated=adapted.generated_text.splitlines()[1]
        column=len(generated[:generated.index('missing')].encode('utf-8'))+1
        frames=[{'file':physical,'line':2,'column':column}, {'file':source.path,'line':2,'column':column}, {'file':'/dependency.m','line':2,'column':column}]
        original=json.loads(json.dumps(frames))
        locations=map_file_errors(adapted, source.context(), physical, frames)
        self.assertEqual(len(locations),1)
        self.assertEqual(locations[0]['column'],len('r="ş"; ')+1)
        self.assertEqual(frames,original)
        native=map_file_errors(adapted,source.context(),physical,frames,native=True)
        self.assertEqual(len(native),1)
        self.assertTrue(native[0]['line_only'])
        self.assertEqual(native[0]['column'],1)

    def test_immutable_same_basename_exact_quota_and_retention(self):
        source=self.source()
        k=self.kernel()
        job=self.submit(k,source)
        entry=k.runtime/job/'entry'/'entry.m'
        self.assertEqual(entry.stat().st_mode & 0o222,0)
        self.assertEqual(entry.parent.stat().st_mode & 0o222,0)
        self.assertEqual(self.path.read_text(),source.document)
        self.assertEqual(k._file_request_count,1)
        self.assertEqual(k._file_staged_bytes,sum(p.stat().st_size for p in (k.runtime/job).rglob('*') if p.is_file()))
        self.assertEqual(k.state['kind'],'file')
        self.assertEqual(k.state['source_adapter']['scope'],'entry-file')
        metadata=json.loads((k.runtime/job/'source.json').read_text())
        self.assertEqual(metadata['source_context'],source.context())
        self.assertEqual(metadata['physical_path'],str(entry))
        for index in range(30): (k.runtime/('later-'+str(index))).mkdir()
        k.state.update(status='idle',job='other')
        k._prune()
        self.assertTrue(entry.exists())
        self.assertEqual(k._file_request_count,1)

    def test_request_and_byte_quota_reject_before_mutation(self):
        for limits in [{'MAX_RETAINED_FILES':0},{'MAX_RETAINED_BYTES':1}]:
            k=self.kernel() if not (self.root/'runtime').exists() else self.saved_kernel
            self.saved_kernel=k
            source=self.source('r=1;')  # unchanged must still consume budget
            before=json.loads(json.dumps(k.state))
            with patch.multiple('backend.kernel',**limits), self.assertRaises(ValueError): self.submit(k,source)
            self.assertEqual(k.state,before)
            self.assertEqual(k.job,'previous')
            self.assertEqual(list(k.runtime.iterdir()),[])
            self.assertEqual(k._file_request_count,0)
            self.assertEqual(k._file_staged_bytes,0)

    def test_sixty_four_request_limit_and_fallback_accounting(self):
        k=self.kernel()
        for text in ['r=1;', 'r="x"; disp "hello"']:
            source=self.source(text)
            self.submit(k,source)
            k.state['status']='idle'
        self.assertEqual(k._file_request_count,2)
        self.assertEqual(len(k._retained_file_jobs),2)
        k._file_request_count=64
        before=dict(k.state)
        folders=set(k.runtime.iterdir())
        with self.assertRaises(ValueError):self.submit(k,self.source())
        self.assertEqual(k.state,before)
        self.assertEqual(set(k.runtime.iterdir()),folders)
        self.assertEqual(k._file_request_count,64)

    def test_only_explicit_start_releases_retention_counters(self):
        k=self.kernel()
        job=self.submit(k,self.source())
        k.state['status']='idle'
        k._prune()
        self.assertEqual(k._retained_file_jobs,{job})
        with patch('backend.kernel._Detached'), patch('backend.kernel.subprocess.Popen'), patch('backend.kernel.threading.Thread'), patch.object(k,'_submit'):
            k.start()
        self.assertEqual(k._retained_file_jobs,set())
        self.assertEqual(k._file_request_count,0)
        self.assertEqual(k._file_staged_bytes,0)

    def test_reset_removes_retained_files_and_refuses_to_forget_failed_cleanup(self):
        k=self.kernel()
        k.breakpoint_jobs={}
        job=self.submit(k,self.source())
        folder=k.runtime/job
        count=k._file_request_count
        with patch.object(k,'_kill'), patch.object(k,'_remove_job'), patch.object(k,'start') as start:
            with self.assertRaises(OSError):k.reset()
            start.assert_not_called()
        self.assertTrue(folder.exists())
        self.assertEqual(k._file_request_count,count)
        self.assertEqual(k.state['status'],'dead')
        with patch.object(k,'_kill'), patch('backend.kernel._Detached'), patch('backend.kernel.subprocess.Popen'), patch('backend.kernel.threading.Thread'), patch.object(k,'_submit'):
            k.reset()
        self.assertFalse(folder.exists())
        self.assertEqual(k._retained_file_jobs,set())
        self.assertEqual(k._file_request_count,0)
        self.assertEqual(k._file_staged_bytes,0)

    def test_failed_staging_leaves_idle_state_and_no_retention(self):
        source=self.source()
        k=self.kernel()
        before=dict(k.state)
        write=Path.write_bytes
        def fail(path,data):
            if path.name=='source.json': raise OSError('fixture disk failure')
            return write(path,data)
        with patch.object(Path,'write_bytes',fail), self.assertRaises(ValueError): self.submit(k,source)
        self.assertEqual(k.state,before)
        self.assertEqual(k.job,'previous')
        self.assertEqual(list(k.runtime.iterdir()),[])
        self.assertEqual(k._retained_file_jobs,set())
        self.assertEqual(k._file_request_count,0)
        self.assertEqual(k._file_staged_bytes,0)


if __name__ == '__main__': unittest.main()
