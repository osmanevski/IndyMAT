function varargout = dec2base (varargin)
% DEC2BASE Two/three-argument real numeric forms reject negative values.
% This narrow validation preserves native conversion of nonnegative values.
% Cell inputs, custom digit alphabets and Octave's fourth-argument extension
% delegate unchanged; no general MATLAB conversion compatibility is claimed.
  if ((nargin == 2 || nargin == 3) && isnumeric (varargin{1}) ...
      && isreal (varargin{1}) && isnumeric (varargin{2}) ...
      && any (varargin{1}(:) < 0) && ! native_binary_caller ())
    error ('MATLAB:dec2base:FirstArg', ...
           'First argument must be an array of integers, 0 <= D <= flintmax.');
  endif
  original = __mf_original_function__ ('dec2base');
  [varargout{1:nargout}] = original (varargin{:});
endfunction

function yes = native_binary_caller ()
% Octave's original dec2bin uses dec2base for MATLAB-valid two's complement.
% Match its resolved source file, not merely a user function with that name.
  yes = false;
  stack = dbstack ();
  if (numel (stack) < 3 || ! strcmp (stack(3).name, 'dec2bin')), return; endif
  binary = __mf_original_function__ ('dec2bin');
  info = functions (binary);
  yes = strcmp (canonicalize_file_name (stack(3).file), ...
                canonicalize_file_name (info.file));
endfunction
