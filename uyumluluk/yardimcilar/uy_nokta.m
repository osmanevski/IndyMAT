classdef uy_nokta
  properties
    x = 0
    y = 0
  end
  methods
    function obj = uy_nokta(x, y)
      if nargin == 2
        obj.x = x;
        obj.y = y;
      end
    end
    function r = norm(obj)
      r = sqrt(obj.x^2 + obj.y^2);
    end
    function obj = olcekle(obj, k)
      obj.x = obj.x * k;
      obj.y = obj.y * k;
    end
  end
  methods (Static)
    function obj = birim()
      obj = uy_nokta(1, 0);
    end
  end
end
