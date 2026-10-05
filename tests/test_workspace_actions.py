import hashlib
import math
import tempfile
import time
import unittest
import os
import threading
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))

from app import Handler,PublishKernel,workspace_scalar,workspace_variable_name,workspace_variable_names,workspace_mat_path
from backend.workspace_actions import WorkspaceActions,MatTarget
from backend.files import Workspace
from backend.kernel import Kernel

ROOT=Path(__file__).resolve().parents[1]

class WorkspaceValidationTests(unittest.TestCase):
    def test_names_reject_code_and_reserved_prefix(self):
        for name in ["x'",'x;y','x\ny','x()','x y','1x','__mf_probe','a'*64,None]:
            with self.assertRaises(ValueError,msg=repr(name)):workspace_variable_name(name)
        self.assertEqual(workspace_variable_name('Alpha_9'),'Alpha_9')
        self.assertEqual(workspace_variable_name('_octave_name'),'_octave_name')
        with self.assertRaises(ValueError):workspace_variable_names(['x','x'])
        with self.assertRaises(ValueError):workspace_variable_names('x')

    def test_scalar_types_are_typed_and_bounded(self):
        self.assertEqual(workspace_scalar('logical',True)['value'],True)
        self.assertEqual(workspace_scalar('char','x')['value'],'x')
        self.assertEqual(workspace_scalar('double',1.25)['value'],1.25)
        self.assertEqual(workspace_scalar('int16',-12)['value'],-12)
        for special in ('NaN','Inf','-Inf'):
            self.assertEqual(workspace_scalar('single',{'special':special})['value'],{'special':special})
        for class_name,value in [('logical',1),('char','xy'),('char','ğ'),('double','1;system("x")'),('double',math.inf),('int8',128),('uint8',-1),('int64',2**60),('single',1e300),('int8',{'special':'NaN'}),('double',{'special':'eval(1)'})]:
            with self.assertRaises(ValueError,msg=(class_name,value)):workspace_scalar(class_name,value)

    def test_mat_paths_are_bounded_and_reject_symlinks(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder).resolve();workspace=Workspace(root,root)
            self.assertEqual(workspace_mat_path(workspace,'safe.mat',False),root/'safe.mat')
            with self.assertRaises(ValueError):workspace_mat_path(workspace,'safe.txt',False)
            with self.assertRaises(PermissionError):workspace_mat_path(workspace,'../escape.mat',False)
            (root/'real').mkdir();(root/'alias').symlink_to(root/'real',target_is_directory=True)
            with self.assertRaises(PermissionError):workspace_mat_path(workspace,'alias/data.mat',False)

class WorkspaceMatRaceTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.root=Path(self.temp.name).resolve()
        self.workspace=Workspace(self.root,self.root)

    def tearDown(self):self.temp.cleanup()

    def test_retained_parent_rejects_swap_to_symlink(self):
        parent=self.root/'parent';parent.mkdir()
        elsewhere=self.root/'elsewhere';elsewhere.mkdir()
        target=MatTarget(self.workspace,str(parent/'value.mat'))
        try:
            parent.rename(self.root/'moved')
            parent.symlink_to(elsewhere,target_is_directory=True)
            with self.assertRaises(PermissionError):target.install(b'new')
            self.assertFalse((elsewhere/'value.mat').exists())
            self.assertFalse((self.root/'moved/value.mat').exists())
        finally:target.close()

    def test_exclusive_install_preserves_recreated_target_and_recovery(self):
        path=self.root/'value.mat';path.write_bytes(b'approved')
        target=MatTarget(self.workspace,str(path))
        rename=os.rename
        def concurrent_create(source,destination,**kwargs):
            rename(source,destination,**kwargs)
            path.write_bytes(b'new arrival')
        try:
            with patch('backend.workspace_actions.os.rename',side_effect=concurrent_create):
                with self.assertRaises(FileExistsError):target.install(b'new saved data')
            self.assertEqual(path.read_bytes(),b'new arrival')
            recovery=list(self.root.glob('.mf-workspace-recovery-*.mat'))
            self.assertEqual(len(recovery),1)
            self.assertEqual(recovery[0].read_bytes(),b'approved')
        finally:target.close()

    def test_same_hash_new_identity_invalidates_approval(self):
        path=self.root/'value.mat';path.write_bytes(b'approved')
        target=MatTarget(self.workspace,str(path))
        try:
            other=self.root/'other';other.write_bytes(b'approved');other.replace(path)
            with self.assertRaises(FileExistsError):target.install(b'new')
            self.assertEqual(path.read_bytes(),b'approved')
        finally:target.close()


class WorkspaceKernelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory();cls.root=Path(cls.temp.name).resolve();cls.runtime=cls.root/'runtime';cls.work=cls.root/'work';cls.work.mkdir()
        cls.app=SimpleNamespace(workspace=Workspace(cls.work,cls.root),file_lock=threading.Lock(),finish_publish=lambda k:None,discard_publish_stages=lambda *args:None)
        cls.app.workspace_actions=WorkspaceActions(cls.app)
        cls.kernel=PublishKernel(ROOT,cls.runtime,cls.work,publish_app=cls.app)
        cls.app.kernel=cls.kernel
        cls.wait()

    @classmethod
    def tearDownClass(cls):
        cls.kernel.close();cls.temp.cleanup()

    @classmethod
    def wait(cls,seconds=30):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            state=cls.kernel.snapshot()
            if state['status'] not in ('starting','running','stopping'):return state
            time.sleep(.05)
        raise AssertionError(cls.kernel.snapshot())

    def action(self,mode,request):
        self.kernel.workspace_action(mode,request);return self.wait()

    def code(self,source):
        self.kernel.submit(source);return self.wait()

    def mat(self,request):
        self.app.workspace_actions.submit(request,workspace_variable_names)
        return self.wait()

    def test_typed_actions_preserve_session(self):
        generation=self.kernel.generation
        self.assertFalse(self.code("workspace_kept=901; scalar_value=1; logical_value=false; char_value='a';")['error'])
        self.assertFalse(self.action('workspace-rename',{'old_name':'scalar_value','new_name':'renamed_value'})['error'])
        self.assertFalse(self.action('workspace-assign-scalar',{'name':'renamed_value','class':'double','value':2.5})['error'])
        self.assertFalse(self.action('workspace-assign-scalar',{'name':'logical_value','class':'logical','value':True})['error'])
        self.assertFalse(self.action('workspace-assign-scalar',{'name':'char_value','class':'char','value':'z'})['error'])
        self.assertFalse(self.code("assert(workspace_kept==901 && renamed_value==2.5 && logical_value && strcmp(char_value,'z'));")['error'])
        self.assertEqual(self.kernel.generation,generation)

    def test_injection_names_never_execute(self):
        self.assertFalse(self.code('workspace_injected=0; workspace_source=3;')['error'])
        for bad in ["workspace_source'; workspace_injected=1; %",'workspace_source;workspace_injected=1','workspace_source\nworkspace_injected=1','workspace_source()']:
            state=self.action('workspace-rename',{'old_name':bad,'new_name':'safe_name'})
            self.assertTrue(state['error'],bad)
        self.assertFalse(self.code('assert(workspace_injected==0 && workspace_source==3);')['error'])

    def test_paused_workspace_is_read_only(self):
        with self.kernel.lock:
            previous=self.kernel.state['status'];self.kernel.state['status']='paused'
            try:
                with self.assertRaisesRegex(ValueError,'salt okunurdur'):
                    self.kernel.workspace_action('workspace-clear-names',{'names':['workspace_kept']})
            finally:self.kernel.state['status']=previous

    def test_save_inspect_load_clear_round_trip(self):
        target=self.work/'roundtrip.mat'
        self.assertFalse(self.code('roundtrip_a=17; roundtrip_b=true;')['error'])
        request={'action':'save','path':str(target),'overwrite':False,'all':False,'names':['roundtrip_a','roundtrip_b']}
        state=self.mat(request)
        self.assertFalse(state['error'],state);self.assertTrue(target.is_file())
        with self.assertRaises(FileExistsError):self.mat(request)
        approval=next(reversed(self.app.workspace_actions.approvals))
        expected=hashlib.sha256(target.read_bytes()).hexdigest()
        self.assertFalse(self.code('roundtrip_a=18;')['error'])
        replaced=self.mat({**request,'overwrite':True,'confirm_overwrite':True,'approval':approval,'expected_hash':expected})
        self.assertFalse(replaced['error'],replaced)
        self.assertFalse(self.code('roundtrip_a=99; clear roundtrip_b;')['error'])
        inspected=self.mat({'action':'load-inspect','path':str(target)})
        self.assertFalse(inspected['error'],inspected)
        info=inspected['workspace_action']
        self.assertEqual(info['replacements'],['roundtrip_a'])
        wrong=self.mat({'action':'load',**info,'action':'load','replacements':[],'confirm':True})
        self.assertIn('farklılaştı',wrong['error'])
        self.assertFalse(self.code('assert(roundtrip_a==99);')['error'])
        info=self.mat({'action':'load-inspect','path':str(target)})['workspace_action']
        loaded=self.mat({**info,'action':'load','confirm':True})
        self.assertFalse(loaded['error'],loaded)
        self.assertFalse(self.code('assert(roundtrip_a==18 && roundtrip_b);')['error'])
        cleared=self.action('workspace-clear-names',{'names':['roundtrip_a','roundtrip_b']})
        self.assertFalse(cleared['error'],cleared)

    def test_shadowed_functions_rename_clear_save_load_no_leaks(self):
        self.assertFalse(self.code("clear=42; save=43; load=44; builtin=45; whos=46; shadow_source=struct('x',7);")['error'])
        try:
            renamed=self.action('workspace-rename',{'old_name':'shadow_source','new_name':'shadow_dest'})
            self.assertFalse(renamed['error'],renamed)
            duplicate=self.action('workspace-rename',{'old_name':'shadow_dest','new_name':'clear'})
            self.assertTrue(duplicate['error'])
            self.assertFalse(self.code("assert(shadow_dest.x==7 && clear==42 && exist('shadow_source','var')==0); assert(exist('__mf_workspace_names__','var')==0 && exist('__mf_workspace_builtin__','var')==0);")['error'])
            path=self.work/'shadow.mat'
            saved=self.mat({'action':'save','path':str(path),'overwrite':False,'all':False,'names':['shadow_dest','clear','save','load']})
            self.assertFalse(saved['error'],saved)
            cleared=self.action('workspace-clear-names',{'names':['shadow_dest']})
            self.assertFalse(cleared['error'],cleared)
            info=self.mat({'action':'load-inspect','path':str(path)})['workspace_action']
            loaded=self.mat({**info,'action':'load','confirm':True})
            self.assertFalse(loaded['error'],loaded)
            self.assertFalse(self.code("assert(shadow_dest.x==7 && clear==42 && save==43 && load==44); assert(exist('__mf_workspace_names__','var')==0);")['error'])
        finally:
            state=self.action('workspace-clear-names',{'names':['clear','save','load','builtin','whos','shadow_dest','shadow_source']})
            self.assertFalse(state['error'],state)

    def test_global_rename_refused_with_binding_and_type_preserved(self):
        self.assertFalse(self.code('global global_probe; global_probe=int8(7);')['error'])
        state=self.action('workspace-rename',{'old_name':'global_probe','new_name':'sin'})
        self.assertIn('Global',state['error'])
        item=next(v for v in state['variables'] if v['name']=='global_probe')
        self.assertTrue(item['global']);self.assertEqual(item['class'],'int8')
        self.assertFalse(self.code("assert(global_probe==int8(7) && exist('sin','var')==0);")['error'])
        self.code('clear global global_probe;')

    def test_rename_rolls_back_if_source_clear_fails(self):
        folder=self.work/'clear-failure';folder.mkdir()
        helper=(ROOT/'octave/__mf_workspace_base__.m').read_text().replace('__mf_workspace_base__','__mf_ws_real_base__')
        (folder/'__mf_ws_real_base__.m').write_text(helper)
        (folder/'__mf_workspace_base__.m').write_text("function items=__mf_workspace_base__(operation,names={})\n persistent failed=false;\n if strcmp(operation,'clear') && any(strcmp(names,'rollback_source')) && ~failed\n failed=true; error('injected clear failure');\n endif\n items=__mf_ws_real_base__(operation,names);\nendfunction\n")
        path=self.kernel.quote(folder)
        state=self.code(f"rollback_source=struct('x',9); addpath({path},'-begin'); try; __mf_workspace__('workspace-rename',jsonencode(struct('old_name','rollback_source','new_name','rollback_dest'))); error('missing failure'); catch err; assert(strcmp(err.message,'injected clear failure')); end; rmpath({path}); assert(rollback_source.x==9 && exist('rollback_dest','var')==0); assert(exist('__mf_workspace_names__','var')==0 && exist('__mf_workspace_builtin__','var')==0);")
        self.assertFalse(state['error'],state)

    def test_live_scalar_class_shape_ranges_and_special_values(self):
        self.assertFalse(self.code("typed_i=int8(7); typed_s=single(2); typed_d=NaN; typed_c='x'; typed_matrix=[1,2];")['error'])
        for request in [
            {'name':'typed_i','class':'double','value':2},
            {'name':'typed_i','class':'int8','value':200},
            {'name':'typed_s','class':'single','value':1e300},
            {'name':'typed_matrix','class':'double','value':1},
            {'name':'typed_c','class':'char','value':'ğ'},
        ]:
            self.assertTrue(self.action('workspace-assign-scalar',request)['error'],request)
        self.assertFalse(self.code("assert(isa(typed_i,'int8') && typed_i==7 && typed_s==single(2) && strcmp(typed_c,'x') && numel(typed_matrix)==2);")['error'])
        for name,kind in [('typed_s','single'),('typed_d','double')]:
            for special in ['NaN','Inf','-Inf']:
                state=self.action('workspace-assign-scalar',{'name':name,'class':kind,'value':{'special':special}})
                self.assertFalse(state['error'],state)
                check='isnan' if special=='NaN' else 'isinf'
                self.assertFalse(self.code(f"assert({check}({name}) && isa({name},'{kind}'));")['error'])

    def test_load_uses_approved_copy_after_original_is_replaced(self):
        path=self.work/'approved.mat'
        self.code('approved_value=17;')
        self.assertFalse(self.mat({'action':'save','path':str(path),'all':False,'names':['approved_value'],'overwrite':False})['error'])
        info=self.mat({'action':'load-inspect','path':str(path)})['workspace_action']
        replacement=self.work/'unapproved.mat'
        self.code('unapproved_value=71;')
        self.mat({'action':'save','path':str(replacement),'all':False,'names':['unapproved_value'],'overwrite':False})
        with self.app.file_lock:
            self.app.workspace.save(str(path),replacement.read_bytes(),expected=info['hash'],binary=True)
        self.code('approved_value=99; clear unapproved_value;')
        state=self.mat({**info,'action':'load','confirm':True})
        self.assertFalse(state['error'],state)
        self.assertFalse(self.code("assert(approved_value==17 && exist('unapproved_value','var')==0);")['error'])

    def test_private_copy_tamper_and_inventory_mismatch_fail_before_assignment(self):
        path=self.work/'inventory.mat'
        self.code('inventory_value=19;')
        self.mat({'action':'save','path':str(path),'all':False,'names':['inventory_value'],'overwrite':False})
        info=self.mat({'action':'load-inspect','path':str(path)})['workspace_action']
        ticket=self.app.workspace_actions.inspections[info['inspection']]
        ticket['inventory'][0]['name']='unexpected'
        self.code('inventory_value=99;')
        state=self.mat({**info,'action':'load','confirm':True})
        self.assertIn('listesi',state['error'])
        self.assertFalse(self.code('assert(inventory_value==99);')['error'])
        info=self.mat({'action':'load-inspect','path':str(path)})['workspace_action']
        ticket=self.app.workspace_actions.inspections[info['inspection']]
        os.chmod(ticket['file'],0o600)
        ticket['file'].write_bytes(b'swap')
        with self.assertRaises(FileExistsError):self.mat({**info,'action':'load','confirm':True})

    def test_other_tab_cannot_modify_private_mat_stage(self):
        path=self.work/'private.mat'
        self.code('private_value=23;')
        self.mat({'action':'save','path':str(path),'all':False,'names':['private_value'],'overwrite':False})
        info=self.mat({'action':'load-inspect','path':str(path)})['workspace_action']
        ticket=self.app.workspace_actions.inspections[info['inspection']]
        with self.app.file_lock:
            with self.assertRaises(PermissionError):
                self.app.workspace.save(str(ticket['file']),b'replacement',expected=info['hash'],binary=True)
            with self.assertRaises(PermissionError):self.app.workspace.folder(str(ticket['stage']))
            with self.assertRaises(PermissionError):MatTarget(self.app.workspace,str(ticket['file']))
        self.assertFalse(self.mat({**info,'action':'load','confirm':True})['error'])

    def test_save_conflicts_with_application_file_write_during_job(self):
        target=self.work/'concurrent.mat'
        self.code('concurrent_value=3;')
        with self.kernel.lock:
            self.app.workspace_actions.submit({'action':'save','path':str(target),'all':False,'names':['concurrent_value'],'overwrite':False},workspace_variable_names)
            with self.app.file_lock:
                self.app.workspace.save(str(target),b'created by another tab',binary=True)
        state=self.wait()
        self.assertIn('değişti',state['error'])
        self.assertEqual(target.read_bytes(),b'created by another tab')

    def test_route_confirmation_is_bound_to_inspection_and_epoch(self):
        handler=Handler.__new__(Handler)
        handler.server=SimpleNamespace(app=self.app)
        replies=[]
        handler.send=lambda status,data:replies.append((status,data))
        self.code('route_value=5;')
        path=self.work/'route.mat'
        handler.post('/api/workspace',{'action':'save','path':str(path),'all':False,'names':['route_value'],'overwrite':False})
        self.assertEqual(replies.pop()[0],202)
        self.assertFalse(self.wait()['error'])
        handler.post('/api/workspace',{'action':'load-inspect','path':str(path)})
        info=self.wait()['workspace_action']
        self.assertEqual(info['path'],str(path))
        self.assertNotIn('workspace-',info['path'])
        with self.assertRaisesRegex(ValueError,'onay'):
            handler.post('/api/workspace',{**info,'action':'load'})
        handler.post('/api/workspace',{**info,'action':'load','confirm':True})
        self.assertFalse(self.wait()['error'])
        with self.assertRaisesRegex(ValueError,'geçersiz'):
            handler.post('/api/workspace',{**info,'action':'load','confirm':True})
        info=self.mat({'action':'load-inspect','path':str(path)})['workspace_action']
        ticket=self.app.workspace_actions.inspections[info['inspection']]
        ticket['generation']-=1
        with self.assertRaisesRegex(ValueError,'geçersiz'):
            self.mat({**info,'action':'load','confirm':True})

    def test_busy_request_cannot_prepare_files_or_interleave_completion(self):
        self.code('busy_value=6;')
        target=self.work/'busy.mat'
        with self.kernel.lock:
            first=self.app.workspace_actions.submit({'action':'save','path':str(target),'all':False,'names':['busy_value'],'overwrite':False},workspace_variable_names)
            before=set((self.kernel.runtime.parent/'.workspace-stages').glob('workspace-*'))
            with self.assertRaisesRegex(ValueError,'işlemi bitirin'):
                self.app.workspace_actions.submit({'action':'load-inspect','path':str(target)},workspace_variable_names)
            self.assertEqual(before,set((self.kernel.runtime.parent/'.workspace-stages').glob('workspace-*')))
        state=self.wait()
        self.assertEqual(state['job'],first)
        self.assertFalse(state['error'],state)
        self.assertTrue(target.is_file(),'idle must not be exposed before installation')

    def test_metadata_survives_user_path_restore(self):
        state=self.code('clear all; restoredefaultpath; restored_path_value=83;')
        self.assertFalse(state['error'],state)
        self.assertTrue(any(item['name']=='restored_path_value' for item in state['variables']),state)

    def test_breakpoint_job_closes_mat_completion_gap(self):
        self.code('completion_value=8;')
        target=self.work/'completion.mat'
        script=self.work/'completion_breakpoint.m'
        script.write_text('completion_a=1;\ncompletion_b=2;\n')
        terminal=threading.Event()
        release=threading.Event()
        def delayed_collector(*args):
            Kernel._collect(self.kernel,*args)
            terminal.set()
            release.wait(10)
        try:
            with patch.object(self.kernel,'_collect',side_effect=delayed_collector):
                self.app.workspace_actions.submit({'action':'save','path':str(target),'all':False,'names':['completion_value'],'overwrite':False},workspace_variable_names)
                self.assertTrue(terminal.wait(10))
                self.assertFalse(target.exists())
                self.kernel.breakpoint(script,2,True)
                self.assertTrue(target.exists(),'direct _submit must finalize preceding MAT save')
                release.set()
                self.assertFalse(self.wait()['error'])
        finally:
            release.set()
            self.kernel.breakpoint(script,2,False)
            self.wait()


def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main()
