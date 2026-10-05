function tracker=__mf_publish_figures__()
% Private publisher state. Supported: synchronous Octave figure/axes and
% line, text, image, patch, surface, scatter, light, hggroup render properties.
% Window metadata and UI chrome are excluded; embedded GUI controls fail.
% Copy-on-write snapshots retain original arrays until the block ends. Exact
% comparisons scan all data in bounded chunks; no listeners or dirty flags
% (which drawnow/user code can reset) are trusted. No root appdata is used.
  before={};
  active=false;
  snapshot=@__mf_publish_snapshot__;
  equal=@__mf_publish_equal__;
  tracker=@dispatch;

  function changed=dispatch(action,varargin)
    changed=[];
    if strcmp(action,'begin')
      if active,error(__mf_text__('A publishing figure section is already active.', 'Yayın grafik bölümü zaten etkin.'));endif
      before=snapshot();
      active=true;
    elseif strcmp(action,'end')
      if ~active,error(__mf_text__('The private state of the publishing figure section was not found.', 'Yayın grafik bölümünün özel durumu bulunamadı.'));endif
      % MATLAB R2025b PublishFigures.leavingCell: save root order before
      % drawnow, then reverse that order for the complete changed set.
      order=flipud(allchild(0)(:));
      after=snapshot();
      for k=1:numel(after)
        previous=find(cellfun(@(item)item.handle==after{k}.handle,before),1);
        if isempty(previous)||~equal(before{previous},after{k})
          changed(end+1)=after{k}.handle;
        endif
      endfor
      missing=setdiff(changed(:),order);
      changed=[order(ismember(order,changed));missing(:)];
      before={};
      active=false;
    elseif strcmp(action,'print')
      previous_current=get(0,'currentfigure');
      unwind_protect
        print(varargin{:});
      unwind_protect_cleanup
        if isempty(previous_current)||isgraphics(previous_current,'figure')
          set(0,'currentfigure',previous_current);
        endif
      end_unwind_protect
    elseif strcmp(action,'cleanup')
      before={};
      active=false;
    else
      error(__mf_text__('Unknown publishing figure operation.', 'Bilinmeyen yayın grafik işlemi.'));
    endif
  endfunction
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
