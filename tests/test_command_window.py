import json
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))

from app import App,Handler
from backend.files import Workspace
from backend.kernel import Kernel


class CommandHistoryTests(unittest.TestCase):
    def make_app(self,folder,session='0123456789abcdef'):
        app=App.__new__(App)
        app.runtime=Path(folder)
        app.history_path=app.runtime/'history.json'
        app.history_lock=threading.Lock()
        app.history_session=session
        app._load_history()
        return app

    def test_review_11_legacy_migration_persists_ids_without_a_mutation(self):
        with tempfile.TemporaryDirectory() as folder:
            history=Path(folder)/'history.json'
            history.write_text(json.dumps(['bir=1;','iki=2;']),encoding='utf-8')
            first=self.make_app(folder).history_detail()['entries']
            self.assertEqual(json.loads(history.read_text())['version'],2)
            second=self.make_app(folder).history_detail()['entries']
            self.assertEqual(first,second)
            self.assertEqual(len({entry['id'] for entry in first}),2)
            self.assertEqual(list(Path(folder).glob('.history-*.tmp')),[])

    def test_review_11_failed_migration_preserves_original(self):
        with tempfile.TemporaryDirectory() as folder:
            history=Path(folder)/'history.json'
            original=b'["safe=1;"]'
            history.write_bytes(original)
            with patch('app.os.replace',side_effect=OSError('injected replace failure')):
                with self.assertRaises(OSError):self.make_app(folder)
            self.assertEqual(history.read_bytes(),original)
            self.assertEqual(list(Path(folder).glob('.history-*.tmp')),[])

    def test_review_9_history_bytes_and_large_commands(self):
        with tempfile.TemporaryDirectory() as folder:
            app=self.make_app(folder)
            # Exact review workload: all 500 near-2M-character commands skip
            # persistence without encoding or rewriting any aggregate history.
            large='x'*1_999_990
            with patch.object(app,'_write_history_locked',wraps=app._write_history_locked) as write:
                for index in range(500):self.assertFalse(app.record(large+str(index)))
                write.assert_not_called()
            self.assertEqual(app.history_codes(),[])
            for index in range(60):
                self.assertTrue(app.record(f'% {index} '+('ğ' * 29000)))
                self.assertLessEqual(app.history_path.stat().st_size,app.HISTORY_TOTAL_BYTES)
            persisted=json.loads(app.history_path.read_bytes())
            self.assertLess(len(persisted['entries']),60)
            self.assertTrue(persisted['entries'][-1]['code'].startswith('% 59 '))
            for entry in persisted['entries']:
                self.assertLessEqual(len(json.dumps(entry,ensure_ascii=False,separators=(',',':')).encode()),app.HISTORY_ENTRY_BYTES)
            before=app.history_path.read_bytes()
            self.assertFalse(app.record('"'*40000),'JSON escaping must count toward the per-entry bound')
            self.assertFalse(app.record('ğ'*40000),'UTF-8 bytes must count toward the per-entry bound')
            self.assertEqual(app.history_path.read_bytes(),before)
            self.assertEqual(self.make_app(folder).history_codes(),app.history_codes())

    def test_review_9_oversized_existing_file_is_bounded_and_recoverable(self):
        with tempfile.TemporaryDirectory() as folder:
            history=Path(folder)/'history.json'
            original=b'['+b' '* (App.HISTORY_LOAD_BYTES+1)+b']'
            history.write_bytes(original)
            app=self.make_app(folder)
            self.assertEqual(app.history_codes(),[])
            app.record('small=1;')
            self.assertLessEqual(history.stat().st_size,app.HISTORY_TOTAL_BYTES)
            recoveries=list(Path(folder).glob('history-recovery-*.json'))
            self.assertEqual(len(recoveries),1)
            self.assertEqual(recoveries[0].read_bytes(),original)

    def test_review_9_history_limit_does_not_truncate_submitted_job(self):
        with tempfile.TemporaryDirectory() as folder:
            app=self.make_app(folder)
            submitted=[]
            app.kernel=SimpleNamespace(submit=lambda *args:(submitted.append(args) or 'identified-job'))
            app.workspace=Workspace(Path(folder),Path(folder))
            handler=Handler.__new__(Handler)
            handler.server=SimpleNamespace(app=app)
            handler.send=lambda status,data:(status,data)
            code='% '+('x'*100000)+'\nanswer=42;'
            self.assertEqual(handler.post('/api/execute',{'code':code,'history':True}),(202,{'job':'identified-job','history_recorded':False}))
            self.assertEqual(submitted[0][0],code)
            self.assertEqual(app.history_codes(),[])

    def test_legacy_load_record_delete_clear_and_persistence(self):
        with tempfile.TemporaryDirectory() as folder:
            history=Path(folder)/'history.json'
            history.write_text(json.dumps(['old=1;','older=2;']),encoding='utf-8')
            app=self.make_app(folder)
            self.assertEqual(app.history_codes(),['old=1;','older=2;'])
            app.record('new=3;\ndisp(new);')
            app.record('new=3;\ndisp(new);')
            detail=app.history_detail()
            self.assertEqual([item['code'] for item in detail['entries']],['old=1;','older=2;','new=3;\ndisp(new);'])
            self.assertEqual(detail['entries'][-1]['session'],app.history_session)
            payload=json.loads(history.read_text(encoding='utf-8'))
            self.assertEqual(payload['version'],2)
            self.assertEqual(len(payload['entries']),3)
            removed=detail['entries'][1]['id']
            self.assertEqual(app.mutate_history({'action':'delete','ids':[removed]})['count'],2)
            reloaded=self.make_app(folder,'fedcba9876543210')
            self.assertEqual(reloaded.history_codes(),['old=1;','new=3;\ndisp(new);'])
            self.assertEqual(reloaded.mutate_history({'action':'clear','confirm':True})['count'],0)
            self.assertEqual(self.make_app(folder).history_codes(),[])
            self.assertEqual(list(Path(folder).glob('.history-*.tmp')),[])

    def test_mutations_are_strict_and_do_not_call_a_kernel(self):
        with tempfile.TemporaryDirectory() as folder:
            app=self.make_app(folder)
            app.record('safe=1;')
            identifier=app.history_detail()['entries'][0]['id']
            for request in [
                {'action':'clear'},
                {'action':'clear','confirm':False},
                {'action':'delete','ids':[]},
                {'action':'delete','ids':['not-an-id']},
                {'action':'delete','ids':[identifier],'code':'disp(1)'},
                {'action':'execute','ids':[identifier]}
            ]:
                with self.assertRaises(ValueError,msg=request):app.mutate_history(request)
            self.assertEqual(app.history_codes(),['safe=1;'])

    def test_routes_mutate_history_and_resolve_only_bounded_m_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder).resolve()
            app=self.make_app(folder)
            app.workspace=Workspace(root,root)
            app.kernel=object()
            target=root/'location.m'
            target.write_text('first=1;\nsecond=2;\n',encoding='utf-8')
            app.record('route=1;')
            identifier=app.history_detail()['entries'][0]['id']
            replies=[]
            handler=Handler.__new__(Handler)
            handler.server=SimpleNamespace(app=app)
            handler.send=lambda status,data,*args:(replies.append((status,data)) or (status,data))
            self.assertEqual(handler.post('/api/error-location',{'path':'location','line':2}),(200,{'path':str(target),'line':2}))
            self.assertEqual(handler.post('/api/error-location',{'path':'location>inner','line':2}),(200,{'path':str(target),'line':2}))
            with self.assertRaises(FileNotFoundError):handler.post('/api/error-location',{'path':'unresolved_frame','line':2})
            (root/'escape.m').symlink_to('/etc/passwd')
            with self.assertRaises(PermissionError):handler.post('/api/error-location',{'path':'escape.m','line':2})
            with self.assertRaises(PermissionError):handler.post('/api/error-location',{'path':'../outside.m','line':1})
            with self.assertRaises(ValueError):handler.post('/api/error-location',{'path':'location.m','line':2,'code':'disp(1)'})
            self.assertEqual(handler.post('/api/history',{'action':'delete','ids':[identifier]}),(200,{'ok':True,'count':0}))
            self.assertEqual(app.history_codes(),[])


