function verify_dalga25_visibility(expected_fixed=true)
  % Run in a graphics-capable isolated process, before and after this commit.
  % Old serializer: hidden surface reports dimension=3; hidden Z + log axis
  % falls back with log_3d. New serializer: both supported dimension=2.
  f=figure('visible','off'); unwind_protect
    plot([1 2],[1 2]); hold on;
    h=surface([1 2;1 2],[1 1;2 2],[1 2;2 3],'visible','off'); view(2);
    d=__mf_figure_data__(f); fprintf('hidden surface: supported=%d dimension=%d reason=%s\n',d.supported,d.axes{1}.dimension,d.reason_code);
    expected_dimension=3;if expected_fixed,expected_dimension=2;endif
    assert(d.supported && d.axes{1}.dimension==expected_dimension && numel(d.axes{1}.series)==1);
    delete(h); h=plot3([1 2],[1 2],[3 4],'visible','off'); view(2); set(gca,'xscale','log');
    d=__mf_figure_data__(f); fprintf('hidden Z/log: supported=%d reason=%s\n',d.supported,d.reason_code);
    if expected_fixed
      assert(d.supported && d.axes{1}.dimension==2 && numel(d.axes{1}.series)==1);
    else
      assert(~d.supported && strcmp(d.reason_code,'log_3d'));
    endif
    set(gca,'xscale','linear');set(h,'visible','on');
    d=__mf_figure_data__(f);assert(d.supported && d.axes{1}.dimension==3);
    set(gca,'visible','off');d=__mf_figure_data__(f);
    assert(d.supported && d.axes{1}.dimension==3 && numel(d.axes{1}.series)==2);
  unwind_protect_cleanup
    close(f);
  end_unwind_protect
endfunction
