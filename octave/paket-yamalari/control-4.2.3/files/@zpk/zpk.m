## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows Z01–Z21,D01–D21,CV02,CV04,CV05,PC09.
function sys = zpk (varargin)
  superiorto ('tf', 'double', 'pid');
  props = struct ('Z', {{}}, 'P', {{}}, 'K', [], 'DisplayFormat', 'roots', ...
    'Variable', 's', 'IODelay', 0, 'InputDelay', 0, 'OutputDelay', 0, ...
    'InputName', {{''}}, 'InputUnit', {{''}}, 'InputGroup', struct (), ...
    'OutputName', {{''}}, 'OutputUnit', {{''}}, 'OutputGroup', struct (), ...
    'Notes', reshape (string ({}), 0, 1), 'UserData', [], 'Name', '', ...
    'Ts', 0, 'TimeUnit', 'seconds', 'SamplingGrid', struct ());
  z = {}; p = {}; k = []; options = {};
  if (nargin == 0)
  elseif (isa (varargin{1}, 'zpk'))
    sys = varargin{1}; if (nargin > 1), sys = set (sys, varargin{2:end}); endif; return;
  elseif (isa (varargin{1}, 'lti'))
    input = varargin{1}; __indymat_zpk_scope__ (input);
    if (isa (input, 'ss'))
      [zv, k] = zero (input); pv = pole (input);
    else
      [num, den] = tfdata (tf (input), 'vector');
      num = num(find (num != 0, 1):end); den = den(find (den != 0, 1):end);
      if (isempty (num)), zv = zeros (0,1); k = 0; else zv = roots (num); k = num(1)/den(1); endif
      pv = roots (den);
    endif
    z = {zv(:)}; p = {pv(:)}; options = varargin(2:end);
    props.InputName = get (input, 'inname'); props.OutputName = get (input, 'outname');
    props.Name = get (input, 'name'); props.UserData = get (input, 'userdata');
  elseif (ischar (varargin{1}))
    if (! strcmp (varargin{1}, 's')), __indymat_zpk_scope__ ('discrete'); endif
    z = {0}; p = {zeros(0,1)}; k = 1; options = varargin(2:end);
  elseif (nargin == 1 || (nargin > 1 && ischar (varargin{2})))
    k = varargin{1}; if (! isempty(k)), z = {zeros(0,1)}; p = z; endif; options = varargin(2:end);
  elseif (nargin >= 3)
    z = varargin{1}; p = varargin{2}; k = varargin{3}; options = varargin(4:end);
    if(isempty(k)), __indymat_zpk_scope__('call'); endif
    if (! iscell(z)), z = {z}; endif; if (! iscell(p)), p = {p}; endif
    if (! isempty(options) && isnumeric(options{1}))
      if (! isequal(options{1},0)), __indymat_zpk_scope__ ('discrete'); endif; options(1) = [];
    endif
  else
    __indymat_zpk_scope__ ('call');
  endif
  if(isnumeric(k) && !isempty(k) && !isscalar(k)), __indymat_zpk_scope__('mimo'); endif
  if(!isempty(options) && isnumeric(options{1}))
    if(!isequal(options{1},0)), __indymat_zpk_scope__('discrete'); endif; options(1)=[];
  endif
  props.Z = z; props.P = p; props.K = k;
  n = double (! isempty(k));
  sys = class (struct ('data', props), 'zpk', lti (n,n,0));
  if (n), sys = __set__ (sys,'Z',z); sys = __set__ (sys,'P',p); sys = __set__ (sys,'K',k); endif
  if (! isempty(options)), sys = set (sys,options{:}); endif
endfunction
