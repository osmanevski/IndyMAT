% Dil ve programlama
%% soz-string-scalar | :cift-tirnak-string | MATLAB cift tirnak metni string nesnesidir
s = "abc";
assert(isa(s, 'string') && isequal(size(s), [1 1]));
%% soz-string-plus | :cift-tirnak-string | Dizgi artisi string birlestirir
assert(isequal("ab" + "cd", "abcd"));
%% soz-string-bracket-concat | :cift-tirnak-string | Koseli parantez string dizisi kurar
s = ["a" "b"];
assert(isa(s, 'string') && isequal(size(s), [1 2]));
%% soz-string-cell | :cift-tirnak-string | Hucre icindeki cift tirnak degeri string kalir
c = {"a", "b"};
assert(isa(c{1}, 'string') && isequal(c, {"a", "b"}));
%% soz-string-eq | :cift-tirnak-string | String esitligi skaler mantiksal verir
assert("same" == "same");
%% soz-string-length-vs-numel | strlength numel | Metin uzunlugu oge sayisindan ayridir
s = "abcd";
assert(strlength(s) == 4 && numel(s) == 1);
%% soz-string-index | :cift-tirnak-string | String indislemesi ogeyi secer
s = ["red" "blue"];
assert(s(2) == "blue");
%% soz-string-escaped-quote | :cift-tirnak-string | Cift tirnak icinde tirnak ikilenir
s = "a""b";
assert(s == "a""b");
%% soz-char-escape-single | :tek-tirnak-kacis | Tek tirnakta ters bolu kacis karakteri degildir
s = 'a\nb';
assert(numel(s) == 4 && s(2) == '\');
%% soz-sprintf-string | sprintf | sprintf string girdisini kabul eder
s = sprintf('%s-%d', "x", 3);
assert(strcmp(s, 'x-3'));
%% soz-string-missing | string ismissing | missing metin string skaleridir
s = string(missing);
assert(isa(s, 'string') && ismissing(s));
%% soz-hex-literal | :hex-literal | Onaltilik literal tamsayi turu tasir
x = 0x2A;
assert(x == 42 && isa(x, 'uint8'));
%% soz-binary-literal | :binary-literal | Ikilik literal tamsayi turu tasir
x = 0b101010;
assert(x == 42 && isa(x, 'uint8'));
%% soz-hex-suffix | :hex-literal | Onaltilik soneki turu belirler
x = 0x2Au16;
assert(isa(x, 'uint16') && x == 42);
%% soz-binary-suffix | :binary-literal | Ikilik soneki turu belirler
x = 0b101010s16;
assert(isa(x, 'int16') && x == 42);
%% soz-scientific-literal | :sayisal-literal | Bilimsel gosterim ondalik sayidir
assert(1e3 == 1000 && 2.5E-2 == 0.025);
%% soz-imaginary-i | :imaginary-literal | i sanal birim eki olarak kullanilir
z = 3i;
assert(real(z) == 0 && imag(z) == 3);
%% soz-imaginary-j | :imaginary-literal | j sanal birim eki olarak kullanilir
z = 3j;
assert(real(z) == 0 && imag(z) == 3);
%% soz-integer-suffix-control | :sayisal-literal | Tam sayi donusum islevi tamsayi turunu verir
x = uint8(42);
assert(isa(x, 'uint8') && x == 42);
%% soz-not-vs-ne | :operator-onceligi | Mantiksal degil ile esit degil ayridir
x = [0 1];
assert(isequal(~logical(x), [true false]) && isequal(x ~= 0, [false true]));
%% soz-unary-minus-power | :operator-onceligi | Kuvvet tekli eksiden once baglanir
assert(-2^2 == -4);
%% soz-element-power-transpose | :operator-onceligi | Eleman bazli kuvvet ve transpoz
A = [1 2; 3 4];
assert(isequal((A.^2).', [1 9; 4 16]));
%% soz-short-circuit-scalar | :short-circuit | && iki skaler kosulu kisa devre eder
n = 0;
if false && bump(), n = 1; end
assert(n == 0);
%% soz-if-bitwise-array-control | :if-ve-vektor | if icinde | tum ogeleri degerlendirir
y = 0;
if [true false] | [false true], y = 1; end
assert(y == 1);
%% soz-if-and-array | :if-ve-vektor | if kosulunda tum ogeler dogruysa blok calisir
x = [1 2]; y = 0;
if x > 0, y = 1; end
assert(y == 1);
%% soz-colon-plus-precedence | :iki-nokta-onceligi | Iki nokta toplama isleminden once baglanir
assert(isequal(1:3+1, [1 2 3 4]));
%% soz-implicit-expand-control | :ortuk-genisleme | Satir ve sutun ortuk genisler
A = [1;2] + [10 20];
assert(isequal(A, [11 21;12 22]));
%% soz-end-nested | :end-indeks | Ic ice indekslemede end yerel boyutu kullanir
A = reshape(1:12, 3, 4);
assert(A(end, end-1) == A(3, 3));
%% soz-end-function-handle | :end-indeks | Anonim govdede end indeksin son oge konumudur
f = @(x) x(end);
assert(f([4 5 6]) == 6);
%% soz-end-method-index | :end-indeks | Hucre icinde end ile son oge secilir
c = {1, 2, 3};
assert(c{end} == 3);
%% soz-linear-index-control | :lineer-indeks | Matris tek indisle sutun-major duzende okunur
A = [1 2;3 4];
assert(isequal(A([2 3]), [3 2]));
%% soz-logical-index | :mantiksal-indeks | Mantiksal indis dogru ogeleri alir
A = [4 7 9];
assert(isequal(A([true false true]), [4 9]));
%% soz-delete-index | :indeks-silme | Bos atama vektor oge siler
A = [1 2 3 4]; A(2:3) = [];
assert(isequal(A, [1 4]));
%% soz-grow-index | :indeks-buyutme | Atama dizi sonrasina sifirlarla buyutur
A = [1 2]; A(5) = 9;
assert(isequal(A, [1 2 0 0 9]));
%% soz-function-result-index-control | :ifade-indeksleme | Gecici degisken cagri sonucunu indeksler
tmp = sin([0 pi/2]);
assert(tmp(2) == 1);
%% soz-struct-csl | struct | Yapisal dizi alanlari virgul ayrimli liste olusturur
s = struct('x', {2, 4});
v = {s.x};
assert(isequal(v, {2, 4}));
%% soz-dynamic-field | struct | Dinamik alan adi nokta indislemesi
s = struct(); name = 'value'; s.(name) = 7;
assert(s.(name) == 7);
%% soz-function-result-dot-control | :ifade-nokta-indeksleme | Fonksiyon sonucunu gecici yapida alanla oku
s = struct('value', 11); tmp = s;
assert(tmp.value == 11);
%% soz-function-result-dot | :ifade-nokta-indeksleme | Fonksiyon sonucunun alanina dogrudan erisim
assert(struct('value', 11).value == 11);
%% soz-block-comment | :blok-yorum | %{ %} tek satirli blok yorumdur
%{
ignored = 99;
%}
assert(true);
%% soz-continuation-trailing-comment | :satir-devami | Devam isaretinden sonra yorum yazilabilir
x = 2 + ... % islem surer
  3;
assert(x == 5);
%% soz-continuation-inside-brackets | :satir-devami | Koseli parantez icinde satir devam eder
A = [1 2 ...
     3 4];
assert(isequal(A, [1 2 3 4]));
%% soz-matrix-comma-semicolon | :matris-ayraclari | Virgül sutun, noktalivirgul satir ayirir
A = [1, 2; 3, 4];
assert(isequal(A, [1 2;3 4]));
%% soz-matrix-newline | :matris-ayraclari | Matris satirlari yeni satirla ayrilabilir
A = [1 2
     3 4];
assert(isequal(A, [1 2;3 4]));
%% soz-matrix-semicolon-suppresses | :matris-ayraclari | Noktalivirgul komut cikti bastirmasini engeller
A = [1 2; 3 4];
assert(A(2,2) == 4);
%% soz-whitespace-negative-vector | :bosluk-duyarliligi | Bosluk eksi tekli isaretini ayristirir
a = 1; b = 2;
assert(isequal([a -b], [1 -2]));
%% soz-whitespace-subtract-vector | :bosluk-duyarliligi | Bosluklu eksi ikili cikarmadir
a = 1; b = 2;
assert(isequal([a - b], -1));
%% soz-command-hold | :komut-sozdizimi | Komut bicimi argumansiz hold on calisir
f = figure('visible', 'off'); ax = axes(f); hold(ax, 'on');
assert(strcmp(get(ax, 'nextplot'), 'add')); close(f);
%% soz-command-format | :komut-sozdizimi | format long komut bicimi cikti bicimini degistirir
before = format(); format long; after = format(); format(before);
assert(~isempty(after));
%% soz-command-close-all | :komut-sozdizimi | close all komut bicimi acik pencereleri kapatir
close all;
assert(true);
%% soz-command-clear | :komut-sozdizimi | clear komutunda birden cok isim verilebilir
x = 1; y = 2; clear x y
assert(~exist('x', 'var') && ~exist('y', 'var'));
%% soz-command-cd-control | cd | cd ile gecici klasore gecis ve geri donus
old = pwd; cd(tempdir); moved = ~strcmp(pwd, old); cd(old);
assert(moved);
%% soz-command-ambiguity-control | :komut-sozdizimi | Tirnakli ifade eksi birle degil cikarimdir
a = 5; x = a - 1;
assert(x == 4);
%% soz-switch-cell | :switch-case | switch cell case alternatifleriyle calisir
x = 'b'; y = 0;
switch x
  case {'a', 'b'}, y = 1;
  otherwise, y = 2;
end
assert(y == 1);
%% soz-switch-string | :switch-case | String skaler switch ile eslesir
x = "b"; y = 0;
switch x
  case "b", y = 1;
  otherwise, y = 2;
end
assert(y == 1);
%% soz-switch-otherwise | :switch-case | Eslesmeyen durumda otherwise calisir
x = 3; y = 0;
switch x
  case 1, y = 1;
  otherwise, y = 2;
end
assert(y == 2);
%% soz-for-matrix-columns | :for-dongusu | for matris sutunlarini sirayla dolasir
A = [1 2;3 4]; seen = [];
for col = A, seen(end+1) = sum(col); end
assert(isequal(seen, [4 6]));
%% soz-for-cell-array | :for-dongusu | for hucre dizisinde sutun oge uzerinden dolasir
c = {'a', 'bb'}; seen = 0;
for item = c, seen = seen + numel(item{1}); end
assert(seen == 3);
%% soz-while-array-condition | :while-kosulu | while vektor kosul tum ogeler dogruyken surer
x = [1 1]; n = 0;
while x > 0, x = x - 1; n = n + 1; end
assert(n == 1);
%% soz-break | :break | break donguyu hemen sonlandirir
n = 0;
for k = 1:5, if k == 3, break; end, n = n + 1; end
assert(n == 2);
%% soz-continue | :continue | continue yalniz o dongu turunu atlar
s = 0;
for k = 1:4, if k == 2, continue; end, s = s + k; end
assert(s == 8);
%% soz-try-catch-identifier | error | catch nesnesi kimlik alanini tasir
ok = false;
try
  error('Ornek:kimlik', 'mesaj');
catch e
  ok = strcmp(e.identifier, 'Ornek:kimlik');
end
assert(ok);
%% soz-return-control | uy_soz_return | return fonksiyondan erken cikis yapar
assert(uy_soz_return(4) == 4 && uy_soz_return(0) == 0);
%% soz-anon-capture | :anonim-fonksiyon | Anonim fonksiyon yakaladigi degeri saklar
a = 3; f = @(x) x + a; a = 9;
assert(f(2) == 5);
%% soz-nested-anon | :anonim-fonksiyon | Ic ice anonim tutamaclar cagrilabilir
f = @(x) @(y) x + y; g = f(3);
assert(g(4) == 7);
%% soz-handle-plus | @plus | Yerlesik islecin fonksiyon tutamaci kullanilir
f = @plus;
assert(f(2, 3) == 5);
%% soz-handle-method | :method-handle | Nesne metodu icin bagli tutamac olusturulur
p = uy_nokta(3, 4); f = @p.olcekle; q = f(2);
assert(q.x == 6 && q.y == 8);
%% soz-handle-nargin | nargin | Tutamacin gerekli arguman sayisi okunur
f = @(x, y) x + y;
assert(nargin(f) == 2);
%% soz-func2str-handle | func2str | Tutamac metne cevrilir
f = @sin;
assert(strcmp(func2str(f), 'sin'));
%% soz-str2func-handle | str2func | Isim metninden tutamac kurulur
f = str2func('cos');
assert(f(0) == 1);
%% soz-cellfun-handle | cellfun | cellfun anonim tutamacla oge uzunluklarini hesaplar
y = cellfun(@numel, {'a', 'bc'});
assert(isequal(y, [1 2]));
%% soz-cellfun-legacy-name-control | cellfun | cellfun yerlesik adini metin olarak alabilir
y = cellfun('isempty', {[], 1});
assert(isequal(y, [true false]));
