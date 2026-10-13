## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ZS01–ZS12,ZA01–ZA10,LIMIT01–LIMIT25.
function sys = __set__ (sys,key,value)
  if (strcmpi(key,'lti')), __indymat_zpk_scope__('property'); endif
  aliases = {'tsam','Ts';'inname','InputName';'outname','OutputName';'ingroup','InputGroup';'outgroup','OutputGroup'};
  for i=1:rows(aliases), if(strcmpi(key,aliases{i,1})), key=aliases{i,2}; break; endif; endfor
  names=fieldnames(sys.data); idx=find(strcmpi(key,names));
  if(isempty(idx)), __indymat_zpk_scope__('property'); endif; key=names{idx};
  switch(key)
    case {'Z','P'}
      if (!iscell(value)), value={value}; endif
      if (!isequal(size(value),[1 1])), __indymat_zpk_scope__('mimo'); endif
      v=value{1}; if (!isnumeric(v) || (!isempty(v) && !isvector(v)) || any(!isfinite(v(:))))
        error('IndyMAT:control:InvalidRoots','Roots must be finite numeric vectors.'); endif
      value={double(v(:))};
    case 'K'
      if(isnumeric(value) && !isscalar(value)), __indymat_zpk_scope__('mimo'); endif
      if(!isnumeric(value) || !isscalar(value) || !isfinite(value)), error('IndyMAT:control:InvalidGain','Gain must be a finite numeric scalar.'); endif
      value=double(value);
    case 'Ts'
      if(!isequal(value,0)), __indymat_zpk_scope__('discrete'); endif
    case {'InputDelay','OutputDelay','IODelay'}
      if(!isequal(value,0)), __indymat_zpk_scope__('delay'); endif
    case 'Variable'
      if(!ischar(value) || !strcmp(value,'s')), __indymat_zpk_scope__('discrete'); endif
    case 'DisplayFormat'
      if(!ischar(value) || !any(strcmp(value,{'roots','frequency'}))), __indymat_zpk_scope__('call'); endif
    case {'Name','TimeUnit'}
      if(!ischar(value) || rows(value)>1), __indymat_zpk_scope__('call'); endif
      if(strcmp(key,'TimeUnit') && !strcmp(value,'seconds')), __indymat_zpk_scope__('call'); endif
    case {'InputName','OutputName','InputUnit','OutputUnit'}
      if(ischar(value)), value={value}; endif
      if(!iscell(value) || numel(value)!=1 || !ischar(value{1})), __indymat_zpk_scope__('mimo'); endif
      value=reshape(value,1,1);
    case {'InputGroup','OutputGroup'}
      if(!isstruct(value) || !isscalar(value)), __indymat_zpk_scope__('call'); endif
      fs=fieldnames(value); for i=1:numel(fs), if(!isequal(value.(fs{i}),1)), __indymat_zpk_scope__('mimo'); endif; endfor
    case 'SamplingGrid'
      if(!isstruct(value) || !isempty(fieldnames(value))), __indymat_zpk_scope__('array'); endif
    case 'Notes'
      value=string(value); value=value(:);
  endswitch
  sys.data.(key)=value;
  switch(key)
    case 'InputName', sys.lti=builtin('subsasgn',sys.lti,substruct('.','inname'),value);
    case 'OutputName', sys.lti=builtin('subsasgn',sys.lti,substruct('.','outname'),value);
    case 'InputGroup', sys.lti=builtin('subsasgn',sys.lti,substruct('.','ingroup'),value);
    case 'OutputGroup', sys.lti=builtin('subsasgn',sys.lti,substruct('.','outgroup'),value);
    case 'Name', sys.lti=builtin('subsasgn',sys.lti,substruct('.','name'),value);
    case 'UserData', sys.lti=builtin('subsasgn',sys.lti,substruct('.','userdata'),value);
  endswitch
endfunction
