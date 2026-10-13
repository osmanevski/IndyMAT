function varargout = strfind (varargin)
% STRFIND Reject multiple cell search terms; other forms use Octave's builtin.
% Cellstr sources and a character-vector search term remain supported.
% Singleton cell terms and other pattern types retain Octave's behavior.
  if (nargin >= 2 && iscell (varargin{2}) && ! isscalar (varargin{2}))
    error ('MATLAB:string:MustBeTextOrPatternScalar', 'Search term must be a text or pattern scalar.');
  endif
  [varargout{1:nargout}] = builtin ('strfind', varargin{:});
endfunction
