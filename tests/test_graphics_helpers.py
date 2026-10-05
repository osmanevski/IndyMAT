import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class GraphicsPureHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = "addpath('%s'); %s" % (str(ROOT / 'octave' / 'compat'), body)
        return subprocess.run([OCTAVE, '--quiet', '--eval', command], cwd=ROOT,
                              text=True, capture_output=True, timeout=20)

    def test_color_parser_rgb_names_and_hex(self):
        result = self.run_octave("c=__mf_graphics_parse_colors__({'red','g','#0000FF'}); assert(isequal(c,[1 0 0;0 1 0;0 0 1]));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_color_parser_validates_numeric_range(self):
        result = self.run_octave("try, __mf_graphics_parse_colors__([1 0 2]); error('expected validation'); catch err, assert(!isempty(strfind(err.message,'[0,1]'))); end_try_catch")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_colororder_numeric_input_is_validated_as_colors_before_creating_axes(self):
        result = self.run_octave("for c={[0 1 2],[1 0 0;0 0 2]}, try, colororder(c{1}); error('expected validation'); catch err, assert(!isempty(strfind(err.message,'[0,1]'))); end_try_catch; endfor")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tile_positions_are_bounded_and_ordered(self):
        result = self.run_octave("p=__mf_tiledlayout_positions__(2,3,'tight','compact'); assert(isequal(size(p),[6 4])); assert(all(p(:,1)>=0) && all(p(:,2)>=0) && all(p(:,1)+p(:,3)<=1) && all(p(:,2)+p(:,4)<=1)); assert(p(1,2)>p(4,2) && p(1,1)<p(2,1));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tile_positions_validate_grid_and_spacing(self):
        result = self.run_octave("try, __mf_tiledlayout_positions__(0,2,'tight','loose'); error('expected validation'); catch err, assert(!isempty(strfind(err.message,'positive integers'))); end_try_catch; try, __mf_tiledlayout_positions__(2,2,'flow','loose'); error('expected validation'); catch err, assert(!isempty(strfind(err.message,'must be compact'))); end_try_catch")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tile_span_covers_cells_and_returns_bounding_box(self):
        result = self.run_octave("p=__mf_tiledlayout_positions__(2,3,'tight','compact'); [box,cells]=__mf_tiledlayout_span__(p,2,3,1,[2 2]); assert(isequal(cells,[1 2 4 5])); assert(box(3)>p(1,3) && box(4)>p(1,4));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tile_span_rejects_out_of_bounds(self):
        result = self.run_octave("p=__mf_tiledlayout_positions__(2,2,'tight','compact'); try, __mf_tiledlayout_span__(p,2,2,2,[1 2]); error('expected validation'); catch err, assert(!isempty(strfind(err.message,'exceeds layout'))); end_try_catch")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_parula_explicit_count_is_numeric_and_bounded(self):
        result = self.run_octave("c=parula(23); assert(isequal(size(c),[23 3]) && isa(c,'double') && all(c(:)>=0) && all(c(:)<=1));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_hex_rejects_partial_and_nonhex_pairs(self):
        result = self.run_octave("for c={'#11223x','#x12233','#112233junk'}, failed=false; try, __mf_graphics_parse_colors__(c{1}); catch, failed=true; end_try_catch; assert(failed); endfor; assert(isequal(__mf_graphics_parse_colors__('#11aAfF'),[17 170 255]/255));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_large_grids_remain_positive_and_bounded(self):
        result = self.run_octave("for dims={[1 20],[20 1],[50 50]}, d=dims{1}; p=__mf_tiledlayout_positions__(d(1),d(2),'loose','loose'); assert(all(p(:,3:4)(:)>0)); assert(all(p(:,1:2)(:)>=0)); assert(all((p(:,1:2)+p(:,3:4))(:)<=1+eps)); endfor")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_first_free_tile_and_span_search(self):
        result = self.run_octave("assert(__mf_tiledlayout_find__(1,2,[0 1],[1 1])==1); assert(__mf_tiledlayout_find__(2,3,[1 0 0 0 0 0],[1 2])==2); assert(__mf_tiledlayout_find__(2,3,[1 0 0 0 0 0],[2 2])==2); failed=false; try, __mf_tiledlayout_find__(1,2,[1 0],[1 2]); catch, failed=true; end_try_catch; assert(failed);")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_polar_options_without_linespec_and_invalid_specs(self):
        result = self.run_octave("for opts={{'LineWidth',2},{'color',[1 0 0]},{'Marker','o'}}, [s,p]=__mf_polarplot_options__(opts{1}); assert(isempty(s) && isequal(p,opts{1})); endfor; [s,p]=__mf_polarplot_options__({'--or','LineWidth',2}); assert(strcmp(s,'--or')); for bad={'rr','---','ooo','purple'}, failed=false; try, __mf_polarplot_options__({bad{1}}); catch, failed=true; end_try_catch; assert(failed); endfor")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_snapshot_properties_strip_callbacks_and_handle_state(self):
        result = self.run_octave("p=struct('color',[1 0 0],'deletefcn',@()error('must not run'),'createfcn',@()0,'buttondownfcn','bad','userdata',42,'__appdata__',struct(),'parent',7,'contextmenu',9,'defaultlightcreatefcn',@()0); clean=__mf_graphics_snapshot_props__(p,fieldnames(p)); assert(isequal(fieldnames(clean),{'color'}) && isequal(clean.color,[1 0 0]));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_export_bounds_include_labels_and_preserve_plot_aspect(self):
        result = self.run_octave("b=__mf_export_bounds__([100 50 600 200],[40 25 15 30],2); assert(isequal(b,[58 23 659 259])); b=__mf_export_bounds__([100 50 600 200;100 50 600 200],[40 25 0 30;0 25 50 30],0); assert(isequal(b,[60 25 690 255]));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_colororder_getter_without_figure_has_no_graphics_side_effect(self):
        result = self.run_octave("assert(isempty(get(0,'currentfigure'))); c=colororder(); assert(isequal(c,get(0,'defaultaxescolororder')) && isempty(get(0,'currentfigure')));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_yyaxis_rejects_explicit_target_before_mutation(self):
        result = self.run_octave("failed=false; try, yyaxis(-1,'right'); catch err, failed=~isempty(strfind(err.message,'explicit axes targets are unsupported')); end_try_catch; assert(failed && isempty(get(0,'currentfigure')));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_export_callback_default_names_are_accepted_by_octave(self):
        result = self.run_octave("args=__mf_export_defaults__(); assert(~isempty(args)); for k=1:2:numel(args), before=get(0,args{k}); assert(isempty(args{k+1})); set(0,args{k},args{k+1}); assert(isempty(get(0,args{k}))); set(0,args{k},before); endfor")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_colororder_recolors_exact_sequence_with_different_cycle_lengths(self):
        result = self.run_octave("old=[1 0 0;0 0 1]; new=[0 1 0;1 0 1;0 1 1]; current=old([1 2 1 2 1],:); [mapped,recolor]=__mf_colororder_recolor__(old,new,current); assert(recolor && isequal(mapped,new([1 2 3 1 2],:))); [mapped,recolor]=__mf_colororder_recolor__(old,[0 1 0],current); assert(recolor && isequal(mapped,repmat([0 1 0],5,1)));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_colororder_mismatch_keeps_entire_existing_color_list(self):
        result = self.run_octave("old=[1 0 0;0 0 1]; current=[old(1,:);0.2 0.3 0.4;old(1,:)]; [mapped,recolor]=__mf_colororder_recolor__(old,[0 1 0],current); assert(~recolor && isequal(mapped,current)); old=[0.2 0.3 0.4]; current=old; current(1)=current(1)+eps; [mapped,recolor]=__mf_colororder_recolor__(old,[0 1 0],current); assert(~recolor && isequal(mapped,current));")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_colororder_empty_and_non_rgb_line_colors(self):
        result = self.run_octave("[mapped,recolor]=__mf_colororder_recolor__([1 0 0],[0 1 0],zeros(0,3)); assert(recolor && isequal(size(mapped),[0 3])); current={[1 0 0],'none'}; [mapped,recolor]=__mf_colororder_recolor__([1 0 0],[0 1 0],current); assert(~recolor && isequal(mapped,current)); [mapped,recolor]=__mf_colororder_recolor__([1 0 0],[0 1 0],{[1 0 0]}); assert(recolor && isequal(mapped,[0 1 0]));")
        self.assertEqual(result.returncode, 0, result.stderr)


class GraphicsToolkitRegressionTests(unittest.TestCase):
    def run_graphics(self, body):
        # Use the same short-lived Qt process as the compatibility harness.
        # The CLI binary lacks Qt even on the orchestrator's graphics-capable host.
        import sys
        import tempfile
        sys.path.insert(0, str(ROOT / 'scripts'))
        import compat
        code = "if isempty(available_graphics_toolkits()), error('GRAPHICS_TOOLKIT_UNAVAILABLE'); end; " + body
        case = {'id': 'graphics-regression', 'code': [code]}
        with tempfile.TemporaryDirectory() as tmp:
            results, _, _ = compat.execute([case], Path(tmp), ROOT / 'uyumluluk' / 'yardimcilar', timeout=45)
        result = results[case['id']]
        if 'GRAPHICS_TOOLKIT_UNAVAILABLE' in result['message']:
            self.skipTest('graphics toolkit unavailable in this environment')
        self.assertEqual(result['status'], 'OK', result['message'])

    def test_yyaxis_plot_preserves_rulers_and_replacement(self):
        self.run_graphics("yyaxis left; plot(1:3); left=gca(); yyaxis right; first=plot(10:10:30); right=gca(); second=plot(20:20:60); assert(~isgraphics(first) && isgraphics(second)); assert(strcmp(get(right,'yaxislocation'),'right') && strcmp(get(right,'color'),'none')); xlim([0 8]); assert(isequal(get(left,'xlim'),[0 8])); assert(numel(findall(gcf(),'type','line'))==2);")

    def test_yyaxis_hold_linkaxes_and_explicit_legend(self):
        self.run_graphics("f=figure(); yyaxis left; h1=plot(1:3); left=gca(); hold on; yyaxis right; h2=plot(10:10:30); h3=plot(20:20:60); right=gca(); assert(ishold() && all(isgraphics([h1 h2 h3]))); hold off; assert(~ishold(left)); lg=legend([h1 h2 h3],{'left','right1','right2'}); assert(numel(get(lg,'string'))==3); a=axes('parent',f); linkaxes([left a],'x'); xlim(a,[0 9]); assert(isequal(get(right,'xlim'),[0 9]));")

    def test_yyaxis_units_delete_and_reset(self):
        self.run_graphics("f=figure(); a=axes('units','pixels','position',[70 60 300 200]); yyaxis left; yyaxis right; b=gca(); assert(strcmp(get(b,'units'),'pixels') && isequal(get(a,'position'),get(b,'position'))); delete(b); assert(~isgraphics(a) && ~isgraphics(b)); axes('parent',f); yyaxis left; yyaxis right; survivor=gca(); cla reset; assert(isgraphics(survivor,'axes') && numel(findall(f,'type','axes'))==1); assert(~isappdata(survivor,'__mf_yyaxis_pair__')); plot(1:3); assert(gca()==survivor);")

    def test_colororder_applies_palette_and_warns_when_lines_do_not_match(self):
        self.run_graphics("f=figure(); h=plot(1:3); hold on; explicit=plot(3:-1:1,'Color',get(h,'color')); beforeline=get(h,'color'); lastwarn(''); colororder([1 0 0]); [message,id]=lastwarn(); assert(strcmp(id,'IndyMAT:colororder:existingLines') && ~isempty(strfind(message,'not recoloured'))); assert(isequal(get(f,'defaultaxescolororder'),[1 0 0]) && isequal(get(gca(),'colororder'),[1 0 0])); assert(isequal(get(h,'color'),beforeline) && isequal(get(explicit,'color'),beforeline));")

    def test_tiledlayout_replaces_complete_dual_axes(self):
        self.run_graphics("f=figure(); yyaxis left; a=gca(); yyaxis right; b=gca(); tiledlayout(1,1); nexttile(); assert(~isgraphics(a) && ~isgraphics(b) && numel(findall(f,'type','axes'))==1);")

    def test_export_callbacks_do_not_escape_or_leak_figures(self):
        self.run_graphics("f=figure(); h=plot(1:3,'DeleteFcn',@(~,~)delete(f)); old=get(0,'defaultfigurecloserequestfcn'); restore=onCleanup(@()set(0,'defaultfigurecloserequestfcn',old)); set(0,'defaultfigurecloserequestfcn',@(~,~)0); before=numel(findall(0,'type','figure')); file=[tempname() '.png']; exportgraphics(f,file); delete(file); assert(isfigure(f) && isgraphics(h) && numel(findall(0,'type','figure'))==before); failed=false; try, exportgraphics(f,fullfile(tempname(),'bad.png')); catch, failed=true; end; assert(failed && isfigure(f) && numel(findall(0,'type','figure'))==before);")

    def test_export_snapshot_includes_hidden_title_legend_colorbar_once(self):
        self.run_graphics("f=figure(); imagesc(magic(3)); hold on; h=plot(1:3,'DisplayName','series'); legend(h,'series'); colorbar(); sgtitle('Hidden overlay title'); [clone,ax]=__mf_export_snapshot__(f,[]); guard=onCleanup(@()delete(clone)); assert(numel(findall(clone,'type','axes','tag','legend'))==1); assert(numel(findall(clone,'type','axes','tag','colorbar'))==1); assert(~isempty(findall(clone,'type','text','string','Hidden overlay title'))); assert(all(isgraphics(ax,'axes')));")

    def test_export_figure_pixels_follow_source_aspect_and_resolution(self):
        self.run_graphics("f=figure('units','pixels','position',[100 100 720 240]); plot(1:3); file=[tempname() '.png']; dpi=100; exportgraphics(f,file,'Resolution',dpi); info=imfinfo(file); delete(file); expected=[720 240]*dpi/get(0,'screenpixelsperinch'); assert(all(abs([info.Width info.Height]-expected)<=2)); assert(abs(info.Width/info.Height-3)<0.04);")

    def test_export_axes_snapshot_keeps_label_insets_and_both_sides(self):
        self.run_graphics("f=figure('units','pixels','position',[100 100 720 360]); yyaxis left; plot(1:3); xlabel('X label'); ylabel('Left label'); a=gca(); yyaxis right; plot(10:10:30); ylabel('Right label'); [clone,copies]=__mf_export_snapshot__(a,[]); guard=onCleanup(@()delete(clone)); assert(numel(copies)==2 && numel(findall(clone,'type','line'))==2); canvas=getpixelposition(clone); for ax=copies, p=getpixelposition(ax); inset=get(ax,'tightinset'); assert(all(p(1:2)-inset(1:2)>=0)); assert(all(p(1:2)+p(3:4)+inset(3:4)<=canvas(3:4)+2)); endfor")

    def test_colororder_axes_setter_succeeds_without_hold_and_documents_reset(self):
        self.run_graphics("a=axes(); before=get(gcf(),'defaultaxescolororder'); colororder(a,[1 0 0]); assert(isequal(get(a,'colororder'),[1 0 0]) && strcmp(get(a,'nextplot'),'replace')); plot(a,1:3); assert(isequal(get(a,'colororder'),before)); colororder([0 1 0]); plot(a,1:3); assert(isequal(get(a,'colororder'),[0 1 0]));")

    def test_export_suppresses_inherited_creation_callbacks(self):
        self.run_graphics("f=figure(); plot(1:3); old=get(0,'defaultlinecreatefcn'); restore=onCleanup(@()set(0,'defaultlinecreatefcn',old)); set(0,'defaultlinecreatefcn',@(~,~)delete(f)); clone=__mf_export_snapshot__(f,[]); assert(isfigure(f)); delete(clone); assert(isfigure(f));")

    def test_yyaxis_recovers_incomplete_pair_metadata(self):
        self.run_graphics("yyaxis left; left=gca(); yyaxis right; right=gca(); setappdata(left,'__mf_yyaxis_pair__',[left;-1000]); axes(left); yyaxis left; pair=getappdata(left,'__mf_yyaxis_pair__'); assert(numel(pair)==2 && all(isgraphics(pair,'axes')) && ~isgraphics(right) && numel(findall(gcf(),'type','axes'))==2); delete(gcf());")

    def test_tiledlayout_options_do_not_confuse_row_count_with_figure_handle(self):
        self.run_graphics("f=figure(1); tiledlayout(1,2,'Padding','tight'); a=nexttile(); b=nexttile(); assert(a~=b && numel(findall(f,'type','axes'))==2);")

    def test_colororder_line_creation_order_and_next_cycle_index(self):
        self.run_graphics("old=[1 0 0;0 0 1]; new=[0 1 0;1 0 1;0 1 1]; colororder(old); h=plot([1 2 3 4 5;2 3 4 5 6]); lastwarn(''); colororder(gca(),new); [msg,id]=lastwarn(); assert(isempty(id)); for k=1:5, assert(isequal(get(h(k),'color'),new(mod(k-1,3)+1,:))); endfor; hold on; extra=plot([5 6]); assert(isequal(get(extra,'color'),new(3,:)));")


class GraphicsCompatibilityContractTests(unittest.TestCase):
    def test_only_one_behavior_case_exercises_explicit_yyaxis_target(self):
        import re
        import sys
        sys.path.insert(0, str(ROOT / 'scripts'))
        import compat
        cases = compat.load_cases(ROOT / 'uyumluluk' / 'durumlar')
        targeted = [case['id'] for case in cases
                    if re.search(r'\byyaxis\s*\(', compat.executable_text('\n'.join(case['code'])))]
        self.assertEqual(targeted, ['grafiky-yyaxis-target'])

def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this
