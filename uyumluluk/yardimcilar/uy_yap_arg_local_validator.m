function y=uy_yap_arg_local_validator(x)
arguments
x {tek}
end
y=x;
end
function tek(x)
if mod(x,2)~=1, error('UyYap:NotOdd','Tek olmali'); end
end
