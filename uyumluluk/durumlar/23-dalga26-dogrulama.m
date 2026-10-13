% Matematik: dalga 26 doğrulama alt kümeleri
%% wave26-cummax-positive | cummax | Pozitif değerler özgün fonksiyona devredilir
assert(isequal(cummax([2 1 3 0]),[2 2 3 3]));
%% wave26-cummax-output-reject | cummax | Kaydedilmiş MATLAB ym-cummax-outs hata referansı
failed=false; try, [a,b]=cummax([2 1 3 0]); catch, failed=true; end; assert(failed);
%% wave26-cummin-positive | cummin | Pozitif değerler özgün fonksiyona devredilir
assert(isequal(cummin([2 1 3 0]),[2 1 1 0]));
%% wave26-cummin-output-reject | cummin | Kaydedilmiş MATLAB ym-cummin-outs hata referansı
failed=false; try, [a,b]=cummin([2 1 3 0]); catch, failed=true; end; assert(failed);
%% wave26-cumsum-positive | cumsum | Kümülatif toplam özgün fonksiyona devredilir
assert(isequal(cumsum([1 2;3 4]),[1 2;4 6]));
%% wave26-cumsum-all-reject | cumsum | Kaydedilmiş MATLAB ym-cumsum-all hata referansı
failed=false; try, cumsum([1 2;3 4],'all'); catch, failed=true; end; assert(failed);
%% wave26-permute-positive | permute | Yoğun dizi özgün fonksiyona devredilir
assert(isequal(permute([1 2;3 4],[2 1]),[1 3;2 4]));
%% wave26-permute-sparse-positive | permute | Gerçek MATLAB R2025b round2 A/B/E ölçümü; sparse korunur
x=sparse([1 0;0 2]); y=permute(x,[2 1]); assert(issparse(y) && isequal(y,x));
assert(issparse(permute(x,[1 2])));
y=permute(sparse([1 0 3]),[2 1]); assert(issparse(y) && isequal(full(y),[1;0;3]));
%% wave26-bitcmp-positive | bitcmp | Tamsayı tümleme özgün fonksiyona devredilir
assert(isequal(bitcmp(uint16(1)),uint16(65534)));
%% wave26-bitcmp-width-reject | bitcmp | Kaydedilmiş MATLAB yd-int-bitcmp-width hata referansı
failed=false; try, bitcmp(uint16(1),8); catch, failed=true; end; assert(failed);
%% wave26-hypot-nan-inf | hypot | Kaydedilmiş MATLAB ym-hypot-inf-nan değer referansı
assert(all(isnan(hypot([Inf NaN],[NaN Inf]))));
%% wave26-hypot-positive | hypot | Analitik 3-4-5 üçgeni ve single türü
assert(isequal(hypot([3 5],[4 12]),[5 13]));
assert(isa(hypot(single(3),single(4)),'single'));
%% wave26-trace-positive | trace | Analitik köşegen toplamı
assert(trace([1 2;3 4])==5);
%% wave26-trace-square-reject | trace | Kaydedilmiş MATLAB ym-trace-rectangular hata referansı
failed=false; try, trace([1 2 3;4 5 6]); catch, failed=true; end; assert(failed);
%% wave26-gcd-positive | gcd | Analitik ortak bölen ve Bezout katsayıları
assert(isequal(gcd([12 18 21],[8 24 14]),[4 6 7]));
[g,a,b]=gcd([12 18],[8 24]); assert(isequal(a.*[12 18]+b.*[8 24],g));
%% wave26-gcd-size-reject | gcd | Kaydedilmiş MATLAB ym-gcd-array-expansion hata referansı
failed=false; try, gcd([12;18],[8 24]); catch, failed=true; end; assert(failed);
%% wave26-dec2base-positive | dec2base | Analitik taban dönüşümü
assert(isequal(dec2base([0 1 10 35],36),char('0','1','A','Z')));
%% wave26-dec2base-negative-reject | dec2base | Kaydedilmiş MATLAB ym-dec2base-negative hata referansı
failed=false; try, dec2base([-1 -10],16); catch, failed=true; end; assert(failed);
%% wave26-spline-positive | spline | Veri düğümleri ve geçerli uç türevleri
assert(isequal(spline([0 1 2],[0 1 0],[0 1 2]),[0 1 0]));
assert(max(abs(spline([0 1 2],[0 0 1 4 4],[0 .5 2])-[0 .25 4]))<1e-12);
%% wave26-spline-count-reject | spline | Kaydedilmiş MATLAB ym-spline-endconds hata referansı
failed=false; try, spline([0 1 2],[1 0 1 0],[0 .5 2]); catch, failed=true; end; assert(failed);
%% wave26-dec2bin-hex-negative | dec2bin dec2hex | Déc2base koruması yerel ikiye tümleyen iç çağrısını bozmaz
assert(isequal(dec2bin([-1 -10]),char('11111111','11110110')));
assert(isequal(dec2hex([-1 -10]),char('FF','F6')));