class CommandErrorCaptureTests(unittest.TestCase):
    def test_real_octave_caught_errors_match_captured_responses(self):
        project=Path(__file__).resolve().parents[1]
        captured=json.loads((project/'tests/fixtures/command_window_errors.json').read_text())
        with tempfile.TemporaryDirectory(prefix='command-window-errors-') as directory:
            root=Path(directory).resolve()
            work=root/'workspace with spaces';work.mkdir()
            for case in captured['cases']:
                if case['source'] is not None:(work/(case['name']+'.m')).write_text(case['source'],encoding='utf-8')
            kernel=Kernel(project,root/'runtime',work)
            handler=Handler.__new__(Handler)
            handler.server=SimpleNamespace(app=SimpleNamespace(kernel=kernel,workspace=Workspace(work,root)))
            handler.send=lambda status,data:(status,data)
            def wait(job=None):
                deadline=time.monotonic()+30
                while time.monotonic()<deadline:
                    state=kernel.snapshot()
                    if state['status'] in ('idle','dead') and (job is None or state['job']==job):return state
                    time.sleep(.025)
                self.fail('Octave job timed out')
            try:
                self.assertEqual(wait()['status'],'idle')
                generation=kernel.generation
                for case in captured['cases']:
                    with self.subTest(case=case['name']):
                        request={'mode':case['mode'],'code':case['code'],'argument':str(work/(case['name']+'.m')) if case['mode']=='file' else ''}
                        status,response=handler.post('/api/execute',request)
                        self.assertEqual(status,202)
                        state=wait(response['job'])
                        self.assertEqual(state['status'],'idle')
                        self.assertEqual(state['version'],captured['version'])
                        self.assertEqual(state['output'],case['output'])
                        self.assertEqual(state['error'].replace(str(work),'<workspace>'),case['error'])
                        self.assertEqual(kernel.generation,generation)
            finally:kernel.close()



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main()
