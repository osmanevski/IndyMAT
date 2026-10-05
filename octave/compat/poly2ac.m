function r = poly2ac (a, efinal)
% POLY2AC Recover autocorrelation samples from an AR polynomial.
% Supports first-order finite real monic polynomials and positive final error.
% Uses the Yule-Walker system; returned samples are a column vector.
  [r, unused_U, unused_k] = rlevinson (a, efinal);
endfunction
