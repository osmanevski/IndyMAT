function [z, p, g] = __mf_ellip_prototype__ (n, rp, rs)
% __MF_ELLIP_PROTOTYPE__ Jacobi elliptic analog lowpass prototype.
% Used by the signal patch for positive, finite ripple specifications whose
% elliptic degree equation is bracketed in double precision. Degenerate
% specifications retain the package implementation.
  ep2 = expm1 (log (10) * rp / 10);
  m1 = ep2 / expm1 (log (10) * rs / 10);
  kk = ellipke ([m1; 1-m1]);
  target = n * kk(1) / kk(2);
  m = fzero (@(v) ellipke(v) / ellipke(1-v) - target, [eps, 1-eps]);
  K = ellipke (m);
  [s, c, d] = ellipj ((1-rem(n,2):2:n-1) * K/n, m);
  nonzero = abs (s) > 10*eps;
  z = 1i ./ (sqrt(m) * s(nonzero));
  z = [z, conj(z)];
  % Invert sc(v,1-m1) by its defining incomplete elliptic integral.
  r = quadgk (@(v) 1 ./ sqrt (1-(1-m1)*sin(v).^2), ...
              0, atan (1/sqrt(ep2)), 'AbsTol', 1e-13, 'RelTol', 1e-13);
  v0 = K*r / (n*kk(1));
  [sv, cv, dv] = ellipj (v0, 1-m);
  p = -(c.*d*sv*cv - 1i*s*dv) ./ (1-(d*sv).^2);
  p = [p, conj(p(nonzero))];
  g = real (prod(-p) / prod(-z));
  if (rem (n,2) == 0), g /= sqrt (1+ep2); endif
endfunction
