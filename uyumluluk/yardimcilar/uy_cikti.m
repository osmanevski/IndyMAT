function varargout = uy_cikti(varargin)
  for k = 1:nargout
    varargout{k} = k * numel(varargin);
  end
end
