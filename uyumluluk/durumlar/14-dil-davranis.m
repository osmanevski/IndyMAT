% Dil ve programlama
%% dav-int-saturate-plus | int8 | Tamsayı toplaması sınıra doyar
x = int8(120) + int8(20);
assert(isa(x, 'int8') && x == 127);
%% dav-int-saturate-multiply | int8 | Tamsayı çarpımı sınıra doyar
x = int8(30) .* int8(10);
assert(isa(x, 'int8') && x == 127);
%% dav-int-division-round | int8 | Tamsayı bölmesi en yakına yuvarlar
x = int8(5) ./ int8(2);
assert(isa(x, 'int8') && x == 3);
%% dav-int-two-classes-error | :integer-mixed-class | Farklı tamsayı sınıfları birlikte kullanılamaz
hata = false;
try, x = int8(1) + int16(2); catch, hata = true; end
assert(hata);
%% dav-idivide-round | idivide | En yakın tamsayıya bölme seçeneği
x = idivide(int8(5), int8(2), 'round');
assert(x == 3);
%% dav-idivide-floor | idivide | floor seçeneği negatif bölmede aşağı yuvarlar
x = idivide(int8(-5), int8(2), 'floor');
assert(x == -3);
%% dav-rem-negative | rem | rem işareti bölüneni izler
assert(rem(-5, 3) == -2);
%% dav-mod-negative | mod | mod işareti böleni izler
assert(mod(-5, 3) == 1);
%% dav-rem-zero | rem | Sıfıra kalan IEEE NaN üretir
assert(isnan(rem(1, 0)));
%% dav-bitand-integer | bitand | Aynı sınıf tamsayılar bit düzeyinde kesişir
x = bitand(uint8(12), uint8(10));
assert(isa(x, 'uint8') && x == 8);
%% dav-bitshift-negative | bitshift | Negatif kaydırma sağa kaydırır
assert(bitshift(uint8(8), -2) == 2);
%% dav-intmax-overflow | intmax | uint8 artırması doygun kalır
x = uint8(intmax('uint8')) + uint8(1);
assert(x == intmax('uint8'));
%% dav-single-plus-double | :single-double-mix | Karışık toplam tek duyarlıklı sınıfı korur
x = single(1) + 0.5;
assert(isa(x, 'single') && x == single(1.5));
%% dav-sum-empty | sum | Boş dizinin toplamı sıfırdır
assert(sum([]) == 0);
%% dav-prod-empty | prod | Boş dizinin çarpımı birdir
assert(prod([]) == 1);
%% dav-max-empty | max | Boş dizinin maksimumu boş kalır
assert(isempty(max([])));
%% dav-mean-empty | mean | Boş dizinin ortalaması NaN olur
assert(isnan(mean([])));
%% dav-sum-empty-shape | sum | Sıfır satırlı matris boyut korur
x = sum(zeros(0, 3));
assert(isequal(size(x), [1 3]) && isequal(x, [0 0 0]));
%% dav-max-omitnan-all | max | all ve omitnan tüm elemanları tek skalerde indirger
x = max([1 NaN; 3 2], [], 'all', 'omitnan');
assert(x == 3);
%% dav-min-includenan | min | includenan herhangi NaN değerini korur
x = min([2 NaN 1], [], 'includenan');
assert(isnan(x));
%% dav-max-placeholder-dim | max | Boş yer tutucuyla ikinci boyut seçilir
x = max([1 7; 9 3], [], 2);
assert(isequal(x, [7; 9]));
%% dav-ismember-rows | ismember | Satır üyeliği eşleşme indisleri verir
[tf, loc] = ismember([1 2; 2 3], [2 3; 1 2], 'rows');
assert(isequal(tf, [true; true]) && isequal(loc, [2; 1]));
%% dav-cumsum-reverse | cumsum | reverse yönü sütun birikimini ters çevirir
x = cumsum([1 2 3], 'reverse');
assert(isequal(x, [6 5 3]));
%% dav-any-empty | any | Boş mantıksal dizide herhangi sonucu false
assert(~any([]));
%% dav-all-empty | all | Boş mantıksal dizide tüm sonucu true
assert(all([]));
%% dav-linspace-singleton | linspace | Tek örnek uç başlangıç değeridir
x = linspace(4, 9, 1);
assert(isequal(x, 9));
%% dav-colon-empty | :colon-range | Azalan aralık yanlış işaretli adımla boştur
assert(isempty(5:1:2));
%% dav-colon-endpoint | :colon-range | Ondalık adım uç değeri yuvarlama toleransıyla korunur
x = 0:0.3:1;
assert(numel(x) == 4 && abs(x(end) - 0.9) < eps);
%% dav-num2str-default | num2str | Varsayılan gösterim dört ondalık basamak kullanır
assert(strcmp(num2str(pi), '3.1416'));
%% dav-mat2str-precision | mat2str | Açık hassasiyet basamak sayısını uygular
assert(strcmp(mat2str(pi, 3), '3.14'));
%% dav-ndims-trailing-one | ndims | Sonraki tekil boyut ndims değerine katılmaz
x = zeros(2, 3, 1);
assert(ndims(x) == 2 && isequal(size(x), [2 3]));
%% dav-size-multiple-outputs | size | Fazla boyut çıktısı son boyutları toplar
[a, b, c] = size(ones(2, 3, 4, 5));
assert(a == 2 && b == 3 && c == 20);
%% dav-logical-index-row | :logical-index-shape | Satır mantıksal indisi satır yönünü korur
x = [4 5 6]; y = x([true false true]);
assert(isequal(size(y), [1 2]));
%% dav-logical-index-column | :logical-index-shape | Sütun mantıksal indisi sütun yönünü korur
x = [4; 5; 6]; y = x([true false true]);
assert(isequal(size(y), [2 1]));
%% dav-linear-index-matrix | reshape | Matris doğrusal indeksle sütun sırası izler
x = reshape(1:6, 2, 3); y = x(:);
assert(isequal(y, (1:6)'));
%% dav-cat-empty | horzcat | Boş sayısal dizi yatay birleştirmede etkisizdir
x = horzcat([], [1 2]);
assert(isequal(x, [1 2]));
%% dav-cell2mat-row | cell2mat | Uyumlu skaler hücreler satır olur
x = cell2mat({1, 2, 3});
assert(isequal(size(x), [1 3]));
%% dav-num2cell-shape | num2cell | Hücre dönüşümü kaynak dizinin boyutunu korur
x = num2cell(reshape(1:6, 2, 3));
assert(isequal(size(x), [2 3]) && isequal(x{2,3}, 6));
%% dav-struct-cell-values | struct | Hücre alanları yapı dizisi üretir
s = struct('v', {1, 2, 3});
assert(isequal(size(s), [1 3]) && s(2).v == 2);
%% dav-struct-field-order | fieldnames | Alan adları eklenme sırasını izler
s = struct('z', 1, 'a', 2, 'm', 3);
assert(isequal(fieldnames(s), {'z'; 'a'; 'm'}));
%% dav-isequaln-nan | isequal isequaln | isequaln NaN konumlarını eşit sayar
assert(~isequal(NaN, NaN) && isequaln(NaN, NaN));
%% dav-isempty-empty-struct | isempty | Alansız skaler struct boş değildir
s = struct();
assert(~isempty(s) && numel(s) == 1);
%% dav-char-class-empty-colon | :colon-range | Boş kolon ifadesi double sınıfındadır
x = 1:0;
assert(isempty(x) && isa(x, 'double'));
%% dav-strsplit-trailing | strsplit | Varsayılan ayırma sondaki boş parçayı atar
assert(isequal(strsplit('a,b,', ','), {'a', 'b', ''}));
%% dav-strjoin-cell | strjoin | Hücre dizileri ayırıcıyla birleştirilir
assert(strcmp(strjoin({'a', 'b', 'c'}, '-'), 'a-b-c'));
%% dav-regexp-tokens | regexp | tokens eşleşen yakalama gruplarını döndürür
x = regexp('ab12cd', '([a-z]+)(\d+)', 'tokens');
assert(isequal(x, {{'ab', '12'}}));
%% dav-regexp-names | regexp | names adlı grup alan adıyla struct verir
x = regexp('id=42', '(?<key>[a-z]+)=(?<value>\d+)', 'names');
assert(strcmp(x.key, 'id') && strcmp(x.value, '42'));
%% dav-regexp-once | regexp | once yalnız ilk eşleşmenin konumunu döndürür
x = regexp('a1b2', '\d', 'once');
assert(x == 2);
%% dav-regexprep-dollar | regexprep | $1 değiştirme metninde grubu yerleştirir
x = regexprep('ab12', '([a-z]+)(\d+)', '$2:$1');
assert(strcmp(x, '12:ab'));
%% dav-strtrim-cell | strtrim | Cell dizisinin metinleri ayrı kırpılır
x = strtrim({' a ', 'b  '});
assert(isequal(x, {'a', 'b'}));
%% dav-strcmp-char-numeric | strcmp | char ve sayısal girdi eşit metin sayılmaz
assert(~strcmp('1', 1));
%% dav-strfind-cell | strfind | Hücre metinlerinde her eşleşme konumu ayrı listelenir
x = strfind({'aba', 'bbb'}, 'b');
assert(isequal(x, {[2], [1 2 3]}));
%% dav-str2double-whitespace | str2double | Baş ve sondaki boşluklu sayı çevrilir
assert(str2double('  12.5 ') == 12.5);
%% dav-str2double-invalid | str2double | Geçersiz metin NaN olur
assert(isnan(str2double('12x')));
%% dav-str2num-expression | str2num | Sayısal ifade hesaplanıp matrise çevrilir
x = str2num('[1 2; 3 4]');
assert(isequal(x, [1 2; 3 4]));
%% dav-dec2bin-width | dec2bin | İstenen genişlik soldan sıfırla doldurur
assert(strcmp(dec2bin(5, 5), '00101'));
%% dav-dec2hex-width | dec2hex | İstenen genişlik onaltılık sıfır dolgusu yapar
assert(strcmp(dec2hex(10, 4), '000A'));
%% dav-fliplr-char | fliplr | Karakter satırını ters yönde çevirir
assert(strcmp(fliplr('abcd'), 'dcba'));
%% dav-double-char-ascii | double | ASCII char kodu sayısal koda dönüşür
assert(isequal(double('abc'), [97 98 99]));
%% dav-length-char-ascii | length | ASCII karakter sayısı length değeridir
assert(length('abc') == 3);
%% dav-cellfun-uniform-error | cellfun | UniformOutput varsayılanı tekil sayısal sonuç ister
hata = false;
try, x = cellfun(@(n) [n n], {1, 2}); catch, hata = true; end
assert(hata);
%% dav-arrayfun-cell-output | arrayfun | UniformOutput false hücre dizisi üretir
x = arrayfun(@(n) 1:n, 1:3, 'UniformOutput', false);
assert(isequal(x, {[1], [1 2], [1 2 3]}));
%% dav-structfun-output | structfun | Skaler alan sonuçları yapı alan sırasını izler
s = struct('a', 2, 'b', 3);
x = structfun(@(v) v^2, s);
assert(isequal(x, [4; 9]));
%% dav-map-order-control | containers.Map keys values | Map preserves insertion order in keys and values
m = containers.Map(); m('b') = 2; m('a') = 1;
k = keys(m); v = values(m);
assert(m.Count == 2 && numel(k) == 2 && numel(v) == 2 && isKey(m, 'a') && isKey(m, 'b'));
%% dav-map-keytype-conversion | containers.Map | Numeric KeyType character key is rejected
m = containers.Map('KeyType', 'double', 'ValueType', 'char');
hata = false; try, m('2') = 'x'; catch, hata = true; end
assert(hata);
%% dav-nested-struct-create | struct | Noktalı atama boş yapıdan iç alan oluşturur
s = struct(); s.a.b = 1;
assert(s.a.b == 1);
%% dav-orderfields | orderfields | Alanları verilen sıralı hücreye göre dizer
s = struct('b', 1, 'a', 2);
t = orderfields(s, {'a', 'b'});
assert(isequal(fieldnames(t), {'a'; 'b'}));
%% dav-rmfield-missing | rmfield | Olmayan alanı kaldırma hata verir
hata = false;
try, s = rmfield(struct('a', 1), 'b'); catch, hata = true; end
assert(hata);
%% dav-getfield-deep | getfield | Derin alan erişimi hücre indeksleriyle çalışır
s.a(1).b = 7;
assert(getfield(s, 'a', {1}, 'b') == 7);
%% dav-deal-control | deal | Bir girdi birden çok çıktıya kopyalanır
[a, b, c] = deal(5);
assert(a == 5 && b == 5 && c == 5);
%% dav-tic-toc-type | tic toc | Toc süresi double skaleridir
h = tic; x = toc(h);
assert(isa(x, 'double') && isscalar(x) && x >= 0);
%% dav-clock-type | clock | Clock altı double tarih alanı verir
x = clock;
assert(isa(x, 'double') && isequal(size(x), [1 6]));
%% dav-datenum-datestr-fixed | datenum datestr | Sabit gün numarasının kısa tarih metni
x = datenum(2024, 1, 2);
assert(strcmp(datestr(x, 'yyyy-mm-dd'), '2024-01-02'));
%% dav-xor-scalar | xor | XOR farklı mantıksal skalerlerde true olur
assert(xor(true, false) && ~xor(true, true));
%% dav-true-size | true | Boyut girdileri belirtilen mantıksal matris üretir
x = true(2, 3);
assert(islogical(x) && isequal(size(x), [2 3]));
%% dav-error-index-identifier | :error-identifier | Alt indis hatasının kimliği farklıdır
id = '';
v = [1 2]; try, x = v(3); catch e, id = e.identifier; end
assert(strcmp(id, 'MATLAB:badsubscript'));
