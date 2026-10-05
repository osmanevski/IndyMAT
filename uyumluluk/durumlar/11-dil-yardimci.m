% Dil ve programlama
%% dily-report-basic | getReport error | Yakalanan hatanın temel düz metin raporu
caught = false;
try
  error('Modul:hata', 'Kod %d', 3);
catch e
  report = getReport(e, 'basic', 'hyperlinks', 'off');
  assert(ischar(report) && ~isempty(strfind(report, 'Kod 3')));
  caught = true;
end
assert(caught);
%% dily-timeit | timeit | Gerçek süre sıfır girdili işlevden ölçülür
t = timeit(@() sum(sin(1:200000)));
assert(isscalar(t) && isfinite(t) && t > 0);
%% dily-timeit-no-output | timeit | Çıktısız çağrı gerçek beklemeyi ölçer
t = timeit(@() pause(0.002), 0);
assert(isfinite(t) && t >= 0.001);
%% dily-dict-numeric | dictionary numEntries | Sayısal anahtarlarla kurucu ve arama
d = dictionary([3 1 2], [30 10 20]);
assert(d(1) == 10 && numEntries(d) == 3);
assert(isequal(d([2 3]), [20 30]));
%% dily-dict-pairs | dictionary | Ardışık anahtar/değer çiftleri
d = dictionary(1, 10, 2, 20);
assert(d(1) == 10 && d(2) == 20);
%% dily-dict-text | dictionary string | Gerçek string anahtarları
d = dictionary(string({'a','b'}), [1 2]);
assert(d(string('b')) == 2 && d('a') == 1);
%% dily-dict-string-char-lookup | dictionary string | String anahtarlı sözlükte char ile arama
d = dictionary(string({'alpha','beta'}), [1 2]);
assert(d('beta') == 2);
%% dily-dict-logical | dictionary isKey | Mantıksal anahtarlar ve varlık sorgusu
d = dictionary([true false], [10 20]);
assert(d(false) == 20 && isequal(isKey(d, [false true]), [true true]));
%% dily-dict-order | dictionary keys values | Ekleme sırası korunur
d = dictionary([3 1 2], [30 10 20]);
assert(isequal(keys(d), [3;1;2]) && isequal(values(d), [30;10;20]));
%% dily-dict-update | dictionary numEntries keys | Güncelleme sıra ve sayıyı korur
d = dictionary([3 1], [30 10]); d(3) = 31; d(2) = 20;
assert(d(3) == 31 && numEntries(d) == 3 && isequal(keys(d), [3;1;2]));
%% dily-dict-vector-write | dictionary | Vektör anahtar ataması
d = dictionary([1 2], [10 20]); d([2 3]) = [21 30];
assert(isequal(d([1 2 3]), [10 21 30]));
d([2 3]) = 9; assert(isequal(d([2 3]), [9 9]));
e = dictionary([1 2], 7); assert(isequal(e([1 2]), [7 7]));
%% dily-dict-remove | dictionary numEntries keys | Boş atama siler, tekrar ekleme sona gelir
d = dictionary([1 2 3], [10 20 30]); d(2) = []; d(2) = 22;
assert(numEntries(d) == 3 && isequal(keys(d), [1;3;2]));
%% dily-dict-value-copy | dictionary | Değer kopyaları bağımsızdır
d = dictionary(1, 10); e = d; e(1) = 20;
assert(d(1) == 10 && e(1) == 20);
%% dily-dict-cell-value | dictionary values | Hücre değerleri hücre olarak döner
d = dictionary([1 2], {magic(2), 'abc'});
v = d(2); assert(iscell(v) && strcmp(v{1}, 'abc'));
v = values(d); assert(isequal(size(v), [2 1]));
%% dily-dict-string-value | dictionary string values | String değer sınıfı korunur
d = dictionary([1 2], string({'a','b'}));
assert(isa(d(1), 'string') && strcmp(char(d(2)), 'b'));
assert(isequal(size(values(d)), [2 1]));
%% dily-dict-configured | dictionary isConfigured numEntries configureDictionary types | Boş ve açık tür yapılandırması
d = dictionary(); assert(~isConfigured(d) && numEntries(d) == 0);
d = configureDictionary('double', 'cell');
assert(isConfigured(d) && numEntries(d) == 0);
[kt, vt] = types(d); assert(strcmp(char(kt), 'double') && strcmp(char(vt), 'cell'));
d(1) = {42}; v = d(1); assert(v{1} == 42);
%% dily-dict-insert-remove | dictionary insert remove numEntries | İşlevsel ekleme ve silme
d = dictionary([1 2], [10 20]); e = insert(d, [2 3], [21 30]);
assert(d(2) == 20 && e(2) == 21 && e(3) == 30);
e = remove(e, [1 3]); assert(numEntries(e) == 1 && e(2) == 21);
%% dily-dict-fallback | dictionary lookup isKey | Eksik anahtarlar için açık varsayılan
d = dictionary([1 2], [10 20]);
assert(isequal(lookup(d, [2 3], 'FallbackValue', -1), [20 -1]));
assert(isequal(isKey(d, [2 3]), [true false]));
%% dily-dict-missing | dictionary | Eksik anahtar erişimi hata verir
d = dictionary(1, 10); failed = false;
try, v = d(2); catch, failed = true; end
assert(failed);
%% dily-dict-duplicates | dictionary numEntries | Yinelenen anahtarın son değeri kullanılır
d = dictionary([3 1 3], [30 10 31]);
assert(numEntries(d) == 2 && d(3) == 31);
%% dily-report-extended | getReport error rethrow | Yeniden fırlatma kimlik ve mesajı korur
caught = false;
try
  try, error('Modul:hata', 'Beklenen hata'); catch e, rethrow(e); end
catch e
  report = getReport(e, 'extended', 'hyperlinks', 'off');
  assert(strcmp(e.identifier, 'Modul:hata') && ~isempty(strfind(report, 'Beklenen hata')));
  caught = true;
end
assert(caught);
