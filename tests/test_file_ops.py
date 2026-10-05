import errno, json, os, tempfile, unittest, sys, hashlib, threading
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.files import Workspace
from backend.file_operations import FileOperations, exclusive_rename
from backend.kernel import Kernel
from app import Handler
from types import SimpleNamespace

class FileOperationTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.base=Path(self.tmp.name).resolve();self.home=self.base/'home';(self.home/'.Trash').mkdir(parents=True);self.home=self.home.resolve();self.work=self.home/'work';self.trash=self.home/'.Trash';self.ws=Workspace(self.work,self.home)
 def tearDown(self):self.tmp.cleanup()
 def op(self,operation,**fields):return self.ws.operate({'operation':operation,**fields},str(self.work))
 def test_create_rename_duplicate_and_move_without_overwrite(self):
  made=self.op('create_file',source=str(self.work),name='deney.m');self.assertEqual(Path(made['path']).read_bytes(),b'');self.assertEqual(len(made['hash']),64)
  renamed=self.op('rename',source=made['path'],name='ölçüm.m');self.assertFalse(Path(made['path']).exists());self.assertEqual(Path(renamed['path']).name,'ölçüm.m');self.assertEqual(renamed['hash'],made['hash'])
  copied=self.op('duplicate',source=renamed['path'],name='ölçüm kopya.m');self.assertEqual(Path(copied['path']).read_bytes(),b'')
  target=self.work/'hedef';target.mkdir();moved=self.op('move',source=copied['path'],destination=str(target));self.assertEqual(Path(moved['path']).parent,target.resolve())
  (self.work/'çakışma.m').write_text('ilk');(self.work/'kaynak.m').write_text('ikinci')
  with self.assertRaisesRegex(FileExistsError,'çakışma.m'):self.op('rename',source=str(self.work/'kaynak.m'),name='çakışma.m')
  self.assertEqual((self.work/'çakışma.m').read_text(),'ilk');self.assertEqual((self.work/'kaynak.m').read_text(),'ikinci')
 def test_path_escape_and_symlinks_are_rejected(self):
  source=self.work/'a.m';source.write_text('x=1;');outside=self.base/'outside';outside.mkdir();(outside/'x.m').write_text('x')
  for value in ('../outside/x.m',str(outside/'x.m')):
   with self.assertRaises(PermissionError):self.op('inspect',source=value)
  (self.work/'source-link').symlink_to(source)
  with self.assertRaises(PermissionError):self.op('inspect',source=str(self.work/'source-link'))
  (self.work/'middle').symlink_to(outside,target_is_directory=True)
  with self.assertRaises(PermissionError):self.op('inspect',source=str(self.work/'middle'/'x.m'))
  (self.work/'destination-link').symlink_to(self.work,target_is_directory=True)
  with self.assertRaises(PermissionError):self.op('move',source=str(source),destination=str(self.work/'destination-link'))
 def test_invalid_names_and_exact_schemas(self):
  source=self.work/'a.m';source.write_text('x')
  for name in ('','.','..','...','a/b','a\\b','x\x00y','x'*256):
   with self.assertRaises(ValueError):self.op('rename',source=str(source),name=name)
  with self.assertRaises(ValueError):self.ws.operate({'operation':'rename','source':str(source),'name':'b.m','extra':True})
  with self.assertRaises(ValueError):self.ws.operate({'operation':'unknown','source':str(source)})
 def test_nonempty_folder_operations_report_bounded_count(self):
  folder=self.work/'veri';folder.mkdir();(folder/'a.txt').write_text('a');sub=folder/'alt';sub.mkdir();(sub/'b.txt').write_text('b')
  info=self.op('inspect',source=str(folder));self.assertTrue(info['directory']);self.assertEqual(info['item_count'],3);self.assertFalse(info['count_truncated'])
  duplicated=self.op('duplicate',source=str(folder),name='veri kopya');self.assertEqual(duplicated['item_count'],3);self.assertEqual((Path(duplicated['path'])/'alt'/'b.txt').read_text(),'b')
  renamed=self.op('rename',source=str(folder),name='veri2');self.assertEqual(renamed['item_count'],3)
  target=self.work/'hedef';target.mkdir();moved=self.op('move',source=renamed['path'],destination=str(target));self.assertEqual(moved['item_count'],3)
  trashed=self.op('trash',source=moved['path']);self.assertEqual(trashed['item_count'],3);self.assertTrue(Path(trashed['path']).is_dir())
 def test_duplicate_folder_rejects_nested_symlink(self):
  folder=self.work/'veri';folder.mkdir();(folder/'a.txt').write_text('a');(folder/'link').symlink_to(self.base)
  with self.assertRaises(PermissionError):self.op('duplicate',source=str(folder),name='veri kopya')
  self.assertFalse((self.work/'veri kopya').exists())
 def test_cross_device_rename_is_refused_and_source_survives(self):
  source=self.work/'a.m';source.write_text('x')
  with patch('backend.file_operations.exclusive_rename',side_effect=OSError(errno.EXDEV,'cross-device')):
   with self.assertRaisesRegex(ValueError,'Farklı bir disk'):self.op('rename',source=str(source),name='b.m')
  self.assertTrue(source.exists());self.assertFalse((self.work/'b.m').exists())
 def test_trash_collision_and_unavailable_trash(self):
  source=self.work/'sonuç.m';source.write_text('yeni');(self.trash/'sonuç.m').write_text('eski')
  result=self.op('trash',source=str(source));self.assertEqual(Path(result['path']).name,'sonuç 2.m');self.assertEqual((self.trash/'sonuç.m').read_text(),'eski');self.assertEqual(Path(result['path']).read_text(),'yeni')
  self.trash.rmdir() if not any(self.trash.iterdir()) else None
  other=self.work/'other.m';other.write_text('x')
  for item in list(self.trash.iterdir()):item.unlink()
  self.trash.rmdir()
  with self.assertRaisesRegex(ValueError,'Çöp klasörü kullanılamıyor'):self.op('trash',source=str(other))
  self.assertTrue(other.exists())
 def test_session_folder_and_parent_cannot_be_moved(self):
  current=self.work/'aktif';current.mkdir();child=current/'alt';child.mkdir()
  for source in (current,child):
   with self.assertRaisesRegex(PermissionError,'Octave oturumunun'):self.ws.operate({'operation':'trash','source':str(source)},str(child))
  self.assertTrue(current.exists())

 def test_1_stale_hash_blocks_file_and_folder_relocation(self):
  folder=self.work/'folder';folder.mkdir();source=folder/'a.m';source.write_text('A')
  digest=hashlib.sha256(b'A').hexdigest();source.write_text('B')
  for operation,fields in [('rename',{'name':'new.m'}),('move',{'destination':str(self.work)})]:
   with self.assertRaisesRegex(FileExistsError,'dışarıdan değişti'):
    self.op(operation,source=str(source),expected=[{'path':str(source),'hash':digest}],**fields)
  with self.assertRaises(FileExistsError):
   self.op('rename',source=str(folder),name='other',expected=[{'path':str(source),'hash':digest}])
  self.assertEqual(source.read_text(),'B');self.assertFalse((self.work/'other').exists())
  with self.assertRaises(FileExistsError):self.ws.save(str(source),'stale edit',digest)

 def test_2_competitor_created_at_rename_is_never_overwritten(self):
  source=self.work/'a.m';source.write_text('source')
  def raced(src_fd,src,dst_fd,dst):
   fd=os.open(dst,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600,dir_fd=dst_fd)
   os.write(fd,b'competitor');os.close(fd)
   return exclusive_rename(src_fd,src,dst_fd,dst)
  with patch('backend.file_operations.exclusive_rename',side_effect=raced):
   with self.assertRaises(FileExistsError):self.op('rename',source=str(source),name='b.m')
  self.assertEqual(source.read_text(),'source');self.assertEqual((self.work/'b.m').read_text(),'competitor')

 def test_2_parent_swapped_after_open_cannot_redirect_mutation(self):
  for operation in ('create_file','create_folder','move','duplicate'):
   with self.subTest(operation=operation):
    folder=self.work/operation;folder.mkdir()
    outside=self.base/('outside-'+operation);outside.mkdir()
    held=self.work/(operation+'-held')
    source=folder/'a.m';source.write_text('source')
    real_open=os.open;swapped=False
    def raced_open(name,flags,*args,**kwargs):
     nonlocal swapped
     fd=real_open(name,flags,*args,**kwargs)
     if name==operation and flags & os.O_DIRECTORY and not swapped:
      swapped=True;folder.rename(held);folder.symlink_to(outside,target_is_directory=True)
     return fd
    with patch('backend.file_operations.os.open',side_effect=raced_open):
     try:
      if operation.startswith('create_'):self.op(operation,source=str(folder),name='created.m')
      elif operation=='move':self.op('move',source=str(self.work/'move-held'/'a.m') if swapped else str(folder/'a.m'),destination=str(self.work))
      else:self.op('duplicate',source=str(source),name='copy.m')
     except (PermissionError,FileNotFoundError):pass
    self.assertTrue(swapped);self.assertEqual(list(outside.iterdir()),[])

 def test_10_case_only_rename_and_rollback(self):
  source=self.work/'Case.m';source.write_text('A')
  insensitive=(self.work/'case.m').exists()
  result=self.op('rename',source=str(source),name='case.m')
  self.assertEqual(Path(result['path']).read_text(),'A')
  self.assertIn('case.m',os.listdir(self.work));self.assertNotIn('Case.m',os.listdir(self.work))
  folder=self.work/'Folder';folder.mkdir();(folder/'child.m').write_text('child')
  changed=self.op('rename',source=str(folder),name='folder')
  self.assertIn('folder',os.listdir(self.work));self.assertNotIn('Folder',os.listdir(self.work))
  self.assertEqual((Path(changed['path'])/'child.m').read_text(),'child')
  if insensitive:
   calls=[]
   def fail_install(sfd,src,dfd,dst):
    calls.append((src,dst))
    if len(calls)==2:raise OSError(errno.EIO,'injected install failure')
    return exclusive_rename(sfd,src,dfd,dst)
   with patch('backend.file_operations.exclusive_rename',side_effect=fail_install):
    with self.assertRaises(OSError):self.op('rename',source=result['path'],name='CASE.m')
   self.assertEqual((self.work/'case.m').read_text(),'A');self.assertEqual(len(calls),3)

 def test_10_distinct_casefold_collision_is_preserved(self):
  source=self.work/'Case.m';source.write_text('A')
  if (self.work/'case.m').exists():return
  (self.work/'case.m').write_text('B')
  with self.assertRaises(FileExistsError):self.op('rename',source=str(source),name='case.m')
  self.assertEqual(source.read_text(),'A');self.assertEqual((self.work/'case.m').read_text(),'B')

 def test_14_invisible_names_refused_without_mutation(self):
  source=self.work/'a.m';source.write_text('A')
  for name in ('.hidden.m','tab\t.m','escape\x1b.m','del\x7f.m','format\u200b.m','control\u0085.m','node_modules','Library','__pycache__'):
   with self.subTest(name=repr(name)):
    with self.assertRaises(ValueError):self.op('rename',source=str(source),name=name)
  self.assertEqual(list(self.work.iterdir()),[source])

 def test_5_route_holds_kernel_lock_and_refuses_all_nonidle_states(self):
  folder=self.work/'folder';folder.mkdir()
  lock=threading.RLock()
  kernel=SimpleNamespace(lock=lock,state={'status':'idle','cwd':str(self.work)})
  kernel.snapshot=lambda:dict(kernel.state)
  calls=[]
  def relocate(source,destination):
   self.assertTrue(lock._is_owned())
   self.assertFalse(Path(source).exists())
   self.assertTrue(Path(destination).exists())
   calls.append((source,destination))
  kernel.relocate_file_breakpoints=relocate
  handler=object.__new__(Handler)
  handler.server=SimpleNamespace(app=SimpleNamespace(kernel=kernel,workspace=self.ws,file_lock=threading.Lock()))
  handler.send=lambda status,data:(status,data)
  for status,waiting in [('running',False),('paused',False),('running',True),('idle',True),('starting',False),('stopping',False)]:
   kernel.state.update(status=status,waiting_input=waiting)
   for operation,fields in [('rename',{'name':'new'}),('move',{'destination':str(self.home)}),('trash',{})]:
    with self.assertRaisesRegex(ValueError,'Octave işlemini bitirin'):handler.post('/api/file-operation',dict(operation=operation,source=str(folder),**fields))
   self.assertTrue(folder.exists())
  kernel.state.update(status='idle',waiting_input=False)
  original=self.ws.operate
  def checked(*args):
   self.assertTrue(lock._is_owned())
   return original(*args)
  with patch.object(self.ws,'operate',side_effect=checked):
   status,result=handler.post('/api/file-operation',dict(operation='rename',source=str(folder),name='new'))
  self.assertEqual(status,200);self.assertTrue(Path(result['path']).exists())
  self.assertEqual(calls,[(str(folder),str(self.work/'new'))])
  (self.work/'collision').mkdir()
  with self.assertRaises(FileExistsError):
   handler.post('/api/file-operation',dict(operation='rename',source=result['path'],name='collision'))
  self.assertEqual(len(calls),1)

 def test_9_kernel_prefix_rekey_removal_and_normal_breakpoint_job(self):
  kernel=object.__new__(Kernel);kernel.root=Path(__file__).resolve().parents[1];kernel.lock=threading.RLock();kernel.state={'status':'idle'};kernel.run_to_cursor_state=None;kernel.breakpoint_jobs={}
  source=str(self.work/'old');destination=str(self.work/'new')
  enabled={'enabled':True,'condition':'value == 2'};disabled={'enabled':False,'condition':'value > 10'}
  kernel.breakpoints={
   source+'/a.m':{2:dict(enabled),4:dict(disabled)},
   source+'/sub/b.m':{3:{'enabled':True,'condition':'iteration == 3'}},
   source+'ish/c.m':{1:{'enabled':True,'condition':''}},
  }
  jobs=[]
  kernel._submit=lambda *args,**kwargs:jobs.append((args,kwargs)) or f'breakpoint-job-{len(jobs)}'
  # Targets that do not exist are never kept in the map: they are created here.
  for name in ('old/renamed.m','new/renamed.m','new/sub/b.m'):
   target=self.work/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text('x=1;\n')
  renamed=source+'/renamed.m'
  self.assertEqual(kernel.relocate_file_breakpoints(source+'/a.m',renamed),'breakpoint-job-1')
  self.assertEqual(kernel.breakpoints[renamed],{2:enabled,4:disabled})
  args,kwargs=jobs[-1];self.assertEqual(args[1],'breakpoint');self.assertFalse(kwargs['apply_breakpoints'])
  ops=json.loads(args[2])['ops'];code=' '.join(op['code'] for op in ops)
  self.assertIn('__mf_breakpoint_clear_moved__',code);self.assertIn("dbstop('renamed'",code);self.assertIn(str(Path(renamed).parent),code)
  self.assertIn('value == 2',code);self.assertNotIn('value > 10',code)
  self.assertTrue(kernel.breakpoint_jobs['breakpoint-job-1']['relocation'])
  self.assertEqual(kernel.relocate_file_breakpoints(source,destination),'breakpoint-job-2')
  self.assertEqual(kernel.breakpoints[destination+'/renamed.m'],{2:enabled,4:disabled})
  self.assertEqual(kernel.breakpoints[destination+'/sub/b.m'][3]['condition'],'iteration == 3')
  self.assertEqual(kernel.breakpoints[source+'ish/c.m'][1]['condition'],'')
  kernel.relocate_file_breakpoints(destination,None)
  self.assertEqual(kernel.breakpoints,{source+'ish/c.m':{1:{'enabled':True,'condition':''}}})
  kernel.run_to_cursor_state={'file':source+'ish/c.m'}
  with self.assertRaises(AssertionError):kernel.relocate_file_breakpoints(source+'ish',None)
  kernel.run_to_cursor_state=None
  kernel.state['status']='paused'
  with self.assertRaises(ValueError):kernel.relocate_file_breakpoints(source,None)


def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
