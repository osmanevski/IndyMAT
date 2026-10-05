function equal=__mf_publish_equal__(left,right)
% Exact render-state comparison. Dense arrays, cells and plain structs only.
% Numeric comparisons use at most 65536 elements per slice. NaNs at the same
% positions compare equal, as do signed zeros; both render identically.
  equal=false;
  if ~strcmp(class(left),class(right))||~isequal(size(left),size(right)),return;endif
  if isnumeric(left)||islogical(left)||ischar(left)
    if issparse(left)||issparse(right)
      error(__mf_text__('Sparse arrays are not supported in published figure properties.', 'Yayın grafik özelliklerinde seyrek diziler desteklenmiyor.'));
    endif
    for first=1:65536:numel(left)
      last=min(first+65535,numel(left));
      if ~isequaln(left(first:last),right(first:last)),return;endif
    endfor
  elseif iscell(left)
    for k=1:numel(left)
      if ~__mf_publish_equal__(left{k},right{k}),return;endif
    endfor
  elseif isstruct(left)
    names=sort(fieldnames(left));
    if ~isequal(names,sort(fieldnames(right))),return;endif
    for k=1:numel(left)
      for j=1:numel(names)
        if ~__mf_publish_equal__(left(k).(names{j}),right(k).(names{j})),return;endif
      endfor
    endfor
  else
    error(__mf_text__('The %s class is not supported in published figure properties.', 'Yayın grafik özelliklerinde %s sınıfı desteklenmiyor.'),class(left));
  endif
  equal=true;
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
