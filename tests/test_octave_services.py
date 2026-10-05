import shutil,sys,tarfile,tempfile,time,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.octave_services import OctaveServices
ROOT=Path(__file__).resolve().parents[1]

class OctaveServicesTests(unittest.TestCase):
 def test_help_core_package_user_and_missing(self):
  with tempfile.TemporaryDirectory() as runtime,tempfile.TemporaryDirectory() as current:
   current=Path(current);(current/'indymat_user_help.m').write_text('function y=indymat_user_help(x)\n% INDYMAT_USER_HELP Kullanıcı yardım metni.\n% Girdiyi geri döndürür.\ny=x;\nendfunction\n')
   service=OctaveServices(ROOT,runtime,shutil.which('octave'))
   core=service.help('fft',current);self.assertTrue(core['found']);self.assertEqual(core['origin'],'core');self.assertIn('Fourier',core['text'])
   package=service.help('butter',current);self.assertTrue(package['found']);self.assertEqual(package['origin'],'package');self.assertIn('signal',package['origin_label'])
   user=service.help('indymat_user_help',current);self.assertTrue(user['found']);self.assertEqual(user['origin'],'user');self.assertIn('Kullanıcı yardım metni',user['text'])
   missing=service.help('indymat_missing_help',current);self.assertFalse(missing['found']);self.assertEqual(missing['message'],'indymat_missing_help bulunamadı.')
   service.close()
 def test_local_package_install_and_uninstall_stays_in_prefix(self):
  with tempfile.TemporaryDirectory() as folder:
   root=Path(folder)/'project';workspace=root/'workspace';workspace.mkdir(parents=True);source=Path(folder)/'indymattest-1.0.0';(source/'inst').mkdir(parents=True)
   (source/'DESCRIPTION').write_text('Name: indymattest\nVersion: 1.0.0\nDate: 2026-10-03\nTitle: IndyMAT test package\nAuthor: Test\nMaintainer: Test <test@example.com>\nDescription: Local package install test.\nCategories: Miscellaneous\nLicense: MIT\n')
   (source/'COPYING').write_text('MIT\n');(source/'inst'/'indymat_pkg_probe.m').write_text('function y=indymat_pkg_probe(); y=123; endfunction\n')
   archive=workspace/'indymattest-1.0.0.tar.gz'
   with tarfile.open(archive,'w:gz') as bundle:bundle.add(source,arcname=source.name)
   service=OctaveServices(root,root/'runtime',shutil.which('octave'))
   installed=self.wait(service,service.start('install','indymattest',archive));self.assertEqual(installed['status'],'done',installed)
   self.assertEqual(service.packages([]),[{'name':'indymattest','version':'1.0.0','loaded':False}])
   package_dir=root/'.packages'/'indymattest-1.0.0';self.assertTrue(package_dir.is_dir());self.assertTrue(package_dir.is_relative_to(root/'.packages'))
   removed=self.wait(service,service.start('uninstall','indymattest'));self.assertEqual(removed['status'],'done',removed);self.assertEqual(service.packages([]),[])
   service.close()
 @staticmethod
 def wait(service,job):
  for _ in range(300):
   state=service.status(job)
   if state['status']!='running':return state
   time.sleep(.05)
  raise AssertionError('package job timeout')


def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
