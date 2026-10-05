import base64,json,sys,time,tempfile,unittest,threading
from contextlib import contextmanager
from html.parser import HTMLParser
from types import SimpleNamespace
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.kernel import Kernel
from backend.files import Workspace
from app import App,Handler,PublishKernel
ROOT=Path(__file__).resolve().parents[1]
class KernelTests(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.temp=tempfile.TemporaryDirectory();cls.k=Kernel(ROOT,Path(cls.temp.name)/'runtime',ROOT/'workspace');cls.wait()
 @classmethod
 def tearDownClass(cls):cls.k.close();cls.temp.cleanup()
 @classmethod
 def wait(cls,seconds=40):
  deadline=time.monotonic()+seconds
  while time.monotonic()<deadline:
   s=cls.k.snapshot()
   if s['status'] not in ('starting','running','stopping'):return s
   time.sleep(.05)
  raise AssertionError('job timeout')
 def run_code(self,code):self.k.submit(code);return self.wait()
 def test_01_persistent(self):
  s=self.run_code('x=42; A=[2 1;1 3]; b=[5;7]; z=A\\b; assert(norm(A*z-b)<1e-12); f=@(x)x.^2;');self.assertFalse(s['error'],s)
  s=self.run_code('assert(x==42); assert(f(3)==9); disp("başarılı İğş");');self.assertIn('başarılı İğş',s['output'])
 def test_02_error_recovery(self):
  self.assertIn('deliberate',self.run_code("error('deliberate');")['error']);self.assertFalse(self.run_code('assert(x==42);')['error'])
 def test_03_clear(self):
  s=self.run_code('clear all; restoredefaultpath; fresh=9;');self.assertFalse(s['error'],s)
  self.assertFalse(self.run_code('assert(fresh==9);')['error'])
 def test_04_inspect(self):
  self.run_code('C=[1+2i NaN;Inf -Inf];');self.k.submit(mode='inspect',argument='C');s=self.wait();self.assertIn('i',s['detail']['rows'][0][0])
 def test_05_interrupt(self):
  self.k.submit('kept=77; while true; end;');time.sleep(.4);self.k.interrupt();s=self.wait(10);self.assertEqual(s['status'],'idle',s)
  self.assertFalse(self.run_code('assert(kept==77);')['error'])
 def test_06_input(self):
  self.k.submit("v=input('Sayı: '); assert(v==17); disp(v);");
  for _ in range(100):
   if self.k.snapshot().get('waiting_input'):break
   time.sleep(.05)
  self.assertTrue(self.k.snapshot().get('waiting_input'),self.k.snapshot());self.k.input('17');s=self.wait();self.assertFalse(s['error'],s)
 def test_07_graphics(self):
  s=self.run_code("close all; figure(1); plot(1:5); title('Ağırlık İıĞğŞş');");self.assertFalse(s['error'],s);self.assertEqual(len(s['figures']),1,s)
  s=self.run_code('hold on; plot(5:-1:1);');self.assertEqual(len(s['figures']),1,s)
  self.run_code('close all;')
 def test_08_exit_reset(self):
  s=self.run_code('quit(3);');self.assertEqual(s['status'],'dead');self.k.reset();s=self.wait();self.assertEqual(s['status'],'idle');self.assertFalse(self.run_code('a=1;')['error'])
 def test_09_ans_compat(self):
  self.run_code('42');self.assertFalse(self.run_code('assert(ans==42);')['error'])
  s=self.run_code("close all; h=histogram([0 .5 1 1.5 2],[0 1 2],'Normalization','pdf'); assert(norm(get(h,'ydata')(:)-[.4;.6])<1e-12); sgtitle('İstatistik');")
  self.assertFalse(s['error'],s);self.assertEqual(len(s['figures']),1);self.run_code('close all;')
 def test_10_flood(self):
  s=self.run_code("printf('%s',repmat('x',1,1500000));");self.assertEqual(s['status'],'idle');self.assertLess(len(s['output']),1100000)
 def test_11_input_guard(self):
  self.k.submit('pause(2);');time.sleep(.2)
  with self.assertRaises(ValueError):self.k.input('clear')
  self.k.interrupt();self.wait()
 def test_12_interrupt_figures(self):
  s=self.run_code('for i=1:4; figure(i); plot(1:10); end; kept=91;');self.assertFalse(s['error'],s)
  self.k.submit('while true; end;');time.sleep(.3);self.k.interrupt();s=self.wait(10);self.assertEqual(s['status'],'idle',s)
  self.assertFalse(self.run_code('assert(kept==91); close all;')['error'])
 def test_13_rotate_preserves_variables(self):
  self.run_code('az=77; el=88; figure(1); surf(peaks(20));')
  self.k.submit(mode='rotate',argument='{"figure":1,"angle":15}');s=self.wait();self.assertFalse(s['error'],s)
  self.assertFalse(self.run_code("assert(az==77&&el==88); assert(strcmp(get(1,'visible'),'off')); close all;")['error'])
 def test_14_packages(self):
  s=self.run_code("pkg load control signal datatypes; s=tf('s'); G=1/(s+1); assert(abs(dcgain(G)-1)<1e-12); [b,a]=butter(3,.2); y=filter(b,a,ones(1,30)); assert(abs(y(end)-1)<.01); T=table([1;2],[3;4]); assert(height(T)==2); labels=string({'a','b'}); stamp=datetime('now');")
  self.assertFalse(s['error'],s)
 def test_15_example_suite(self):
  for name in ('matrisler','yuzey_3d','diferansiyel_denklem','kontrol_sistemi','filtre_tasarimi','veri_analizi','ssiodev'):
   self.k.submit(mode='file',argument=str(ROOT/'workspace/examples'/f'{name}.m'));s=self.wait(60);self.assertFalse(s['error'],f'{name}: {s}');self.assertTrue(s['figures'],name)
  self.run_code('close all;')
 def test_16_reset_stale(self):
  self.k.submit("pause(3); stale=1;");time.sleep(.2);self.k.reset();s=self.wait();self.assertEqual(s['status'],'idle');time.sleep(.3)
  self.assertFalse(self.run_code("assert(exist('stale','var')==0);")['error'])
 def test_17_timeout(self):
  self.k.submit('kept=55; while true; end;',timeout=.3);s=self.wait(10);self.assertEqual(s['status'],'idle',s);self.assertIn('süre sınırı',s['error'])
  self.assertFalse(self.run_code('assert(kept==55);')['error'])
 def test_18_interrupt_input_and_pause(self):
  for code in ("kept=44; x=input('Sayı: ');",'kept=44; pause;'):
   self.k.submit(code)
   for _ in range(100):
    if self.k.snapshot().get('waiting_input'):break
    time.sleep(.05)
   self.assertTrue(self.k.snapshot().get('waiting_input'),self.k.snapshot());self.k.interrupt();time.sleep(.3);self.k.interrupt()
   s=self.wait(10);self.assertEqual(s['status'],'idle',s);self.assertFalse(self.run_code('assert(kept==44);')['error'])
 def test_19_multidimensional_metadata_and_figure(self):
  s=self.run_code("close all; nd_value=reshape(1:8,[2,2,2]); scalar_after_nd=42; figure(1); plot(1:3);")
  self.assertFalse(s['error'],s);items={v['name']:v for v in s['variables']}
  self.assertEqual(items['nd_value']['preview'],'2x2x2 double');self.assertEqual(items['scalar_after_nd']['preview'],'42');self.assertEqual(len(s['figures']),1,s)
  self.run_code('close all;')
 def test_20_inspect_supported_values(self):
  s=self.run_code("nd_detail=reshape(1:8,[2,2,2]); cell_detail={1,'iki';struct('x',3),true}; struct_detail=struct('a',{1,2},'b',{'x','y'}); logical_detail=logical([1 0;0 1]); char_detail=['ab';'cd']; complex_detail=[1+2i,3-4i]; int_detail=int16([-2 7]); function_detail=@sin; empty_detail=zeros(0,3);")
  self.assertFalse(s['error'],s)
  details={}
  for name in ('nd_detail','cell_detail','struct_detail','logical_detail','char_detail','complex_detail','int_detail','function_detail','empty_detail'):
   self.k.submit(mode='inspect',argument=name);s=self.wait();self.assertFalse(s['error'],f'{name}: {s}');self.assertEqual(s['detail']['name'],name);details[name]=s['detail']
  self.assertIn('çok boyutlu',details['nd_detail']['text']);self.assertIn('Cell array',details['cell_detail']['text']);self.assertIn('Alanlar',details['struct_detail']['text'])
  self.assertEqual(details['logical_detail']['rows'][0],['1','0']);self.assertEqual(details['char_detail']['text'],'ab\ncd');self.assertIn('i',details['complex_detail']['rows'][0][0])
  self.assertEqual(details['int_detail']['rows'][0],['-2','7']);self.assertEqual(details['function_detail']['text'],'sin');self.assertEqual(details['empty_detail']['rows'],[])
 def test_21_clc_output_diary_and_input(self):
  s=self.run_code("diary_file=[tempname() '.txt']; diary(diary_file); disp('clc-oncesi-kernel'); clc; disp('clc-arasi-kernel'); clc; disp('clc-sonrasi-kernel'); diary off; diary_text=fileread(diary_file); unlink(diary_file); assert(isempty(strfind(diary_text,'__MF_CLC_')));")
  self.assertFalse(s['error'],s);self.assertNotIn('clc-oncesi-kernel',s['output']);self.assertNotIn('clc-arasi-kernel',s['output']);self.assertIn('clc-sonrasi-kernel',s['output']);self.assertNotIn('__MF_CLC_',s['output']);self.assertGreaterEqual(s['console_clear'],2)
  self.k.submit("disp('clc-input-oncesi'); clc; clc_input=input('Değer: '); disp(clc_input);")
  for _ in range(100):
   if self.k.snapshot().get('waiting_input'):break
   time.sleep(.05)
  self.assertTrue(self.k.snapshot().get('waiting_input'),self.k.snapshot());self.k.input('23');s=self.wait();self.assertFalse(s['error'],s);self.assertNotIn('clc-input-oncesi',s['output']);self.assertIn('23',s['output'])
 def test_22_interactive_figure_export(self):
  code="close all; figure(31); subplot(2,1,1); plot([1 2 4],[3 5 4],'-o','displayname','ölçüm'); hold on; long_y=zeros(1,5001); long_y(2501)=999; plot(1:5001,long_y,'displayname','uzun'); legend('show'); title('Doğrulama'); grid on; subplot(2,1,2); semilogx([1 10 100],[2 4 8],'displayname','log seri'); xlabel('f'); ylabel('g');"
  s=self.run_code(code);self.assertFalse(s['error'],s);self.assertEqual(len(s['figures']),1,s);figure=s['figures'][0];self.assertTrue(figure['interactive'],figure);self.assertTrue(figure['decimated'],figure)
  data=json.loads((Path(self.temp.name)/'runtime'/figure['job']/figure['data_file']).read_text());self.assertTrue(data['supported']);self.assertEqual(len(data['axes']),2)
  series=[item for axis in data['axes'] for item in axis['series']];exact=next(item for item in series if item['display_name']=='ölçüm');self.assertEqual(exact['x'],[1,2,4]);self.assertEqual(exact['y'],[3,5,4]);self.assertEqual(exact['marker'],'o')
  large=next(item for item in series if item['display_name']=='uzun');self.assertLessEqual(len(large['x']),2000);self.assertEqual(max(large['y']),999);self.assertEqual(large['original_points'],5001)
  log_axis=next(axis for axis in data['axes'] if axis['xscale']=='log');self.assertEqual(log_axis['xlabel'],'f');self.assertEqual(log_axis['ylabel'],'g');self.assertTrue(any(axis['legend']['visible'] for axis in data['axes']));self.assertTrue(all(len(axis['position'])==4 for axis in data['axes']))
  s=self.run_code('close all; figure(32); surf(peaks(8));');self.assertFalse(s['error'],s);figure=s['figures'][0];self.assertFalse(figure['interactive'],figure);self.assertIn('surface',figure['fallback_reason'])
  data=json.loads((Path(self.temp.name)/'runtime'/figure['job']/figure['data_file']).read_text());self.assertFalse(data['supported']);self.assertIn('surface',data['reason']);self.run_code('close all;')
 def test_23_profile_saved_script(self):
  folder=Path(self.temp.name)/'profile-case';folder.mkdir()
  script=folder/'profile_script.m';helper=folder/'profile_helper.m'
  script.write_text('for k=1:4; profile_value=profile_helper(k); end\n')
  helper.write_text('function y=profile_helper(x)\n  y=sin(x);\nend\n')
  self.k.submit(mode='profile',argument=str(script));s=self.wait();self.assertFalse(s['error'],s)
  rows={row['name']:row for row in s['profile']};self.assertEqual(rows['profile_script']['calls'],1);self.assertEqual(rows['profile_helper']['calls'],4);self.assertEqual(rows['sin']['calls'],4)
  self.assertEqual(rows['profile_helper']['file'],str(helper.resolve()));self.assertEqual(rows['profile_helper']['line'],1);self.assertGreaterEqual(rows['profile_helper']['total'],rows['profile_helper']['self'])
  self.assertFalse(self.run_code('assert(profile_value==sin(4));')['error'])
 def test_24_publish_html_with_code_and_output(self):
  folder=Path(self.temp.name).resolve()/'publish-case';folder.mkdir();script=folder/'publish_code.m'
  script.write_text('%% Profil raporu\npublished_value=21*2;\ndisp(published_value);\n')
  # Without the app adapter the protocol must explicitly say staged-only,
  # and must never leave a half-installed final report at the advertised path.
  self.k.submit(mode='publish',argument=str(script));s=self.wait();self.assertFalse(s['error'],s)
  report=Path(s['publish']['path']);self.assertEqual(report,folder/'html'/'publish_code.html')
  self.assertTrue(s['publish']['staged']);self.assertFalse(report.exists())
  self.assertTrue((self.k.runtime/s['job']/'publish'/'publish_code.html').is_file())
  with self.finalised_publish(script) as (app,s):
   self.assertFalse(s['error'],s);self.assertFalse(s['publish']['staged'])
   self.assertEqual(Path(s['publish']['path']),report);self.assertTrue(report.is_file())
   html=report.read_text();self.assertIn('published_value=21*2',html);self.assertIn('42',html)
   self.assertIn('Content-Security-Policy',html);self.assertNotIn('<script',html)
   self.assertFalse((app.kernel.runtime/s['job']/'publish').exists())
 def test_25_publish_includes_figure_image(self):
  if not self.k.snapshot().get('toolkits'):self.skipTest('Bu Octave kurulumunda grafik araç takımı yok.')
  folder=Path(self.temp.name).resolve()/'publish-figure';folder.mkdir();script=folder/'publish_figure.m'
  script.write_text("%% Grafik\nfigure; plot(1:3,[1 4 9],'-o','markerfacecolor','auto'); hold on; plot(1:3,[2 5 8],'-s','markerfacecolor',[1 1 1]); title('Yayın');\nface_probe=__mf_figure_data__(gcf()); assert(face_probe.supported); assert(sum(cellfun(@(s)s.marker_face_auto,face_probe.axes{1}.series))==1);\n")
  with self.finalised_publish(script) as (app,s):
   self.assertFalse(s['error'],s);self.assertFalse(s['publish']['staged']);self.assertGreaterEqual(s['publish']['images'],1,s)
   class Images(HTMLParser):
    def __init__(self):super().__init__();self.sources=[]
    def handle_starttag(self,tag,attrs):
     if tag=='img':self.sources.append(dict(attrs).get('src',''))
   images=Images();images.feed(Path(s['publish']['path']).read_text())
   self.assertEqual(len(images.sources),s['publish']['images'])
   for src in images.sources:
    self.assertTrue(src.startswith('data:image/png;base64,'));png=base64.b64decode(src.split(',',1)[1],validate=True)
    self.assertTrue(png.startswith(b'\x89PNG\r\n\x1a\n'));self.assertGreater(len(png),100)
    self.assertGreater(int.from_bytes(png[16:20],'big'),0);self.assertGreater(int.from_bytes(png[20:24],'big'),0)
   self.assertFalse((app.kernel.runtime/s['job']/'publish').exists())
 @contextmanager
 def finalised_publish(self,script):
  # Same App/PublishKernel wiring and POST registration path as production,
  # without a listening HTTP server or the user's persistent Octave session.
  with tempfile.TemporaryDirectory(dir=Path(self.temp.name).resolve()) as runtime:
   app=App.__new__(App);app.workspace=Workspace(script.parent,Path(self.temp.name).resolve())
   app.file_lock=threading.Lock();app.publish_lock=threading.Lock();app.publish_renders={}
   app.kernel=PublishKernel(ROOT,Path(runtime),script.parent,publish_app=app)
   def idle():
    deadline=time.monotonic()+60
    while time.monotonic()<deadline:
     state=app.kernel.snapshot()
     if state['status'] not in ('starting','running','stopping'):return state
     time.sleep(.05)
    self.fail('publish finalisation timeout')
   try:
    self.assertFalse(idle()['error'])
    handler=Handler.__new__(Handler);handler.server=SimpleNamespace(app=app);reply=[]
    handler.send=lambda status,data:reply.append((status,data))
    handler.post('/api/publish',{'path':str(script)})
    self.assertEqual(reply[0][0],202);state=idle();self.assertEqual(state['job'],reply[0][1]['job'])
    yield app,state
   finally:app.kernel.close()
 def test_26_profile_and_publish_fail_cleanly(self):
  folder=Path(self.temp.name)/'failure-case';folder.mkdir();script=folder/'broken_script.m';script.write_text("error('beklenen yayın hatası');\n")
  before=self.k.snapshot()['cwd'];self.k.submit(mode='profile',argument=str(script));s=self.wait();self.assertIn('beklenen yayın hatası',s['error'])
  self.assertFalse(self.run_code("profile_status=profile('status'); assert(strcmp(profile_status.ProfilerStatus,'off')); clear profile_status; profile_failure_recovered=1;")['error'])
  self.k.submit(mode='publish',argument=str(script));s=self.wait();self.assertIn('HTML raporu oluşturulamadı',s['error']);self.assertIn('beklenen yayın hatası',s['error']);self.assertEqual(s['cwd'],before)
  self.assertFalse(self.run_code('assert(profile_failure_recovered==1);')['error'])
 def test_27_package_load_unload_preserves_session(self):
  self.run_code('package_action_kept=808;')
  self.k.package('unload','signal');s=self.wait();self.assertFalse(s['error'],s);self.assertFalse(next(p for p in s['packages'] if p['name']=='signal')['loaded'])
  self.k.package('load','signal');s=self.wait();self.assertFalse(s['error'],s);self.assertTrue(next(p for p in s['packages'] if p['name']=='signal')['loaded'])
  self.assertFalse(self.run_code('assert(package_action_kept==808); assert(exist("butter","file")>0);')['error'])
 def test_28_debugger(self):
  path=ROOT/'workspace'/'kernel_debug_test.m';script=ROOT/'workspace'/'kernel_debug_script_test.m';path.write_text('function y=kernel_debug_test(x)\n  local_value=x+1;\n  stepped_value=kernel_debug_inner(local_value);\n  y=stepped_value+3;\nendfunction\nfunction value=kernel_debug_inner(value)\n  value=value*2;\nendfunction\n');script.write_text('script_a=2;\nscript_b=script_a+5;\nscript_done=script_b*2;\n')
  def paused():
   for _ in range(300):
    state=self.k.snapshot()
    if state['status']=='paused' and state.get('debug',{}).get('ready'):return state
    time.sleep(.05)
   raise AssertionError(self.k.snapshot())
  try:
   self.k.breakpoint(path,2,True);self.wait();self.assertEqual(self.k.snapshot()['breakpoints'][0]['lines'],[2])
   self.k.submit('debug_answer=kernel_debug_test(4); disp(debug_answer);');s=paused();self.assertEqual(s['debug']['file'],str(path));self.assertEqual(s['debug']['line'],2);self.assertEqual({v['name'] for v in s['variables']},{'x'})
   with self.assertRaises(ValueError):self.k.input('dbcont')
   self.k.debug('eval','scope_probe=x+10;');s=paused();self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='scope_probe'),'14')
   self.k.debug('step');s=paused();self.assertEqual(s['debug']['line'],3);self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='local_value'),'5')
   self.k.debug('in');s=paused();self.assertEqual(s['debug']['line'],7);self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='value'),'5');self.k.debug('out');s=paused();self.assertEqual(s['debug']['line'],4)
   self.k.debug('continue');s=self.wait();self.assertFalse(s['error'],s);self.assertIn('13',s['output']);self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='debug_answer'),'13')
   self.k.submit('kernel_debug_test(5);');paused();self.k.debug('quit');self.assertEqual(self.wait()['status'],'idle')
   self.k.submit('kernel_debug_test(6);');paused();self.k.interrupt();self.k.interrupt();self.assertEqual(self.wait(10)['status'],'idle')
   self.k.submit('kernel_debug_test(6);');paused();self.k.reset();s=self.wait();self.assertIsNone(s['debug']);self.k.submit('kernel_debug_test(7);');s=paused();self.assertEqual(s['debug']['line'],2);self.k.debug('continue');self.assertFalse(self.wait()['error'])
   self.k.breakpoint(path,2,False);self.wait();self.assertEqual(self.k.snapshot()['breakpoints'],[])
   self.k.breakpoint(script,2,True);self.wait();self.k.submit(mode='file',argument=str(script));s=paused();self.assertEqual(s['debug']['line'],2);self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='script_a'),'2');self.k.debug('continue');s=self.wait();self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='script_done'),'14');self.k.breakpoint(script,2,False);self.wait()
  finally:
   if self.k.snapshot()['status']=='paused':self.k.interrupt();self.wait(10)
   path.unlink(missing_ok=True);script.unlink(missing_ok=True)
 def test_29_session_is_not_a_dock_application(self):
  if sys.platform!='darwin':self.skipTest('macOS Dock kaydı yalnız macOS için geçerli.')
  import re,subprocess
  self.assertFalse(self.run_code("figure('visible','off'); plot(1:3); drawnow; close all;")['error'])
  group=set(subprocess.run(['pgrep','-g',str(self.k.proc.pid)],capture_output=True,text=True).stdout.split());self.assertTrue(group)
  listing=subprocess.run(['lsappinfo','list'],capture_output=True,text=True).stdout
  types={m.group(1):m.group(2) for m in re.finditer(r'pid = (\d+) .*?type="(\w+)"',listing) if m.group(1) in group}
  self.assertNotIn('Foreground',types.values(),types)

def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
