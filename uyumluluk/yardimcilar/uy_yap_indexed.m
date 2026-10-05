classdef uy_yap_indexed
properties
Data = [4 5 6]
end
methods
function y=subsref(obj,s)
if strcmp(s(1).type,'()')
y=obj.Data(s(1).subs{:});
else
y=builtin('subsref',obj,s);
end
end
function y=end(obj,k,n)
y=numel(obj.Data);
end
end
end
