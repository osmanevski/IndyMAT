## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ZP02–ZP18,ZP21.
function value = get (sys, key)
  if (nargin == 1), value = sys.data; return; endif
  if (iscell(key)), value = cellfun (@(k) get(sys,k),key,'uniformoutput',false); return; endif
  if (strcmpi(key,'lti')), value=sys.lti; return; endif
  aliases = {'tsam','Ts';'inname','InputName';'outname','OutputName';'ingroup','InputGroup';'outgroup','OutputGroup'};
  for i=1:rows(aliases), if(strcmpi(key,aliases{i,1})), key=aliases{i,2}; break; endif; endfor
  names = fieldnames(sys.data); idx=find(strcmpi(key,names));
  if (isempty(idx)), __indymat_zpk_scope__('property'); endif
  value = sys.data.(names{idx});
endfunction
