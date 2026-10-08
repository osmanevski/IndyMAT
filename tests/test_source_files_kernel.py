"""Real persistent Kernel acceptance for opt-in saved entry scripts; root runs serially."""
import hashlib
import os
import tempfile
import time
import unittest
from pathlib import Path

from backend.files import Workspace
from backend.kernel import Kernel
from backend.source_files import read_file_source

ROOT=Path(__file__).resolve().parents[1]


class SavedEntryKernelTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.base=Path(self.temp.name)
        self.work=self.base/'work';self.work.mkdir()
        self.entry_dir=self.work/'entries';self.entry_dir.mkdir()
        self.file=self.entry_dir/'entry.m'
        self.workspace=Workspace(self.work,allowed_root=self.base)
        self.kernel=Kernel(ROOT,self.base/'runtime',self.work,executable='octave-cli')
        self.wait()

    def tearDown(self):
        self.kernel.close()
        for entry in self.kernel.runtime.glob('*/entry'):entry.chmod(0o700)
        self.temp.cleanup()

    def wait(self,predicate=None):
        deadline=time.monotonic()+30
        while time.monotonic()<deadline:
            state=self.kernel.snapshot()
            complete=predicate(state) if predicate else state['status'] in ('idle','dead')
            if complete:return state
            time.sleep(.025)
        self.fail(str(self.kernel.snapshot()))

    def native(self,code):
        self.kernel.submit(code)
        result=self.wait()
        self.assertFalse(result['error'],result)
        return result

    def source(self,text,bom=False):
        data=(b'\xef\xbb\xbf' if bom else b'')+text.encode('utf-8')
        self.file.write_bytes(data)
        return read_file_source(self.workspace,str(self.file),hashlib.sha256(data).hexdigest())

    def run_saved(self,text):
        source=self.source(text)
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        result=self.wait()
        self.assertEqual(result['status'],'idle',result)
        self.assertEqual(result['kind'],'file')
        self.assertEqual(result['source_adapter']['scope'],'entry-file')
        return result

    def test_off_default_and_native_profile_publish_scope(self):
        source=self.source('r="ab"+"cd";')
        self.kernel.submit(mode='file',argument=source.path)
        state=self.wait()
        self.assertNotIn('source_adapter',state)
        self.native('assert(isa(r,"double"));')
        self.assertEqual(self.kernel._file_request_count,0)
        with self.assertRaises(ValueError):self.kernel.submit(mode='profile',argument=source.path,source_file=source)
        with self.assertRaises(ValueError):self.kernel.submit(mode='publish',argument=source.path,source_file=source)

    def test_identity_dependencies_cwd_original_bytes_and_base_handles(self):
        (self.entry_dir/'dep.m').write_text('function y=dep()\npersistent n=0;n=n+1;y=n;\nendfunction')
        (self.entry_dir/'native_script.m').write_text('dependency_literal="native";')
        (self.entry_dir/'relative.txt').write_text('relative-data')
        self.native('kept=19; kept_handle=@sin; string=17;')
        code="r=\"a\"+\"b\"; dependency_count=dep(); dependency_handle=@dep; dep_info=functions(dependency_handle); native_script; where=pwd(); stem=mfilename(); physical=mfilename('fullpath'); resolved=which('entry'); data=fileread('relative.txt'); captured=@()r;"
        result=self.run_saved(code)
        self.assertFalse(result['error'],result)
        self.assertEqual(result['source_adapter']['status'],'adapted')
        self.assertEqual(result['cwd'],str(self.work))
        self.assertEqual(self.file.read_text(),code)
        physical=self.kernel.runtime/result['job']/'entry'/'entry.m'
        self.native("assert(strcmp(stem,'entry')); assert(strcmp(physical,"+self.kernel.quote(str(physical.with_suffix('')))+")); assert(strcmp(resolved,"+self.kernel.quote(str(self.file))+")); assert(strcmp(where,"+self.kernel.quote(str(self.entry_dir))+")); assert(strcmp(data,'relative-data')); assert(isa(dependency_literal,'char')); assert(strcmp(char(captured()),'ab')); assert(string==17); assert(kept==19); assert(kept_handle(0)==0); assert(dependency_handle()==2); assert(strcmp(dep_info.file,"+self.kernel.quote(str(self.entry_dir/'dep.m'))+") );")
        # More than prune's native job window: captured handles retain entry bytes.
        for _ in range(23):self.native('prune_tick=1;')
        self.assertTrue(physical.exists())
        self.native("assert(strcmp(char(captured()),'ab'));")

    def test_errors_exact_entry_maps_and_once_only(self):
        self.native('attempts=0;')
        result=self.run_saved('attempts=attempts+1;\nr="ş"; missing_saved_entry;')
        self.assertEqual(result['source_adapter']['status'],'adapted')
        self.assertEqual(result['error'],result['raw_error'])
        self.assertTrue(any(f['file']==str(self.kernel.runtime/result['job']/'entry'/'entry.m') for f in result['error_frames']))
        self.assertEqual(result['source_error_locations'][0]['path'],str(self.file))
        self.assertEqual(result['source_error_locations'][0]['line'],2)
        self.native('assert(attempts==1);')
        (self.entry_dir/'dep_error.m').write_text('function dep_error()\nmissing_native_dependency;\nendfunction')
        result=self.run_saved('r="x"; dep_error();')
        self.assertTrue(any(f['file']==str(self.entry_dir/'dep_error.m') for f in result['error_frames']))
        self.assertTrue(all(item['path']==str(self.file) for item in result['source_error_locations']))

    def test_input_stop_twice_preserves_epoch_variables_and_cwd(self):
        self.native('kept=31; kept_handle=@sin;')
        epoch=self.kernel.generation
        source=self.source('r="x"; n=input(\'Number: \');')
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        self.wait(lambda s:s['waiting_input'])
        self.kernel.input('17')
        self.assertFalse(self.wait()['error'])
        self.native('assert(n==17);')
        source=self.source('r="x"; while true; pause(.01); end')
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        self.wait(lambda s:s['status']=='running')
        time.sleep(.2)
        self.kernel.interrupt();self.kernel.interrupt()
        result=self.wait()
        self.assertEqual(self.kernel.generation,epoch)
        self.assertEqual(result['cwd'],str(self.work))
        self.native('assert(kept==31); assert(kept_handle(0)==0);')

    def test_deliberate_cd_success_error_and_stop(self):
        target=self.work/'chosen';target.mkdir()
        for tail in ['', 'missing_after_cd;', 'while true; pause(.01); end']:
            self.native('cd('+self.kernel.quote(str(self.work))+');')
            source=self.source('r="x"; cd('+self.kernel.quote(str(target))+'); '+tail)
            self.kernel.submit(mode='file',argument=source.path,source_file=source)
            if 'while' in tail:
                self.wait(lambda s:s['status']=='running');time.sleep(.2)
                self.kernel.interrupt();self.kernel.interrupt()
            state=self.wait()
            self.assertEqual(state['cwd'],str(target),state)

    def test_snapshot_saved_hash_rechecked_by_engine_including_bom(self):
        source=self.source('executed_changed_source=1; r="x";',bom=True)
        # Change after server snapshot, before the execution descriptor reaches Octave.
        self.file.write_text('executed_changed_source=2;')
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        result=self.wait()
        self.assertIn('saved source changed',result['error'].lower())
        self.native("assert(!exist('executed_changed_source','var'));")
        source=self.source('r="x";',bom=True)
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        self.assertFalse(self.wait()['error'])

    def test_private_local_and_uncertain_whole_unit_fallback(self):
        private=self.entry_dir/'private';private.mkdir()
        result=self.run_saved('r="x";')
        self.assertEqual(result['source_adapter']['diagnostics'][0]['code'],'file-hierarchy')
        self.native('assert(isa(r,"char"));')
        private.rmdir()
        result=self.run_saved('r="x";\nfunction y=local()\ny=1;\nendfunction')
        self.assertEqual(result['source_adapter']['diagnostics'][0]['code'],'file-declaration')
        result=self.run_saved('r="x"; disp "hello"')
        self.assertEqual(result['source_adapter']['status'],'fallback')
        self.native('assert(isa(r,"char"));')

    def test_engine_console_breakpoint_and_profiler_fallback(self):
        (self.work/'watch.m').write_text('function y=watch()\ny=1;\nendfunction')
        # Octave reparses a file written in the current timestamp tick at its
        # next prompt, dropping console dbstop. Exercise an established file.
        os.utime(self.work/'watch.m',(time.time()-2,time.time()-2))
        self.native('dbstop watch 2;')
        result=self.run_saved('r="x"; missing_native_fallback;')
        self.assertEqual(result['source_adapter']['diagnostics'][0]['code'],'file-debug')
        self.assertTrue(result['source_error_locations'][0]['line_only'])
        self.native('dbclear watch; profile on;')
        result=self.run_saved('r="x"; missing_profile_fallback;')
        self.assertEqual(result['source_adapter']['diagnostics'][0]['code'],'file-profile')
        self.assertTrue(result['source_error_locations'][0]['line_only'])
        self.native('profile off; assert(isa(r,"char"));')
        self.native('dbstop if error;')
        result=self.run_saved('r="x";')
        self.assertEqual(result['source_adapter']['diagnostics'][0]['code'],'file-debug')
        self.native('dbclear if error; assert(isa(r,"char"));')

    def test_saved_file_breakpoint_with_stop_on_error_stays_native(self):
        source=self.source('r="x";\nafter_breakpoint=1;')
        os.utime(self.file,(time.time()-2,time.time()-2))
        self.native("addpath("+self.kernel.quote(self.entry_dir)+"); dbstop('entry','2'); dbstop if error;")
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        paused=self.wait(lambda s:s['status']=='paused' and (s.get('debug') or {}).get('ready'))
        self.assertTrue(any(frame.get('file')==source.path for frame in paused['debug']['stack']))
        self.kernel.debug('continue')
        state=self.wait()
        self.assertFalse(state['error'],state)
        self.assertEqual(state['source_adapter']['status'],'fallback')
        self.native('assert(isa(r,"char"));assert(after_breakpoint==1);')

    def test_editor_breakpoint_and_stop_on_error_stays_native(self):
        chosen=self.work/'chosen';chosen.mkdir()
        source=self.source('r="x";\nafter_breakpoint=1;\ncd('+self.kernel.quote(chosen)+');')
        self.native('dbstop if error;')
        self.kernel.configure_breakpoint(self.file,2,'set')
        self.assertFalse(self.wait()['error'])
        self.kernel.submit(mode='file',argument=source.path,source_file=source)
        paused=self.wait(lambda s:s['status']=='paused' and (s.get('debug') or {}).get('ready'))
        self.assertTrue(any(frame.get('file')==source.path for frame in paused['debug']['stack']))
        self.kernel.debug('continue')
        state=self.wait()
        self.assertFalse(state['error'],state)
        self.assertEqual(state['cwd'],str(chosen))
        self.assertEqual(state['source_adapter']['status'],'fallback')
        self.native('assert(isa(r,"char"));assert(after_breakpoint==1);')

    def test_constructor_shadow_in_original_directory_is_native(self):
        (self.entry_dir/'string.m').write_text('function y=string(x)\ny=x;\nendfunction')
        result=self.run_saved('r="x";')
        self.assertEqual(result['source_adapter']['status'],'fallback')
        self.native('assert(isa(r,"char"));')


if __name__ == '__main__':unittest.main()
