## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows LIMIT01–LIMIT25; project subset contract (not MATLAB error parity).
function __indymat_zpk_scope__ (value)
  if (isa(value,'lti'))
    if (! isequal(size(value),[1 1])), __indymat_zpk_scope__('mimo'); endif
    if (get(value,'tsam') != 0), __indymat_zpk_scope__('discrete'); endif
    return;
  elseif (isnumeric(value))
    if (! isscalar(value)), __indymat_zpk_scope__('mimo'); endif
    return;
  endif
  switch (value)
    case 'mimo', suffix='UnsupportedMIMO'; message='This representation supports SISO models only.';
    case 'discrete', suffix='UnsupportedDiscreteTime'; message='This representation supports continuous-time models with Ts equal to zero only.';
    case 'delay', suffix='UnsupportedDelay'; message='This representation does not support time delays.';
    case 'array', suffix='UnsupportedModelArray'; message='This representation does not support arrays of models.';
    case 'property', suffix='UnsupportedProperty'; message='This property is outside the documented control representation subset.';
    otherwise, suffix='UnsupportedCallForm'; message='This call form is outside the documented control representation subset.';
  endswitch
  error (['IndyMAT:control:' suffix], message);
endfunction
