"""Measured sgtitle layout regressions in a real, isolated Octave Kernel."""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from backend.kernel import Kernel


class SgtitleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        executable = os.environ.get('INDYMAT_FIGURE_TEST_OCTAVE') or shutil.which('octave-cli')
        if not executable:
            raise unittest.SkipTest('octave-cli unavailable')
        cls.temp = tempfile.TemporaryDirectory()
        cls.work = Path(cls.temp.name)
        cls.k = Kernel(ROOT, cls.work / 'runtime', cls.work, executable=executable)
        try:
            cls.wait()
            cls.k.submit("figure('visible','off');close all;")
            state = cls.wait()
            if state['error'] and 'no graphics toolkits' in state['error']:
                raise unittest.SkipTest('this Octave has no graphics toolkit')
            if state['error']:
                raise AssertionError(state)
        except BaseException:
            cls.k.close()
            cls.temp.cleanup()
            raise

    @classmethod
    def tearDownClass(cls):
        cls.k.close()
        cls.temp.cleanup()

    @classmethod
    def wait(cls):
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            state = cls.k.snapshot()
            if state['status'] not in ('starting', 'running', 'stopping'):
                return state
            time.sleep(.05)
        raise AssertionError('sgtitle test timeout')

    def run_code(self, code):
        self.k.submit("close all;figure('visible','off','position',[100 100 640 480]);" + code)
        state = self.wait()
        self.assertFalse(state['error'], state)
        return state

    def snapshot(self, code):
        output = self.work / 'sgtitle.json'
        self.run_code(code + ";d=__mf_figure_data__(gcf);assert(d.supported);" +
                      f"fid=fopen({Kernel.quote(output)},'w');fputs(fid,jsonencode(d));fclose(fid);")
        return json.loads(output.read_text())

    def test_centered_default_and_exported_source_geometry(self):
        data = self.snapshot("a=subplot(2,2,1);plot(1:3);title('Panel');p=get(a,'position');" +
                             "h=sgtitle('Figure title');assert(gca==a);" +
                             "assert(strcmp(get(h,'fitboxtotext'),'off'));assert(get(h,'fontsize')==15);")
        visible = [a for a in data['axes'] if a['visible']][0]
        self.assertLess(sum(visible['position'][1::2]), .925)
        self.assertAlmostEqual(sum(visible['title_layout_position'][1::2]), .925)
        heading = next(s for a in data['axes'] for s in a['series'] if s.get('figure_title'))
        self.assertEqual(heading['position'][0], .5)
        self.assertAlmostEqual(heading['position'][1], .9875)
        self.assertEqual((heading['font_size'], heading['font_weight'], heading['vertical_alignment']), (15, 'normal', 'top'))

    def test_repeat_and_changed_title_height_do_not_accumulate(self):
        self.run_code("a=subplot(2,2,1);plot(1:3);sgtitle('one');p=get(a,'position');" +
                      "sgtitle('replacement');assert(isequal(p,get(a,'position')));" +
                      "sgtitle({'one','two'});q=get(a,'position');assert(q(4)<p(4));" +
                      "sgtitle('one');assert(isequal(p,get(a,'position')));" +
                      "assert(numel(findall(gcf,'tag','__mf_sgtitle__'))==1);")

    def test_explicit_title_properties_and_position(self):
        self.run_code("a=subplot(2,2,1);plot(1:3);p=get(a,'position');" +
                      "h=sgtitle('left','horizontalalignment','left','fontsize',24,'fontweight','bold','color',[.2 .4 .6]);" +
                      "t=findall(h,'type','text');assert(get(t,'position')(1)==0);" +
                      "assert(get(h,'fontsize')==24);assert(isequal(get(h,'color'),[.2 .4 .6]));" +
                      "q=get(a,'position');h=sgtitle('custom','position',[.2 .4 .5 .1]);" +
                      "assert(isequal(get(h,'position'),[.2 .4 .5 .1]));assert(isequal(q,get(a,'position')));")

    def test_manual_axes_and_post_title_edits_remain_untouched(self):
        self.run_code("a=subplot(2,2,1);plot(1:3);set(a,'position',[.1 .6 .3 .25]);" +
                      "b=axes('position',[.55 .6 .3 .25]);plot(1:3);" +
                      "sgtitle('one');assert(isequal(get(a,'position'),[.1 .6 .3 .25]));" +
                      "assert(isequal(get(b,'position'),[.55 .6 .3 .25]));" +
                      "tiledlayout(1,2);a=nexttile();plot(1:3);sgtitle('one');" +
                      "set(a,'position',[.1 .5 .3 .3]);sgtitle('two');sgtitle('three');" +
                      "assert(isequal(get(a,'position'),[.1 .5 .3 .3]));")

    def test_tiled_axes_created_after_title_share_reserved_grid(self):
        data = self.snapshot("tiledlayout(1,2);a=nexttile();plot(1:3);sgtitle('one');" +
                             "b=nexttile();plot(1:3);assert(isequal(get(a,'position')([2 4]),get(b,'position')([2 4])));" +
                             "p=get(a,'position');sgtitle('one');assert(isequal(p,get(a,'position')));")
        axes = [a for a in data['axes'] if a['visible']]
        self.assertEqual(len(axes), 2)
        self.assertTrue(all('title_layout_position' in a for a in axes))

    def test_multiline_short_figure_has_top_anchor_and_more_room(self):
        data = self.snapshot("set(gcf,'position',[100 100 640 220]);a=subplot(2,2,1);plot(1:3);title('Panel');" +
                             "h=sgtitle({'first','second'});th=findall(h,'type','text');" +
                             "assert(get(th,'position')(2)<1);assert(get(h,'position')(2)>sum(get(a,'position')([2 4])));")
        heading = next(s for a in data['axes'] for s in a['series'] if s.get('figure_title'))
        self.assertEqual(heading['lines'], ['first', 'second'])
        self.assertEqual(heading['position'][0], .5)

    def test_explicit_figure_target_preserves_current_axes(self):
        self.run_code("f1=gcf;subplot(2,2,1);plot(1:3);f2=figure('visible','off');a=axes();plot(1:3);" +
                      "sgtitle(f1,'target');assert(gcf==f2);assert(gca==a);")

    def test_post_creation_title_geometry_edits_end_automatic_marker(self):
        for edit in ["set(h,'position',[.2 .4 .5 .1])", "set(h,'units','pixels')"]:
            with self.subTest(edit=edit):
                data = self.snapshot("a=subplot(2,2,1);plot(1:3);h=sgtitle('User title');" +
                                     edit + ";p=get(h,'position');u=get(h,'units');")
                heading = next(s for a in data['axes'] for s in a['series']
                               if s['kind'] == 'text' and s['lines'] == ['User title'])
                self.assertFalse(heading.get('figure_title', False))
                self.k.submit("assert(isequal(p,get(h,'position')));assert(isequal(u,get(h,'units')));")
                state = self.wait()
                self.assertFalse(state['error'], state)

    def test_extraction_does_not_mutate_layout_or_title(self):
        self.run_code("a=subplot(2,2,1);plot(1:3);h=sgtitle('title');" +
                      "p=get(a);q=get(h);ap=getappdata(a);hp=getappdata(h);" +
                      "d=__mf_figure_data__(gcf);assert(d.supported);" +
                      "assert(isequaln(p,get(a)));assert(isequaln(q,get(h)));" +
                      "assert(isequaln(ap,getappdata(a)));assert(isequaln(hp,getappdata(h)));")


if __name__ == '__main__':
    unittest.main()
