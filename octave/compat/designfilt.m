function d = designfilt (response, varargin)
% DESIGNFILT Minimum-order real lowpass FIR equiripple subset.
% Supports PassbandFrequency, StopbandFrequency, PassbandRipple (dB),
% StopbandAttenuation (dB), optional SampleRate and DesignMethod='equiripple'.
% Requires the installed signal package. Orders outside 2..32, fixed-order and IIR
% designs, other responses/options and nonfinite specifications are rejected.
% Solver non-convergence is reported explicitly.
% Returns the FIR digitalFilter subset, with numeric filter/filtfilt/freqz/tf
% methods; not a struct or a replacement for the full MATLAB filter toolbox.
  if (nargin<1 || !ischar(response) || !strcmpi(response,'lowpassfir'))
    error ('designfilt: only minimum-order lowpassfir equiripple designs are supported');
  endif
  specs=struct('PassbandFrequency',[], 'StopbandFrequency',[], ...
               'PassbandRipple',[], 'StopbandAttenuation',[], 'SampleRate',2);
  names=fieldnames(specs);
  if (mod(numel(varargin),2)), error ('designfilt: options must be name/value pairs'); endif
  for i=1:2:numel(varargin)
    name=varargin{i}; value=varargin{i+1};
    if (!ischar(name)), error ('designfilt: option names must be character vectors'); endif
    if (strcmpi(name,'DesignMethod'))
      if (!ischar(value) || !strcmpi(value,'equiripple'))
        error ('designfilt: only the equiripple design method is supported');
      endif
      continue;
    endif
    index=find(strcmpi(name,names),1);
    if (isempty(index)), error ('designfilt: unsupported option %s',name); endif
    if (!isnumeric(value) || !isreal(value) || !isscalar(value) || !isfinite(value) || value<=0)
      error ('designfilt: specifications must be positive finite real numeric scalars');
    endif
    specs.(names{index})=double(value);
  endfor
  for i=1:4
    if (isempty(specs.(names{i}))), error ('designfilt: %s is required',names{i}); endif
  endfor
  fp=2*specs.PassbandFrequency/specs.SampleRate;
  fs=2*specs.StopbandFrequency/specs.SampleRate;
  if (!(fp<fs && fs<1)), error ('designfilt: frequencies must satisfy 0 < Fpass < Fstop < SampleRate/2'); endif
  ripple=(10^(specs.PassbandRipple/20)-1)/(10^(specs.PassbandRipple/20)+1);
  stop=10^(-specs.StopbandAttenuation/20);
  if (!(ripple>0 && ripple<1 && stop>0))
    error ('designfilt: specifications are outside the supported floating-point range');
  endif
  if (exist('remez')==0), error ('designfilt: the signal package must be loaded'); endif
  if (1-ripple<=stop || (1-ripple)/(2*cos(fp*pi/2))<=min((1+ripple)/2,stop/(2*cos(fs*pi/2))))
    error ('designfilt: designs of order below 2 are unsupported');
  endif
  % Search both Type-I and Type-II designs. Test the actual extrema, including
  % band endpoints, rather than accepting a frequency-grid approximation.
  for order=2:32
    try
      if (order==2), b=three_taps(fp,fs,ripple,stop);
      else, b=remez(order,[0 fp fs 1],[1 1 0 0],[1 ripple/stop]); b=b(:)'; endif
    catch
      error ('designfilt: equiripple solver failed for these specifications');
    end_try_catch
    [pass_error,stop_error]=band_errors(b,fp,fs);
    if (pass_error<=ripple && stop_error<=stop)
      d=digitalFilter(b,specs); return;
    endif
  endfor
  error ('designfilt: designs requiring order above 32 are unsupported');
endfunction

function b = three_taps (fp, fs, ripple, stop)
  % The signal solver requires at least four taps. A three-tap symmetric FIR
  % has zero-phase response c+2*a*cos(w), monotone in each band. Its minimax
  % problem therefore has only these four endpoint constraints.
  x=[1; cos(pi*fp); cos(pi*fs); -1]; target=[1;1;0;0];
  delta=[ripple;ripple;stop;stop];
  constraints=[2*x ones(4,1) -delta; -2*x -ones(4,1) -delta];
  bounds=[target;-target]; triples=nchoosek(1:8,3); best=Inf; b=[];
  for k=1:rows(triples)
    ids=triples(k,:); matrix=constraints(ids,:);
    if (rcond(matrix)<eps), continue; endif
    candidate=matrix\bounds(ids);
    if (candidate(3)<best && all(constraints*candidate<=bounds+1e-12))
      best=candidate(3); b=[candidate(1) candidate(2) candidate(1)];
    endif
  endfor
  if (isempty(b)), error ('designfilt: three-tap minimax solver failed'); endif
endfunction

function [pass_error, stop_error] = band_errors (b, fp, fs)
  % |H|^2 is a polynomial in cos(w), using its autocorrelation coefficients.
  % Its derivative roots give every possible interior amplitude extremum.
  order=numel(b)-1; correlation=conv(b,fliplr(b));
  coefficients=[correlation(order+1) 2*correlation(order+2:end)];
  derivative=zeros(1,order); derivative(end)=2*order*coefficients(end);
  if (order>1), derivative(end-1)=2*(order-1)*coefficients(end-1); endif
  for k=order-3:-1:0
    derivative(k+1)=derivative(k+3)+2*(k+1)*coefficients(k+2);
  endfor
  derivative(1)/=2;
  % A Chebyshev colleague matrix avoids the ill-conditioned conversion to
  % monomial coefficients for longer filters.
  while (numel(derivative)>1 && derivative(end)==0), derivative(end)=[]; endwhile
  degree=numel(derivative)-1;
  if (degree==0), critical=[];
  elseif (degree==1), critical=-derivative(1)/derivative(2);
  else
    colleague=zeros(degree); colleague(2,1)=1;
    for k=2:degree-1, colleague(k-1,k)=0.5; colleague(k+1,k)=0.5; endfor
    colleague(:,end)=-0.5*derivative(1:end-1)'/derivative(end);
    colleague(end-1,end)+=0.5; critical=eig(colleague);
  endif
  critical=real(critical(abs(imag(critical))<1e-8 & real(critical)>-1 & real(critical)<1));
  w=acos(critical); wp=[0; fp*pi; w(w<fp*pi)]; ws=[fs*pi; pi; w(w>fs*pi)];
  hp=abs(exp(-1i*wp*(0:order))*b(:)); hs=abs(exp(-1i*ws*(0:order))*b(:));
  pass_error=max(abs(hp-1)); stop_error=max(hs);
endfunction
