% Sinyal işleme: dalga 26 paket doğrulaması
%% wave26-interp-default-short | interp | Kaydedilmiş MATLAB ys-interp-values hata referansı; paket yaması uygulanmalı
failed=false; try, interp([1 2 3],2); catch, failed=true; end; assert(failed);
%% wave26-interp-default-shape | interp | Belgelerden türetilmiş çıktı boyutu; sayısal MATLAB eşliği iddiası yok
assert(isequal(size(interp(1:9,2)),[1 18]));
assert(isequal(size(interp((1:9)',3)),[27 1]));
%% wave26-interp-explicit-minimum | interp | R2025b round2 interp satırları 1,6,7: oran bir ve açık n4
for args={{1:3,1},{1:8,2,4},{1:8,2,4,.5}}
  failed=false; try, interp(args{1}{:}); catch e, failed=strcmp(e.identifier,'signal:interp:InvalidDimensions'); end
  assert(failed);
end
%% wave26-interp-short-filter-shape | interp | R2025b round2 interp satırları 5,8: açık n1 üç örneği kabul eder
assert(isequal(size(interp(1:3,2,1)),[1 6]));
assert(isequal(size(interp(1:3,2,1,1)),[1 6]));
%% wave26-interp-single-class | interp | R2025b round2 interp satır 10: varsayılan oran iki single korunur
y=interp(single(1:9),2); assert(isa(y,'single') && isequal(size(y),[1 18]));
