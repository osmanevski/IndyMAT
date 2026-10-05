classdef digitalFilter
% DIGITALFILTER Real scalar FIR subset created by designfilt.
% Read-only design metadata and coefficients; filter(d,X), filtfilt(d,X),
% numeric freqz(d,N), filtord and two-output tf are supported. No IIR, filter
% arrays, mutable coefficients, GPU/tall data or FilterOrder property.
% This class supplies its own methods without shadowing numeric functions.
  properties (SetAccess=private)
    Numerator
    Denominator=1
    Coefficients
    DesignMethod='equiripple'
    FrequencyResponse='lowpass'
    ImpulseResponse='fir'
    PassbandFrequency
    StopbandFrequency
    PassbandRipple
    StopbandAttenuation
    SampleRate
  endproperties
  methods
    function obj = digitalFilter (b, specs)
      if (nargin!=2 || !isnumeric(b) || !isreal(b) || !isrow(b) || isempty(b) || any(!isfinite(b)))
        error ('digitalFilter: construct FIR filters through designfilt');
      endif
      obj.Numerator=b; obj.Coefficients=b;
      obj.PassbandFrequency=specs.PassbandFrequency;
      obj.StopbandFrequency=specs.StopbandFrequency;
      obj.PassbandRipple=specs.PassbandRipple;
      obj.StopbandAttenuation=specs.StopbandAttenuation;
      obj.SampleRate=specs.SampleRate;
    endfunction
    function varargout = filter (obj, x, varargin)
      if (numel(obj)!=1), error ('digitalFilter.filter: filter arrays are unsupported'); endif
      if (!isempty(varargin)), error ('digitalFilter.filter: optional arguments are unsupported'); endif
      if (!isnumeric(x) || !(isa(x,'single') || isa(x,'double')))
        error ('digitalFilter.filter: input must be single or double numeric data');
      endif
      [varargout{1:nargout}]=builtin('filter',obj.Numerator,1,x,varargin{:});
    endfunction
    function y = filtfilt (obj, x)
      if (numel(obj)!=1), error ('digitalFilter.filtfilt: filter arrays are unsupported'); endif
      if (!isnumeric(x) || !(isa(x,'single') || isa(x,'double')) || ndims(x)>2)
        error ('digitalFilter.filtfilt: input must be a single or double vector or matrix');
      endif
      if (isrow(x)), length_x=columns(x); else, length_x=rows(x); endif
      limit=max(3*(numel(obj.Numerator)-1),1);
      if (length_x<=limit)
        error ('signal:filtfilt:InvalidDimensionsDataShortForFiltOrder', ...
               'digitalFilter.filtfilt: data length must be greater than %d',limit);
      endif
      y=filtfilt(obj.Numerator,1,x);
    endfunction
    function varargout = freqz (obj, varargin)
      if (numel(obj)!=1), error ('digitalFilter.freqz: filter arrays are unsupported'); endif
      if (numel(varargin)!=1 || !isnumeric(varargin{1}) || !isreal(varargin{1}) || ...
          !isscalar(varargin{1}) || !isfinite(varargin{1}) || varargin{1}<1 || ...
          varargin{1}!=fix(varargin{1}) || nargout==0)
        error ('digitalFilter.freqz: supported form is [H,W] = freqz(D,N) with a positive integer N');
      endif
      [varargout{1:nargout}]=freqz(obj.Numerator,1,varargin{:});
    endfunction
    function n = filtord (obj)
      if (numel(obj)!=1), error ('digitalFilter.filtord: filter arrays are unsupported'); endif
      n=numel(obj.Numerator)-1;
    endfunction
    function [b, a] = tf (obj)
      if (numel(obj)!=1), error ('digitalFilter.tf: filter arrays are unsupported'); endif
      b=obj.Numerator; a=obj.Denominator;
    endfunction
    function varargout = subsref (obj, index)
      if (numel(obj)!=1), error ('digitalFilter: filter arrays are unsupported'); endif
      if (!isempty(index) && strcmp(index(1).type,'.') && ...
          !any(strcmp(index(1).subs,[properties(obj); methods(obj)])))
        error ('MATLAB:noSuchMethodOrField', ...
               'digitalFilter: unrecognized method or property %s',index(1).subs);
      endif
      [varargout{1:nargout}]=builtin('subsref',obj,index);
    endfunction
  endmethods
endclassdef
