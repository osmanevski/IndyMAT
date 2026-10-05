% Matematik ve doğrusal cebir
%% maty-default | round | Varsayılan yarımlar sıfırdan uzağa
assert(isequal(round([-2.5 -1.5 0 1.5 2.5]), [-3 -2 0 2 3]));
%% maty-decimals | round | Pozitif ondalık basamaklar
assert(max(abs(round([2.567 -2.567], 2) - [2.57 -2.57])) < 1e-12);
%% maty-negative | round | Negatif ve sıfır basamaklar
assert(isequal(round([1234 -1234], -2), [1200 -1200]));
assert(isequal(round([2.5 -2.5], 0), [3 -3]));
%% maty-mode | round | Açık decimals seçeneği
assert(isequal(round([1.125 -1.125], 2, 'decimals'), [1.13 -1.13]));
%% maty-significant | round | Anlamlı basamaklar
assert(isequal(round([1234 -1234 0], 2, 'significant'), [1200 -1200 0]));
assert(abs(round(0.01234, 2, 'significant') - 0.012) < 1e-15);
%% maty-single | round | Single sınıfı korunur
y = round(single([2.567 -2.567]), 2);
assert(isa(y, 'single') && isequal(y, single([2.57 -2.57])));
%% maty-complex | round | Karmaşık bileşenler ayrı yuvarlanır
y = round(complex(1.234, -5.678), 2);
assert(abs(y - complex(1.23, -5.68)) < 1e-12);
%% maty-special | round | Boş dizi ve sonlu olmayan değerler
assert(isequal(size(round(zeros(0,3), 2)), [0 3]));
y = round([Inf -Inf NaN 0], 2);
assert(isinf(y(1)) && isinf(y(2)) && isnan(y(3)) && y(4) == 0);
%% maty-integer | round | Tamsayı tek girdide korunur, basamak girdisi reddedilir
y = round(int32([1234 -1234]));
assert(isa(y, 'int32') && isequal(y, int32([1234 -1234])));
failed = false;
try, round(int32(1234), -2); catch, failed = true; end
assert(failed);
%% maty-validation | round | Geçersiz basamak ve mod reddedilir
failed = false;
try, round(1, 1.5); catch, failed = true; end
assert(failed);
failed = false;
try, round(1, 0, 'significant'); catch, failed = true; end
assert(failed);
