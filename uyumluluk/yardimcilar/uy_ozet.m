function s = uy_ozet(x, derinlik)
% UY_OZET Canonical one-line summary of a value, identical code for MATLAB and Octave.
% class|size|content, so two engines can be compared text against text (scripts/fark.py).
  if nargin < 2, derinlik = 0; end
  boyut = sprintf('%dx', size(x)); boyut = boyut(1:end-1);
  sinif = class(x);
  if derinlik > 3, s = [sinif '|' boyut '|...']; return; end
  if isa(x, 'function_handle')
    icerik = regexprep(func2str(x), '\s+', '');
  elseif ischar(x)
    icerik = ['u' sprintf('%d,', double(x(:)'))];
  elseif isa(x, 'string')
    parca = cell(1, numel(x));
    for k = 1:numel(x)
      if ismissing(x(k)), parca{k} = '<missing>'; else, parca{k} = uy_ozet(char(x(k)), derinlik + 1); end
    end
    icerik = uy_birlestir(parca);
  elseif islogical(x)
    icerik = sprintf('%d', x(:)');
  elseif isnumeric(x)
    v = double(x(:)');
    if numel(v) > 60, v = [v(1:60)]; ek = '...'; else, ek = ''; end
    if isreal(x), icerik = [sprintf('%.12g,', v) ek];
    else
      parca = cell(1, numel(v));
      for k = 1:numel(v)
        if imag(v(k)) == 0
          parca{k} = sprintf('%.12g+0i,', real(v(k)));
        else
          parca{k} = sprintf('%.12g%+.12gi,', real(v(k)), imag(v(k)));
        end
      end
      icerik = [parca{:} ek];
    end
  elseif iscell(x)
    parca = cell(1, min(numel(x), 40));
    for k = 1:numel(parca), parca{k} = uy_ozet(x{k}, derinlik + 1); end
    icerik = uy_birlestir(parca);
  elseif isstruct(x)
    adlar = fieldnames(x); parca = cell(1, numel(adlar));
    for k = 1:numel(adlar)
      if numel(x) == 1, parca{k} = [adlar{k} '=' uy_ozet(x.(adlar{k}), derinlik + 1)];
      else, parca{k} = adlar{k}; end
    end
    icerik = uy_birlestir(parca);
  else
    icerik = '<nesne>';
  end
  s = [sinif '|' boyut '|' icerik];
end

function s = uy_birlestir(parca)
  s = '{';
  for k = 1:numel(parca), s = [s parca{k} ';']; end
  s = [s '}'];
end
