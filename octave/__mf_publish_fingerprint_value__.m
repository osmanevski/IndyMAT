function signature=__mf_publish_fingerprint_value__(value)
% Full-data, bounded-buffer SHA-256 chain (not the SHA-256 of one giant JSON).
% Dense numeric/logical/char arrays, cells and plain structs only. Capture uses
% exact cached-value comparisons; this helper is for diagnostics/regressions.
  header=jsonencode(struct('class',class(value),'size',size(value)));
  signature=hash('sha256',header);
  if isnumeric(value)||islogical(value)||ischar(value)
    if issparse(value),error(__mf_text__('Sparse arrays are not supported in published figure properties.', 'Yayın grafik özelliklerinde seyrek diziler desteklenmiyor.'));endif
    for first=1:65536:numel(value)
      chunk=value(first:min(first+65535,numel(value)));
      if islogical(chunk)||ischar(chunk),bytes=uint8(chunk);else,bytes=typecast(chunk(:),'uint8');endif
      digest=hash('sha256',char(bytes(:).'));
      signature=hash('sha256',[signature digest]);
    endfor
  elseif iscell(value)
    for k=1:numel(value)
      signature=hash('sha256',[signature __mf_publish_fingerprint_value__(value{k})]);
    endfor
  elseif isstruct(value)
    names=sort(fieldnames(value));
    signature=hash('sha256',[signature jsonencode(names)]);
    for k=1:numel(value)
      for j=1:numel(names)
        signature=hash('sha256',[signature __mf_publish_fingerprint_value__(value(k).(names{j}))]);
      endfor
    endfor
  else
    error(__mf_text__('The %s class is not supported in published figure properties.', 'Yayın grafik özelliklerinde %s sınıfı desteklenmiyor.'),class(value));
  endif
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
