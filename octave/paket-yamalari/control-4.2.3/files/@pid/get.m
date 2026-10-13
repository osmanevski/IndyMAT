## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function varargout=get(sys,varargin)
  if(nargin==1)
    out=struct(); keys=properties(sys);
    for k=1:numel(keys), out.(keys{k})=get(sys,keys{k}); endfor
    varargout{1}=out; return;
  endif
  for j=1:numel(varargin)
    key=lower(varargin{j});
    switch key
      case "lti", v=sys.lti;
      case "kp", v=sys.Kp;
      case "ki", v=sys.Ki;
      case "kd", v=sys.Kd;
      case "tf", v=sys.Tf;
      case {"iformula","dformula","name"}
        if(strcmp(key,"name")), v=sys.modelname; else, v=""; endif
      case {"ts","tsam","inputdelay","outputdelay"}, v=0;
      case {"inputname","outputname","inputunit","outputunit","inname","outname"}, v={""};
      case {"inputgroup","outputgroup","samplinggrid","ingroup","outgroup"}, v=struct();
      case "notes", v=string(cell(0,1));
      case "userdata", v=[];
      case "timeunit", v="seconds";
      otherwise, error("IndyMAT:control:UnsupportedProperty","This property is outside the documented control representation subset.");
    endswitch
    varargout{j}=v;
  endfor
endfunction
