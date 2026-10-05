classdef uy_yap_value
properties
Data = 1
end
properties (Access=private)
Secret = 9
end
properties (SetAccess=private)
ReadOnly = 4
end
properties (Constant)
Limit = 12
end
properties (Dependent)
Twice
end
methods
function obj=uy_yap_value(x)
if nargin>0, obj.Data=x; end
end
function y=get.Twice(obj)
y=2*obj.Data;
end
function obj=set.Twice(obj,y)
obj.Data=y/2;
end
function obj=set.Data(obj,y)
obj.Data=abs(y);
end
function y=readSecret(obj)
y=obj.Secret;
end
function y=square(obj)
y=obj.Data^2;
end
function c=plus(a,b)
c=uy_yap_value(a.Data+b.Data);
end
function y=eq(a,b)
y=a.Data==b.Data;
end
function disp(obj)
fprintf('uy_yap_value:%g\n',obj.Data);
end
end
end
