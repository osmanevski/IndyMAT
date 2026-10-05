classdef uy_yap_events < handle
properties
Count = 0
end
events
Tick
end
methods
function fire(obj)
notify(obj,'Tick');
end
function record(obj)
obj.Count=obj.Count+1;
end
end
end
