import os,tempfile,unittest,sys
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.files import Workspace
class FilesTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.base=Path(self.tmp.name);self.home=self.base/'home';self.ws=Workspace(self.home/'work',self.home)
 def tearDown(self):self.tmp.cleanup()
 def test_roundtrip_conflict(self):
  r=self.ws.save('Türkçe klasör/a.m','x=1;','absent');self.assertEqual(self.ws.read(r['path'])['content'],'x=1;')
  with self.assertRaises(FileExistsError):self.ws.save(r['path'],'x=2;','wrong')
  self.ws.save(r['path'],'x=2;',r['hash']);tree=self.ws.tree();self.assertEqual(len(tree['entries']),1);self.assertTrue(tree['entries'][0]['directory'])
 def test_traversal_and_symlink(self):
  for p in ('../secret.m','/etc/passwd','a\nb.m'):
   with self.assertRaises((PermissionError,ValueError)):self.ws.path(p)
  outside=self.base/'outside';outside.mkdir();(self.ws.root/'escape').symlink_to(outside)
  with self.assertRaises(PermissionError):self.ws.save('escape/a.m','x=1;')
  with self.assertRaises(PermissionError):self.ws.set_current(outside)
 def test_current_folder_and_bounded_listing(self):
  folder=self.ws.root/'ders';folder.mkdir()
  for i in range(12):(folder/f'{i:02}.m').write_text('x=1;')
  (folder/'.hidden.m').write_text('x=2;');(folder/'node_modules').mkdir();(folder/'node_modules'/'ignored.m').write_text('x=3;')
  self.ws.set_current(folder);tree=self.ws.tree(10)
  self.assertEqual(tree['current'],str(folder));self.assertEqual(len(tree['entries']),10);self.assertTrue(tree['truncated'])
  self.assertTrue(all('/node_modules/' not in item['path'] for item in tree['entries']))
  saved=self.ws.save('local.m','answer=42;','absent');self.assertEqual((folder/'local.m').read_text(),'answer=42;');self.assertEqual(saved['path'],'local.m')
 def test_binary_and_extension(self):
  self.ws.save('data.mat',b'\x00\x01',binary=True)
  with self.assertRaises(ValueError):self.ws.read('data.mat')
  with self.assertRaises(ValueError):self.ws.save('bad.py','x')
 def test_external_write_after_hash_check_survives(self):
  saved=self.ws.save('race.m','original=1;','absent');target=self.ws.path('race.m');real_replace=os.replace;injected=False
  def replace_with_race(src,dst):
   nonlocal injected
   if Path(src)==target:
    injected=True;target.write_text('external=2;',encoding='utf-8')
   return real_replace(src,dst)
  with patch('backend.files.os.replace',side_effect=replace_with_race):
   with self.assertRaises(FileExistsError) as conflict:self.ws.save('race.m','editor=3;',saved['hash'])
  self.assertTrue(injected);self.assertEqual(target.read_text(encoding='utf-8'),'external=2;')
  recoveries=list(target.parent.glob('.mf-recovery-race.m-*'));self.assertEqual(len(recoveries),1)
  self.assertEqual(recoveries[0].read_text(encoding='utf-8'),'external=2;');self.assertIn(recoveries[0].name,str(conflict.exception))
 def test_new_file_created_during_save_survives(self):
  target=self.ws.path('new-race.m');real_link=os.link;injected=False
  def link_with_race(src,dst):
   nonlocal injected
   if Path(dst)==target and not injected:
    injected=True;target.write_text('external=1;',encoding='utf-8')
   return real_link(src,dst)
  with patch('backend.files.os.link',side_effect=link_with_race):
   with self.assertRaises(FileExistsError):self.ws.save('new-race.m','editor=2;','absent')
  self.assertTrue(injected);self.assertEqual(target.read_text(encoding='utf-8'),'external=1;')
 def test_existing_file_recreated_after_move_survives(self):
  saved=self.ws.save('recreated.m','original=1;','absent');target=self.ws.path('recreated.m');real_link=os.link;injected=False
  def link_with_race(src,dst):
   nonlocal injected
   if Path(dst)==target and not injected:
    injected=True;target.write_text('external=2;',encoding='utf-8')
   return real_link(src,dst)
  with patch('backend.files.os.link',side_effect=link_with_race):
   with self.assertRaises(FileExistsError) as conflict:self.ws.save('recreated.m','editor=3;',saved['hash'])
  self.assertTrue(injected);self.assertEqual(target.read_text(encoding='utf-8'),'external=2;')
  recoveries=list(target.parent.glob('.mf-recovery-recreated.m-*'));self.assertEqual(len(recoveries),1)
  self.assertEqual(recoveries[0].read_text(encoding='utf-8'),'original=1;');self.assertIn(recoveries[0].name,str(conflict.exception))

def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
