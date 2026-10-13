% Dizgi: dalga 26 doğrulama alt kümeleri
%% wave26-strncmpi-positive | strncmpi | Skaler karşılaştırma uzunluğu
assert(isequal(strncmpi({'Abc','XY'},{'aBd','xy'},2),[true true]));
%% wave26-strncmpi-length-reject | strncmpi | Kaydedilmiş MATLAB ys-strncmpi-cells hata referansı
failed=false; try, strncmpi({'Abc','XY'},{'aBd','xy'},[2 1]); catch, failed=true; end; assert(failed);
%% wave26-strfind-positive | strfind | Hücre kaynakları ve örtüşen eşleşmeler
assert(isequal(strfind({'banana','ana'},'ana'),{[2 4],1}));
%% wave26-strfind-pattern-reject | strfind | Kaydedilmiş MATLAB ys-strfind-pattern-cell hata referansı
failed=false; try, strfind('abc',{'a','bc','z'}); catch, failed=true; end; assert(failed);
%% wave26-strrep-positive | strrep | Hücre kaynaklarında değiştirme
assert(isequal(strrep({'aba','bab'},'a','XY'),{'XYbXY','bXYb'}));
%% wave26-strrep-multirow-reject | strrep | Kaydedilmiş MATLAB ys-strrep-char-matrix hata referansı
failed=false; try, strrep(char('aba','bab'),'a','XY'); catch, failed=true; end; assert(failed);
%% wave26-structfun-positive | structfun | Skaler yapı alanlarına işlev uygulama
assert(isequal(structfun(@(x)x+1,struct('a',1,'b',2)),[2;3]));
%% wave26-structfun-empty-array-reject | structfun | Kaydedilmiş MATLAB yd-struct-structfun-empty hata referansı
failed=false; try, structfun(@isempty,struct('a',{})); catch, failed=true; end; assert(failed);
