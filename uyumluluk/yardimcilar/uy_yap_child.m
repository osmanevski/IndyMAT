classdef uy_yap_child < uy_yap_base
methods
function obj=uy_yap_child(x)
if nargin==0, x=5; end
obj@uy_yap_base(x);
end
end
end
