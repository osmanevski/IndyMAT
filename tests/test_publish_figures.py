import base64
import json
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from contextlib import contextmanager
from html.parser import HTMLParser
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from app import App,Handler,PublishKernel
from backend.files import Workspace

ROOT=Path(__file__).resolve().parents[1]
def oq(value):return "'"+str(value).replace("'","''")+"'"


class PublishFigureTests(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.octave=shutil.which('octave-cli')
  if not cls.octave:raise unittest.SkipTest('octave-cli is not installed')

 def test_fingerprint_is_repeatable_and_tracks_property_changes(self):
  code=f"addpath('{ROOT/'octave'}'); a=__mf_publish_fingerprint_value__(struct('xdata',[1 2],'color',[0 0 0])); b=__mf_publish_fingerprint_value__(struct('color',[0 0 0],'xdata',[1 2])); c=__mf_publish_fingerprint_value__(struct('color',[0 0 0],'xdata',[1 3])); assert(strcmp(a,b)); assert(!strcmp(a,c)); disp('fingerprint-ok');"
  result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],capture_output=True,text=True,timeout=15)
  self.assertEqual(result.returncode,0,result.stdout+result.stderr)
  self.assertIn('fingerprint-ok',result.stdout)

 def test_full_data_comparison_and_streamed_hash_cross_chunk_boundaries(self):
  code=f"addpath({oq(ROOT/'octave')}); a=reshape(1:200000,400,500); b=a; assert(__mf_publish_equal__(a,b)); old=__mf_publish_fingerprint_value__(a); for idx=[1 65536 65537 200000]; b=a; b(idx)=-1; assert(!__mf_publish_equal__(a,b)); assert(!strcmp(old,__mf_publish_fingerprint_value__(b))); end; assert(__mf_publish_equal__([NaN Inf],[NaN Inf])); assert(!__mf_publish_equal__(single(1),double(1))); try; __mf_publish_fingerprint_value__(@sin); error('missing rejection'); catch err; assert(!isempty(strfind(err.message,'desteklenmiyor'))); end;"
  result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],capture_output=True,text=True,timeout=30)
  self.assertEqual(result.returncode,0,result.stdout+result.stderr)

 def test_tracker_private_state_and_missing_state_error(self):
  code=f"addpath({oq(ROOT/'octave')}); before=getappdata(0); tracker=__mf_publish_figures__(); assert(isequal(before,getappdata(0))); try; tracker('end'); error('missing rejection'); catch err; assert(!isempty(strfind(err.message,'özel durumu bulunamadı'))); end; tracker('begin'); assert(isequal(before,getappdata(0))); assert(isempty(tracker('end'))); tracker('cleanup'); assert(isequal(before,getappdata(0)));"
  result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],capture_output=True,text=True,timeout=30)
  self.assertEqual(result.returncode,0,result.stdout+result.stderr)

 def test_wrapper_restores_flag_and_does_not_expose_bookkeeping(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-bookkeeping-') as temporary:
   root=Path(temporary);stage=root/'runtime'/'publish';stage.mkdir(parents=True);source=root/'bookkeeping.m'
   source.write_text("%% Report\nbookkeeping_value=2+3; assert(!isappdata(0,'__mf_publish_figure_state__')); assert(exist('mf_tracker','var')==0); assert(exist('before','var')==0);\n")
   for name in ('__mf_publish_figures__','__mf_publish_snapshot__','__mf_publish_equal__','__mf_publish_fingerprint_value__'):
    (root/(name+'.m')).write_text(f"function varargout={name}(varargin)\nerror('CURRENT DIRECTORY HELPER WAS CALLED');\nendfunction\n")
   quote=lambda value:"'"+str(value).replace("'","''")+"'"
   code=f"addpath({quote(ROOT/'octave')}); setappdata(0,'__mf_folder__',{quote(root/'runtime')}); setappdata(0,'__mf_publishing__','prior'); __mf_publish__({quote(source)}); assert(strcmp(getappdata(0,'__mf_publishing__'),'prior')); assert(!isappdata(0,'__mf_publish_figure_state__'));"
   result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],cwd=root,capture_output=True,text=True,timeout=45)
   self.assertEqual(result.returncode,0,result.stdout+result.stderr)
   report=(stage/'bookkeeping.html').read_text()
   self.assertIn('bookkeeping_value=2+3',report)
   self.assertNotIn('__mf_publish_figures__',report)

 def test_wrapper_restores_state_after_publish_error(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-error-state-') as temporary:
   root=Path(temporary);(root/'runtime'/'publish').mkdir(parents=True);source=root/'failure.m'
   source.write_text("error('publish failure probe');\n")
   quote=lambda value:"'"+str(value).replace("'","''")+"'"
   code=f"addpath({quote(ROOT/'octave')}); setappdata(0,'__mf_folder__',{quote(root/'runtime')}); setappdata(0,'__mf_publishing__','prior'); before=pwd(); try; __mf_publish__({quote(source)}); error('missing failure'); catch err; assert(!isempty(strfind(err.message,'publish failure probe'))); end_try_catch; assert(strcmp(pwd(),before)); assert(strcmp(getappdata(0,'__mf_publishing__'),'prior')); assert(!isappdata(0,'__mf_publish_figure_state__'));"
   result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],cwd=root,capture_output=True,text=True,timeout=45)
   self.assertEqual(result.returncode,0,result.stdout+result.stderr)

 def wait(self,app,seconds=60):
  deadline=time.monotonic()+seconds
  while time.monotonic()<deadline:
   state=app.kernel.snapshot()
   if state['status'] not in ('starting','running','stopping'):return state
   time.sleep(.05)
  self.fail('publish finalisation timeout')

 @contextmanager
 def finalised_publish(self,source,prelude=None,graphics=True,seconds=60):
  # Match test_kernel.py: the script and runtime share one resolved allowed
  # root. A separate sibling temporary directory cannot be its allowed root.
  source=source.resolve();allowed_root=source.parent
  with tempfile.TemporaryDirectory(dir=allowed_root) as runtime:
   app=App.__new__(App);app.workspace=Workspace(source.parent,allowed_root)
   app.file_lock=threading.Lock();app.publish_lock=threading.Lock();app.publish_renders={}
   app.kernel=PublishKernel(ROOT,Path(runtime).resolve(),source.parent,publish_app=app)
   try:
    initial=self.wait(app);self.assertFalse(initial['error'],initial)
    if graphics and not initial.get('toolkits'):
     self.skipTest('Bu Octave oturumunda grafik araç takımı yok.')
    if prelude:
     app.kernel.submit(prelude);state=self.wait(app);self.assertFalse(state['error'],state)
    handler=Handler.__new__(Handler);handler.server=SimpleNamespace(app=app);reply=[]
    handler.send=lambda status,data:reply.append((status,data))
    handler.post('/api/publish',{'path':str(source)})
    self.assertEqual(reply[0][0],202);state=self.wait(app,seconds);self.assertEqual(state['job'],reply[0][1]['job'])
    yield app,state
   finally:app.kernel.close()

 def report_images(self,app,state,count,expected=None,colors=None):
  self.assertFalse(state['error'],state);self.assertFalse(state['publish']['staged'])
  class Images(HTMLParser):
   def __init__(self):
    super().__init__();self.sources=[];self.sections=[];self.section='';self.heading=False
   def handle_starttag(self,tag,attrs):
    if tag in ('h1','h2'):self.heading=True;self.section=''
    if tag=='img':self.sources.append(dict(attrs).get('src',''));self.sections.append(self.section.strip())
   def handle_data(self,data):
    if self.heading:self.section+=data
   def handle_endtag(self,tag):
    if tag in ('h1','h2'):self.heading=False
  images=Images();images.feed(Path(state['publish']['path']).read_text())
  self.assertEqual(len(images.sources),count)
  self.assertEqual(state['publish']['images'],count)
  decoded_paths=[]
  for index,src in enumerate(images.sources):
   self.assertTrue(src.startswith('data:image/png;base64,'))
   png=base64.b64decode(src.split(',',1)[1],validate=True)
   self.assertTrue(png.startswith(b'\x89PNG\r\n\x1a\n'));self.assertGreater(len(png),100)
   self.assertGreater(int.from_bytes(png[16:20],'big'),0);self.assertGreater(int.from_bytes(png[20:24],'big'),0)
   path=app.kernel.runtime/f'capture-{index}.png';path.write_bytes(png);decoded_paths.append(path)
  if expected is not None:self.assert_decoded_images(decoded_paths,expected,colors)
  self.assertFalse((app.kernel.runtime/state['job']/'publish').exists())
  return images.sections

 def assert_decoded_images(self,actual,expected,colors):
  # Content identity oracle for deliberately large, uniquely coloured regions.
  # References are printed from the same live handles with publish's flags.
  # Normalize palette/grayscale/alpha representations before counting pixels.
  # Dimension equality is deliberately not required: colour coverage is scale
  # independent. A small antialiased edge or changed margin may differ, but a
  # blank, stale state, wrong figure, swapped order or repeated PNG must fail.
  self.assertEqual(len(actual),len(expected));self.assertEqual(len(colors),len(actual))
  paths=[str(path) for pair in zip(actual,expected) for path in pair]
  with tempfile.TemporaryDirectory(prefix='indymat-png-content-') as temporary:
   folder=Path(temporary)
   (folder/'pf_png_statistics.m').write_text(PNG_STATISTICS)
   code=f"addpath({oq(folder)}); paths={{{','.join(oq(path) for path in paths)}}}; "
   code+="""stats={}; differences={};
for k=1:2:numel(paths)
  [a,ar]=pf_png_statistics(paths{k}); [b,br]=pf_png_statistics(paths{k+1});
  stats(end+1:end+2)={a,b}; difference=struct('equal_dimensions',isequal(size(ar),size(br)));
  if difference.equal_dimensions
    delta=max(abs(ar-br),[],3);
    difference.max_channel_delta=max(delta(:));
    difference.fraction_changed=sum(delta(:)>0)/numel(delta);
    difference.fraction_delta_over_0_05=sum(delta(:)>0.05)/numel(delta);
  endif
  differences{end+1}=difference;
end
disp(jsonencode(struct('stats',{stats},'differences',{differences})));
"""
   result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],capture_output=True,text=True,timeout=45)
  self.assertEqual(result.returncode,0,result.stdout+result.stderr)
  result_data=json.loads(result.stdout);stats=result_data['stats']
  names=['red','green','blue']
  for index,color in enumerate(colors):
   a,b=stats[2*index:2*index+2];target=names.index(color)
   diagnostic=(f'capture[{index}] expected {color}; actual={actual[index]} '
               f'{a["width"]}x{a["height"]}, reference={expected[index]} '
               f'{b["width"]}x{b["height"]}; RGB coverage actual={a["coverage"]}, '
               f'reference={b["coverage"]}; formats actual={a["format"]}, reference={b["format"]}; '
               f'normalized pixel difference={result_data["differences"][index]}')
   for label,entry in [('reference',b),('actual',a)]:
    coverage=entry['coverage'];wanted=coverage[target]
    self.assertGreaterEqual(wanted,0.01,f'{diagnostic}; {label} lacks a substantial {color} region (minimum 1%)')
    purity=wanted/sum(coverage)
    self.assertGreaterEqual(purity,0.95,f'{diagnostic}; {label} shows another identity colour (purity {purity:.4f}, minimum 95%)')
   self.assertLessEqual(abs(a['coverage'][target]-b['coverage'][target]),0.15,
                        f'{diagnostic}; coloured area differs by more than 15 percentage points')
   difference=result_data['differences'][index]
   if not difference['equal_dimensions'] or difference.get('fraction_changed',0)>0 or a['format']!=b['format']:
    print('\nPNG content identity passed despite raster differences: '+diagnostic,flush=True)

 def test_decoded_image_comparison_rejects_a_repeated_placeholder(self):
  with tempfile.TemporaryDirectory() as temporary:
   red=Path(temporary)/'red.png';blue=Path(temporary)/'blue.png'
   code=f"a=zeros(8,8,3,'uint8'); a(:,:,1)=255; imwrite(a,{oq(red)}); a(:,:,1)=0; a(:,:,3)=255; imwrite(a,{oq(blue)});"
   result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],capture_output=True,text=True,timeout=30)
   self.assertEqual(result.returncode,0,result.stdout+result.stderr)
   self.assert_decoded_images([red,blue],[red,blue],['red','blue'])
   with self.assertRaisesRegex(AssertionError,r'capture\[1\] expected blue.*8x8.*RGB coverage'):
    self.assert_decoded_images([red,red],[red,blue],['red','blue'])
   with self.assertRaisesRegex(AssertionError,r'capture\[0\] expected red'):
    self.assert_decoded_images([blue,red],[red,blue],['red','blue'])

 def test_decoded_image_comparison_tolerates_scale_palette_and_small_edge_changes(self):
  with tempfile.TemporaryDirectory() as temporary:
   folder=Path(temporary);reference=folder/'red.png';scaled=folder/'scaled.png';indexed=folder/'indexed.png';edge=folder/'edge.png';blank=folder/'blank.png'
   code=f"a=zeros(64,64,3,'uint8'); a(:,:,1)=255; imwrite(a,{oq(reference)}); imwrite(repmat(a,2,2),{oq(scaled)}); imwrite(ones(64,'uint8'),[0 0 1;1 0 0],{oq(indexed)}); a([1 end],:,:)=255; a(:,[1 end],:)=255; imwrite(a,{oq(edge)}); imwrite(uint8(255*ones(64,64,3)),{oq(blank)});"
   result=subprocess.run([self.octave,'--quiet','--no-history','--eval',code],capture_output=True,text=True,timeout=30)
   self.assertEqual(result.returncode,0,result.stdout+result.stderr)
   self.assert_decoded_images([scaled,indexed,edge],[reference]*3,['red']*3)
   with self.assertRaisesRegex(AssertionError,'actual lacks a substantial red region'):
    self.assert_decoded_images([blank],[reference],['red'])

 def test_resolved_fixture_reaches_server_finaliser_without_graphics(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-fixture-') as temporary:
   source=Path(temporary).resolve()/'fixture.m';source.write_text('%% Fixture\ndisp(42);\n')
   with self.finalised_publish(source,graphics=False) as (app,state):
    self.report_images(app,state,0)

 def test_metadata_changes_do_not_capture_and_private_helpers_resist_shadowing(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-metadata-') as temporary:
   folder=Path(temporary).resolve();source=folder/'metadata.m'
   for name in ('__mf_publish_figures__','__mf_publish_snapshot__','__mf_publish_equal__'):
    (folder/(name+'.m')).write_text(f"function varargout={name}(varargin)\nerror('shadow helper executed');\nendfunction\n")
   source.write_text("%% Metadata only\nassert(!isappdata(0,'__mf_publish_figure_state__')); assert(exist('mf_tracker','var')==0); set(3,'userdata',struct('payload',@sin),'tag','changed','name','window name','selected','on'); set(0,'currentfigure',1); setappdata(0,'__mf_publish_figure_state__',42); rmappdata(0,'__mf_publish_figure_state__');\n%% Visual change\nset(findobj(3,'type','image'),'cdata',repmat(reshape([1 0 0],1,1,3),32,32)); drawnow(); print(3,'expected.png','-dpng','-color');\n")
   with self.finalised_publish(source,"close all; figure(1,'visible','off'); image(repmat(reshape([0 0 1],1,1,3),32,32)); axis off; figure(3,'visible','off'); image(repmat(reshape([0 1 0],1,1,3),32,32)); axis off;") as (app,state):
    self.assertEqual(self.report_images(app,state,1,[folder/'expected.png'],['red']),['Visual change'])

 def test_mixed_new_and_existing_captures_follow_reversed_root_order(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-order-') as temporary:
   folder=Path(temporary).resolve();source=folder/'ordered.m'
   source.write_text("%% Mixed figures\nset(findobj(31,'type','image'),'cdata',repmat(reshape([1 0 0],1,1,3),32,32)); set(findobj(7,'type','image'),'cdata',repmat(reshape([0 1 0],1,1,3),32,32)); figure(2,'visible','off'); image(repmat(reshape([0 0 1],1,1,3),32,32)); axis off; for h=[31 7 2]; drawnow(); print(h,sprintf('expected%d.png',h),'-dpng','-color'); end; order=flipud(allchild(0)(:)); fid=fopen('order.json','w'); fputs(fid,jsonencode(order)); fclose(fid);\n")
   prelude="close all; figure(31,'visible','off'); image(zeros(32,32,3)); axis off; figure(7,'visible','off'); image(zeros(32,32,3)); axis off;"
   with self.finalised_publish(source,prelude) as (app,state):
    order=json.loads((folder/'order.json').read_text());self.assertEqual(set(order),{31,7,2})
    self.assertEqual(self.report_images(app,state,3,[folder/f'expected{h}.png' for h in order],[{31:'red',7:'green',2:'blue'}[h] for h in order]),['Mixed figures']*3)

 def test_unsupported_embedded_gui_fails_explicitly(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-unsupported-') as temporary:
   source=Path(temporary).resolve()/'gui.m'
   source.write_text("figure(3,'visible','off'); uicontrol('parent',3,'style','pushbutton','string','unsupported');\n")
   with self.finalised_publish(source) as (app,state):
    self.assertIn('nesne türünü desteklemiyor: uicontrol',state['error'])
    self.assertFalse((source.parent/'html'/'gui.html').exists())

 def test_large_figure_snapshot_benchmark(self):
  # Measures capture bookkeeping only, excluding print. Closes the benchmark
  # figures in the script so Kernel's normal figure export does not dominate.
  with tempfile.TemporaryDirectory(prefix='indymat-publish-benchmark-') as temporary:
   folder=Path(temporary).resolve();source=folder/'benchmark.m'
   (folder/'pf_legacy_pass.m').write_text(LEGACY_FIGURE_PASS)
   source.write_text("""figure(41,'visible','off'); subplot(1,2,1); im=image(reshape(mod(1:4000000,256),2000,2000));
subplot(1,2,2); [x,y]=meshgrid(linspace(-3,3,1000)); z=sin(x).*cos(y); sf=surf(x,y,z,'edgecolor','none'); drawnow();
tracker=__mf_publish_figures__(); tracker('begin'); assert(isempty(tracker('end')));
old=zeros(1,3); fresh=zeros(1,3);
for run=1:3
  stamp=tic(); pf_legacy_pass(); pf_legacy_pass(); old(run)=toc(stamp);
  stamp=tic(); tracker('begin'); assert(isempty(tracker('end'))); fresh(run)=toc(stamp);
end
tracker('begin'); values=get(im,'cdata'); values(end)+=1; set(im,'cdata',values); assert(isequal(tracker('end'),41));
tracker('begin'); values=get(sf,'zdata'); values(end)+=1; set(sf,'zdata',values); assert(isequal(tracker('end'),41));
tracker('cleanup'); close(41);
result=struct('old_seconds_per_block',median(old),'new_seconds_per_block',median(fresh),'image_pixels',4000000,'surface_points',1000000);
fid=fopen('benchmark.json','w'); fputs(fid,jsonencode(result)); fclose(fid);
""")
   with self.finalised_publish(source,"close all; set(0,'defaultfigurevisible','off');",seconds=180) as (app,state):
    self.report_images(app,state,0)
    measurements=json.loads((folder/'benchmark.json').read_text())
    self.assertGreater(measurements['old_seconds_per_block'],0)
    self.assertGreater(measurements['new_seconds_per_block'],0)
    print('\nQt publish bookkeeping benchmark: '+json.dumps(measurements),flush=True)

 def test_existing_redraw_and_two_sections_are_captured_and_noop_is_not(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-redraw-') as temporary:
   folder=Path(temporary).resolve();source=folder/'redraw.m'
   source.write_text("%% Redraw figure 3\nfigure(3,'visible','off'); plot(1:4,[2 4 6 8],'color',[1 0 0],'linewidth',60); title('redrawn'); drawnow(); print(3,'expected3.png','-dpng','-color');\n%% No change\nsection_noop_value=19;\n%% Redraw figure 1\nfigure(1,'visible','off'); plot(1:4,[8 6 4 2],'color',[0 0 1],'linewidth',60); title('also redrawn'); drawnow(); print(1,'expected1.png','-dpng','-color');\n")
   with self.finalised_publish(source,"close all; figure(1,'visible','off'); plot(1:2); figure(3,'visible','off'); plot(1:2); set(0,'currentfigure',1);") as (app,state):
    self.assertEqual(self.report_images(app,state,2,[folder/'expected3.png',folder/'expected1.png'],['red','blue']),['Redraw figure 3','Redraw figure 1'])
    probe="assert(isgraphics(1,'figure')&&isgraphics(3,'figure')); assert(strcmp(get(1,'visible'),'off')&&strcmp(get(3,'visible'),'off')); assert(get(0,'currentfigure')==1); assert(numel(findall(0,'type','figure'))==2);"
    app.kernel.submit(probe);checked=self.wait(app);self.assertFalse(checked['error'],checked)

 def test_new_figure_capture_and_closed_existing_figure(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-new-close-') as temporary:
   folder=Path(temporary).resolve();source=folder/'new_close.m'
   source.write_text("%% Create figure 11\nfigure(11,'visible','off'); plot(1:4,[1 3 2 4]);\n%% Close figure 5\nclose(5);\n")
   with self.finalised_publish(source,"close all; figure(5,'visible','off'); plot(1:2); figure(7,'visible','off'); plot(1:2);") as (app,state):
    self.assertEqual(self.report_images(app,state,1),['Create figure 11'])
    app.kernel.submit("assert(!isgraphics(5,'figure')); assert(isgraphics(7,'figure')); assert(isgraphics(11,'figure')); assert(numel(findall(0,'type','figure'))==2); assert(strcmp(get(7,'visible'),'off')); assert(strcmp(get(11,'visible'),'off')); assert(get(0,'currentfigure')==11);")
    checked=self.wait(app);self.assertFalse(checked['error'],checked)

 def test_plot_without_existing_figure_stays_open_and_is_not_recaptured_by_noop(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-implicit-') as temporary:
   source=Path(temporary).resolve()/'implicit.m'
   source.write_text("%% Initial plot\nplot([0 1],[.5 .5],'color',[1 0 0],'linewidth',80); axis([0 1 0 1]); axis off; setappdata(0,'pf_created',gcf()); drawnow(); print(gcf(),'initial.png','-dpng','-color');\n%% No change\nnoop=1;\n%% Change same figure\nassert(gcf()==getappdata(0,'pf_created')); plot([0 1],[.5 .5],'color',[0 0 1],'linewidth',80); axis([0 1 0 1]); axis off; drawnow(); print(gcf(),'changed.png','-dpng','-color');\n")
   with self.finalised_publish(source,"close all; set(0,'defaultfigurevisible','off');") as (app,state):
    self.assertEqual(self.report_images(app,state,2,[source.parent/'initial.png',source.parent/'changed.png'],['red','blue']),['Initial plot','Change same figure'])
    app.kernel.submit("pf_h=getappdata(0,'pf_created'); assert(isgraphics(pf_h,'figure')); assert(numel(findall(0,'type','figure'))==1); assert(get(0,'currentfigure')==pf_h); assert(strcmp(get(pf_h,'visible'),'off')); rmappdata(0,'pf_created');")
    checked=self.wait(app);self.assertFalse(checked['error'],checked)

 def test_error_after_section_restores_publish_flag_and_keeps_close_side_effect(self):
  with tempfile.TemporaryDirectory(prefix='indymat-publish-error-') as temporary:
   folder=Path(temporary).resolve();source=folder/'failure.m'
   source.write_text("%% Create and redraw\nfigure(9,'visible','off'); plot(1:3); figure(7,'visible','off'); plot(3:-1:1);\n%% Close an existing figure\nclose(5);\n%% Fail after the previous section\nerror('beklenen bölüm hatası');\n")
   with self.finalised_publish(source,"close all; figure(5,'visible','off'); plot(1:2); figure(7,'visible','off'); plot(1:2);") as (app,state):
    self.assertIn('HTML raporu oluşturulamadı',state['error']);self.assertIn('beklenen bölüm hatası',state['error'])
    self.assertFalse((folder/'html'/'failure.html').exists());self.assertEqual(Path(state['cwd']).resolve(),folder)
    app.kernel.submit("assert(!isappdata(0,'__mf_publishing__')); assert(!isappdata(0,'__mf_publish_figure_state__')); assert(!isgraphics(5,'figure')); assert(isgraphics(7,'figure')); assert(isgraphics(9,'figure')); assert(numel(findall(0,'type','figure'))==2); assert(strcmp(get(7,'visible'),'off')); assert(strcmp(get(9,'visible'),'off')); assert(get(0,'currentfigure')==7);")
    checked=self.wait(app);self.assertFalse(checked['error'],checked)


# Pure PNG decoding; no toolkit or rendering is needed by this helper.
PNG_STATISTICS = r'''
function [result,rgb]=pf_png_statistics(filename)
  [pixels,map,alpha]=imread(filename);
  storage=class(pixels);
  channels=size(pixels,3);
  if ~isempty(map)
    % imread represents a two-entry PNG palette with logical indices;
    % ind2rgb requires numeric zero-based indices for integer images.
    if islogical(pixels),pixels=uint8(pixels);endif
    rgb=ind2rgb(pixels,map);
  else
    rgb=unit_pixels(pixels);
    if channels==1,rgb=repmat(rgb,1,1,3);endif
  endif
  if size(rgb,3)~=3,error('Unexpected decoded channels in %s: %s',filename,mat2str(size(rgb)));endif
  if ~isempty(alpha)
    opacity=unit_pixels(alpha);
    if ~isequal(size(opacity),[size(rgb,1) size(rgb,2)])
      error('Unexpected decoded alpha dimensions in %s: %s',filename,mat2str(size(opacity)));
    endif
    rgb=rgb.*repmat(opacity,1,1,3)+repmat(1-opacity,1,1,3);
  endif
  coverage=zeros(1,3);
  for channel=1:3
    target=zeros(1,1,3); target(channel)=1;
    mask=all(abs(rgb-target)<=0.20,3);
    coverage(channel)=sum(mask(:))/numel(mask);
  endfor
  result=struct('width',size(rgb,2),'height',size(rgb,1),'coverage',coverage, ...
    'format',sprintf('%s channels=%d palette=%d alpha=%d',storage,channels,size(map,1),~isempty(alpha)));
endfunction

function result=unit_pixels(value)
  if isinteger(value),result=double(value)/double(intmax(class(value)));
  else,result=double(value);endif
endfunction
'''

# Frozen pre-review hashing algorithm, used only by the Qt benchmark above.
LEGACY_FIGURE_PASS = r'''
function pf_legacy_pass()
  figures=findall(0,'type','figure');
  for k=1:numel(figures),pf_legacy_signature(figures(k));endfor
endfunction
function signature=pf_legacy_signature(figure_handle)
  objects=findall(figure_handle);
  values=cell(numel(objects),1);
  ignored={'beingdeleted','currentpoint','keypressfcn','keyreleasefcn','windowbuttondownfcn','windowbuttonmotionfcn','windowbuttonupfcn','windowscrollwheelfcn','createfcn','deletefcn'};
  for k=1:numel(objects)
    object=objects(k);
    properties=get(object);
    names=sort(fieldnames(properties));
    record=struct('handle',double(object),'type',get(object,'type'),'properties',struct());
    for j=1:numel(names)
      name=names{j};
      if any(strcmpi(name,ignored)),continue;endif
      value=pf_legacy_value(properties.(name));
      if ~isempty(value)
        record.properties.(name)=value;
      endif
    endfor
    values{k}=record;
  endfor
  signature=pf_legacy_value(values);
endfunction
function signature=pf_legacy_value(value)
  if iscell(value)
    normalized=cell(size(value));
    for k=1:numel(value)
      normalized{k}=pf_legacy_value(value{k});
    endfor
    encoded=jsonencode(struct('class',class(value),'size',size(value),'value',{normalized}));
  elseif isstruct(value)
    names=sort(fieldnames(value));
    normalized=repmat(struct(),size(value));
    for k=1:numel(value)
      for j=1:numel(names)
        child=pf_legacy_value(value(k).(names{j}));
        if ~isempty(child),normalized(k).(names{j})=child;endif
      endfor
    endfor
    encoded=jsonencode(struct('class',class(value),'size',size(value),'value',normalized));
  elseif isnumeric(value)||islogical(value)||ischar(value)
    if isnumeric(value)&&!issparse(value)
      try
        bytes=typecast(value(:),'uint8');
        header=jsonencode(struct('class',class(value),'size',size(value)));
        signature=hash('sha256',[header char(0) char(bytes(:).')]);
        return;
      catch
      end_try_catch
    endif
    encoded=jsonencode(struct('class',class(value),'size',size(value),'value',value));
  elseif isempty(value)
    signature='';return;
  else
    signature='';return;
  endif
  signature=hash('sha256',encoded);
endfunction
'''


def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main()
