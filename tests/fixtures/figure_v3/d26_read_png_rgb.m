function rgb = d26_read_png_rgb(path)
  % Native PNGs may be RGB, grayscale, bilevel or indexed. Return the same
  % 8-bit RGB scale for every oracle row; do not infer channels from the scene.
  [pixels,map]=imread(path);
  if ~isempty(map)
    rgb=ind2rgb(pixels,map); % Handles integer zero-based vs double one-based indices.
  else
    if islogical(pixels),rgb=double(pixels);
    elseif isinteger(pixels),rgb=double(pixels)/double(intmax(class(pixels)));
    else,rgb=double(pixels);endif
    if ndims(rgb)==2,rgb=repmat(rgb,[1 1 3]);endif
  endif
  assert(ndims(rgb)==3&&size(rgb,3)==3,'Expected RGB, grayscale or indexed PNG');
  rgb=uint8(round(255*max(0,min(1,rgb))));
endfunction
