## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows CV01,CV03,D05,D19,D20; complex conversion is documented Octave limit.
function [result, parent] = __sys2tf__ (sys)
  if(isempty(sys.data.K)), result=tf(); else
    num=sys.data.K*poly(sys.data.Z{1}); den=poly(sys.data.P{1});
    if(!isreal(num) || !isreal(den))
      error('IndyMAT:control:UnsupportedComplexConversion','The installed Octave tf representation supports real polynomial coefficients only.');
    endif
    result=tf(num,den);
  endif
  parent=sys.lti;
endfunction
