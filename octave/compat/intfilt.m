function b = intfilt (l, p, alpha)
% INTFILT Bandlimited least-squares or odd-order Lagrange interpolation FIR.
% Supports double scalar parameters: integer L>=2, integer P>=1, 0<ALPHA<=1.
% Its length is 2*L*P-1 and its gain is L. ALPHA sets the original bandwidth.
% 'Lagrange' supports odd positive polynomial orders (length (P+1)*L-1).
% Even-order Lagrange phase alignment, ALPHA=0 and other forms are unsupported.
% Ill-conditioned bandlimited least-squares systems (RCOND<1e-12) error.
  if (nargin != 3), error ('intfilt: expected L, P, ALPHA or L, N, Lagrange'); endif
  if (! isa (l, 'double') || ! isreal (l) || ! isscalar (l) || ! isfinite (l) || l < 2 || fix (l) != l)
    error ('intfilt: L must be a finite integer double scalar at least two');
  endif
  if (! isa (p, 'double') || ! isreal (p) || ! isscalar (p) || ! isfinite (p) || p < 1 || fix (p) != p)
    error ('intfilt: P or N must be a positive finite integer double scalar');
  endif
  l = double (l); p = double (p);
  if (ischar (alpha) && strcmpi (alpha, 'Lagrange'))
    if (mod (p, 2) == 0), error ('intfilt: even-order Lagrange phase alignment is unsupported'); endif
    if (p > 31 || (p+1)*l > 1e6), error ('intfilt: Lagrange order or length exceeds the supported allocation limit'); endif
    half = (p+1)/2;
    t = (-(half*l-1):(half*l-1))/l;
    b = zeros (size (t));
    % Each interval uses N+1 surrounding original samples. Evaluate the
    % coefficient of the sample at zero in that interval's Lagrange basis.
    for k = 1:numel (t)
      knots = (floor (t(k))-half+1):(floor (t(k))+half);
      others = knots(knots != 0);
      b(k) = prod ((t(k)-others) ./ (-others));
    endfor
  else
    if (! isa (alpha, 'double') || ! isreal (alpha) || ! isscalar (alpha) || ! isfinite (alpha) || alpha <= 0 || alpha > 1)
      error ('intfilt: ALPHA must be a finite double scalar in (0,1], or Lagrange');
    endif
    if (l*p > 2048), error ('intfilt: least-squares order exceeds the supported allocation limit'); endif
    % Each polyphase branch interpolates from 2*P original samples. Very
    % narrow occupied bands make these samples numerically indistinguishable.
    correlation = sinc (double(alpha) * ((0:2*p-1)' - (0:2*p-1)));
    if (rcond (correlation) < 1e-12)
      error ('intfilt: bandlimited least-squares system is too ill-conditioned for this subset');
    endif
    % In the high-rate Nyquist scale the signal occupies ALPHA/L and its
    % spectral images are centered at 2*K/L. Only these image bands belong
    % in the objective: constraining the gaps between them would change the
    % interpolator and corrupt the original samples when L is greater than 2.
    edges = [0 double(alpha)/l]; desired = [1 1];
    for center = 2:2:l
      edges = [edges (center-double(alpha))/l min(1,(center+double(alpha))/l)];
      desired = [desired 0 0];
    endfor
    b = l * firls (2*l*p-2, edges, desired);
  endif
endfunction
