function args=__mf_export_defaults__()
% Supported per-figure callback defaults in Octave 11.3.
% defaultlightcreatefcn/defaultlightdeletefcn are rejected despite appearing in
% factory properties. Lights receive explicit empty callbacks in local_clone.
  args={};
  for kind={'axes','line','text','patch','surface','image','hggroup'}
    args=[args {['default' kind{1} 'createfcn'],[],['default' kind{1} 'deletefcn'],[]}];
  endfor
endfunction
