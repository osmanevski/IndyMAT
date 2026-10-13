## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function out=set(sys,varargin)
  if(isempty(varargin) || mod(numel(varargin),2))
    error("Control:general:OptionHelper1","Invalid syntax. Use name/value pairs to specify options and property values.");
  endif
  out=sys;
  for j=1:2:numel(varargin)
    key=lower(varargin{j}); v=varargin{j+1};
    switch key
      case {"kp","ki","kd","tf"}
        names={"Kp","Ki","Kd","Tf"}; idx=find(strcmp(key,lower(names))); out.(names{idx})=v;
      case {"iformula","dformula"}
        if(!ischar(v) || !any(strcmp(v,{"ForwardEuler","BackwardEuler","Trapezoidal"})))
          error("Control:ltiobject:pidInvalidFormula",'Valid discretization methods for PID controllers are: "ForwardEuler", "BackwardEuler" and "Trapezoidal".');
        endif
      case {"ts","tsam"}
        if(!isequal(v,0)), error("IndyMAT:control:UnsupportedDiscreteTime","This representation supports continuous-time models with Ts equal to zero only."); endif
      case {"inputdelay","outputdelay"}
        error("Control:ltiobject:ReadOnlyDelay", 'The property "%s" is read-only for the class "pid".',varargin{j});
      case "name"
        if(!ischar(v)||rows(v)>1), error("IndyMAT:control:UnsupportedProperty","This property is outside the documented control representation subset."); endif
        out.modelname=v;
      otherwise, error("IndyMAT:control:UnsupportedProperty","This property is outside the documented control representation subset.");
    endswitch
  endfor
  out=__pid_validate__(out);
  if(nargout==0)
    name=inputname(1);
    if(isempty(name)), error("IndyMAT:control:UnsupportedCallForm","This call form is outside the documented control representation subset."); endif
    assignin("caller",name,out);
  endif
endfunction
