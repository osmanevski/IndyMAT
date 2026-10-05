% Dizgi ve metin
%% dizgiy-contains-char-coklu | contains | char ve birden cok desen
assert(contains('ankara', {'kar', 'iz'}));
assert(~contains('ankara', {'xyz', '123'}));
assert(contains('', ''));
assert(contains('abc', ''));
%% dizgiy-contains-cell-ignorecase | contains | cellstr boyutu ve IgnoreCase
x = contains({'Elma', 'ARMUT'; '', 'karaz'}, 'ar', 'IgnoreCase', true);
assert(isequal(x, [false true; false true]));
%% dizgiy-baslar-biter-char | startsWith endsWith | char ve IgnoreCase
assert(startsWith('IndyMAT', 'indy', 'IgnoreCase', true));
assert(endsWith('IndyMAT', {'MAT', 'xyz'}));
assert(~startsWith('IndyMAT', 'MAT'));
%% dizgiy-baslar-biter-cell | startsWith endsWith | cellstr boyutu
x = {'alpha', 'Beta'; '', 'gamma'};
assert(isequal(startsWith(x, {'al', 'ga'}), [true false; false true]));
assert(isequal(endsWith(x, 'A', 'IgnoreCase', true), [true true; false true]));
%% dizgiy-count-cakisma | count | cakisan eslesmeler bir kez kullanilir
assert(count('aaaa', 'aa') == 2);
assert(count('banana', 'ana') == 1);
%% dizgiy-count-cell-coklu | count | cellstr, coklu desen ve IgnoreCase
x = count({'Red blue RED', 'none'}, {'red', 'blue'}, 'IgnoreCase', true);
assert(isequal(x, [3 0]));
%% dizgiy-replace-char-coklu | replace | eszamanli coklu degistirme
x = replace('abcabc', {'ab', 'c'}, {'X', 'Y'});
assert(ischar(x) && strcmp(x, 'XYXY'));
%% dizgiy-replace-cell-no-match | replace | cellstr tipi ve eslesme yok
x = replace({'abc'; 'def'}, 'x', 'Y');
assert(iscell(x) && isequal(x, {'abc'; 'def'}));
%% dizgiy-erase-cakisma | erase | cakisan eslesme ve char tipi
x = erase('aaaa', 'aa');
assert(ischar(x) && strcmp(x, ''));
%% dizgiy-reverse-tip-bos | reverse | char, cellstr ve bos metin
assert(ischar(reverse('')) && isempty(reverse('')));
assert(strcmp(reverse('abc'), 'cba'));
assert(isequal(reverse({'ab', ''; 'xyz', 'q'}), {'ba', ''; 'zyx', 'q'}));
%% dizgiy-strip-bosluk | strip | varsayilan bosluklar ve tip koruma
x = strip({sprintf('  a\t'), sprintf('\nb\r')});
assert(iscell(x) && isequal(x, {'a', 'b'}));
%% dizgiy-strip-yon-karakter | strip | yon ve tek karakter
assert(strcmp(strip('00012.30', 'left', '0'), '12.30'));
assert(strcmp(strip('xxabcxxx', 'right', 'x'), 'xxabc'));
%% dizgiy-pad-varsayilan | pad | cellstr en uzun elemana tamamlanir
x = pad({'a', 'bbb'});
assert(isequal(x, {'a  ', 'bbb'}));
%% dizgiy-pad-yon-karakter | pad | uzunluk, iki yan ve dolgu karakteri
x = pad('a', 4, 'both', '-');
assert(ischar(x) && strcmp(x, '-a--'));
assert(strcmp(pad('abcdef', 3), 'abcdef'));
%% dizgiy-strlength-tip-bos | strlength | cellstr boyutu ve bos metin
x = strlength({'a', ''; 'abcd', 'xy'});
assert(isequal(x, [1 0; 4 2]));
assert(strlength('') == 0);
%% dizgiy-extract-side-pattern | extractBefore extractAfter | desen ile cikarma
assert(strcmp(extractBefore('abc--def', '--'), 'abc'));
assert(strcmp(extractAfter('abc--def', '--'), 'def'));
%% dizgiy-extract-side-position | extractBefore extractAfter | konum ve cellstr
x = extractBefore({'abc', 'wxyz'}, [2 4]);
y = extractAfter({'abc', 'wxyz'}, [1 2]);
assert(isequal(x, {'a', 'wxy'}));
assert(isequal(y, {'bc', 'yz'}));
%% dizgiy-extract-between-pattern | extractBetween | desen sinirlari ve cell cikti
x = extractBetween('a[xy]b', '[', ']');
y = extractBetween('a[xy]b', '[', ']', 'Boundaries', 'inclusive');
assert(iscell(x) && isequal(x, {'xy'}));
assert(isequal(y, {'[xy]'}));
%% dizgiy-extract-between-position | extractBetween | konum sinirlari
x = extractBetween('abcdef', 2, 4);
y = extractBetween('abcdef', 2, 4, 'Boundaries', 'exclusive');
assert(isequal(x, {'bcd'}) && isequal(y, {'c'}));
%% dizgiy-split-tekrar | split | yinelenen ayiraclar korunur
x = split('a,,b,', ',');
assert(isequal(x, {'a'; ''; 'b'; ''}));
%% dizgiy-split-coklu-match | split | coklu ayirac ve ikinci cikti
[x, m] = split('a--b,c', {'--', ','});
assert(isequal(x, {'a'; 'b'; 'c'}));
assert(isequal(m, {'--'; ','}));
%% dizgiy-split-cell-boyut | split | cellstr yeni boyut yerlesimi
x = split({'a,b'; 'c,d'}, ',');
assert(isequal(size(x), [2 2]));
assert(isequal(x, {'a', 'b'; 'c', 'd'}));
%% dizgiy-join-temel | join | satir ve sutun cellstr
x = join({'a', 'b'}, '-');
y = join({'a'; 'b'}, '-');
assert(iscell(x) && isequal(size(x), [1 1]) && isequal(x, {'a-b'}));
assert(iscell(y) && isequal(size(y), [1 1]) && isequal(y, {'a-b'}));
%% dizgiy-join-ayirac-dizisi | join | her araliga farkli ayirac
x = join({'a', 'b', 'c'}, {'-', '/'});
assert(isequal(x, {'a-b/c'}));
%% dizgiy-splitlines-crlf | splitlines | CRLF tek sinir ve sondaki bos satir
x = splitlines(sprintf('a\r\nb\n'));
assert(isequal(x, {'a'; 'b'; ''}));
%% dizgiy-compose-temel | compose | char bicim cellstr dondurur
x = compose('%d-%s', 3, 'a');
assert(iscell(x) && isequal(x, {'3-a'}));
%% dizgiy-compose-dizi | compose | satir verisi ve yinelenen bicim
x = compose('%02d', [1 2 3]);
assert(isequal(x, {'01', '02', '03'}));
%% dizgiy-compose-kacis | compose | bicimsiz metinde kacis dizileri
x = compose({'a\nb', 'c\td'});
assert(isequal(x, {sprintf('a\nb'), sprintf('c\td')}));
%% dizgiy-desteklenmeyen-tip-hata | contains pad | desteklenmeyen girdiler sessizce kabul edilmez
didThrow = false;
try, contains(42, '2'); catch, didThrow = true; end
assert(didThrow);
didThrow = false;
try, pad('a', 3, 'middle'); catch, didThrow = true; end
assert(didThrow);
%% dizgiy-karisik-contains | contains string | Bilinen paket dispatch acigi; MATLAB true
assert(contains('abc', string('b')));
%% dizgiy-karisik-replace | replace string | Bilinen paket dispatch acigi; MATLAB char
x = replace('abc', string('b'), 'X');
assert(ischar(x) && strcmp(x, 'aXc'));
%% dizgiy-karisik-compose | compose string | Bilinen paket dispatch acigi; MATLAB cell
assert(isequal(compose('%s', string('a')), {'a'}));
%% dizgiy-karisik-erase | erase string | Bilinen paket dispatch acigi; MATLAB char
x = erase('abc', string('b'));
assert(ischar(x) && strcmp(x, 'ac'));
%% dizgiy-karisik-split | split string | Bilinen paket dispatch acigi; MATLAB cell
assert(isequal(split('a,b', string(',')), {'a'; 'b'}));
%% dizgiy-karisik-join | join string | Bilinen paket dispatch acigi; MATLAB cell
assert(isequal(join({'a', 'b'}, string('-')), {'a-b'}));
%% dizgiy-dizisi-contains-bos | contains string | Bilinen paket semantik acigi; bos desen eslesir
assert(contains(string('abc'), ''));
%% dizgiy-dizisi-count-cakisma | count string | Bilinen paket semantik acigi; cakisan metin tekrar sayilmaz
assert(count(string('aaaa'), 'aa') == 2);
%% dizgiy-literal-sorgu | contains startsWith endsWith count | Regex karakterleri duz metindir
assert(contains('a.b', '.') && ~contains('abc', '.'));
assert(startsWith('[abc', '[') && endsWith('abc$', '$'));
assert(count('a.a.a', '.') == 2);
%% dizgiy-literal-degistirme | replace erase | Regex karakterleri ve cakisan desenler
assert(strcmp(replace('a+b', '+', '-'), 'a-b'));
assert(strcmp(erase('a.b', '.'), 'ab'));
assert(strcmp(replace('aaa', 'aa', 'X'), 'Xa'));
assert(strcmp(erase('aaa', 'aa'), 'a'));
%% dizgiy-literal-bolme-birlestirme | split join | Duz ayiraclar ve acik bos ayirac
assert(isequal(split('a|b', '|'), {'a'; 'b'}));
assert(isequal(join({'a', 'b'}, ''), {'ab'}));
%% dizgiy-utf8-uzunluk-ters | strlength reverse | Turkce UTF-8 karakterleri
assert(strlength('çığ') == 3);
assert(isequal(strlength({'çığ', 'şöü'}), [3 3]));
assert(strcmp(reverse('çığ'), 'ğıç'));
assert(isequal(reverse({'çığ', 'şöü'}), {'ğıç', 'üöş'}));
%% dizgiy-utf8-pad-strip | pad strip | Turkce metin ve tek cok-baytli karakter
assert(strcmp(pad('çığ', 5), 'çığ  '));
assert(strcmp(pad('çığ', 5, 'left', 'ö'), 'ööçığ'));
assert(isequal(pad({'ç', 'ığ'}), {'ç ', 'ığ'}));
assert(strcmp(strip('ööçığöö', 'ö'), 'çığ'));
assert(isequal(strip({' çığ ', ' şü '}), {'çığ', 'şü'}));
%% dizgiy-utf8-konum | extractBefore extractAfter extractBetween | Konumlar bayt degil karakter sayar
assert(strcmp(extractBefore('çığ', 2), 'ç'));
assert(strcmp(extractAfter('çığ', 2), 'ğ'));
assert(isequal(extractBetween('çığ', 2, 3), {'ığ'}));
assert(isequal(extractBefore({'çığ', 'şöü'}, [2 3]), {'ç', 'şö'}));
assert(isequal(extractAfter({'çığ', 'şöü'}, [1 2]), {'ığ', 'ü'}));
assert(isequal(extractBetween({'çığ', 'şöü'}, 2, 3), {'ığ', 'öü'}));
%% dizgiy-utf8-literal | contains startsWith endsWith count replace erase split join | Turkce buyuk-kucuk harf duyarli literal desenler
assert(contains('çığ', 'ığ') && startsWith('çığ', 'ç') && endsWith('çığ', 'ğ'));
assert(count('çığçığ', 'ığ') == 2);
assert(strcmp(replace('çığ', 'ı', 'ö'), 'çöğ'));
assert(strcmp(erase('çığ', 'ı'), 'çğ'));
assert(isequal(split('ç|ığ', '|'), {'ç'; 'ığ'}));
assert(isequal(join({'ç', 'ığ'}, ''), {'çığ'}));
%% dizgiy-utf8-satir-bicim | splitlines compose | Turkce satirlar ve bicimlenmemis yuzde-s
assert(isequal(splitlines(sprintf('çığ\nşöü')), {'çığ'; 'şöü'}));
assert(isequal(compose('%s', 'çığ'), {'çığ'}));
assert(isequal(compose('%s', 'a  '), {'a  '}));
%% dizgiy-gecersiz-secenekler | contains startsWith endsWith count strip split extractBetween | Bilinmeyen secenekler ve fazla girdiler
failed = false; try, contains('a', 'a', 'Bogus', true); catch, failed = true; end; assert(failed);
failed = false; try, startsWith('a', 'a', 'Bogus', true); catch, failed = true; end; assert(failed);
failed = false; try, endsWith('a', 'a', 'Bogus', true); catch, failed = true; end; assert(failed);
failed = false; try, count('a', 'a', 'Bogus', true); catch, failed = true; end; assert(failed);
failed = false; try, strip('a', 'middle'); catch, failed = true; end; assert(failed);
failed = false; try, split('a', ',', 1, 'Bogus'); catch, failed = true; end; assert(failed);
failed = false; try, extractBetween('[a]', '[', ']', 'Bogus', true); catch, failed = true; end; assert(failed);
%% dizgiy-gecersiz-tip-konum | replace reverse strlength extractBefore extractAfter pad join compose | Gecersiz tipler, konum ve bicim
failed = false; try, replace('a', 42, 'b'); catch, failed = true; end; assert(failed);
failed = false; try, reverse(42); catch, failed = true; end; assert(failed);
failed = false; try, strlength(42); catch, failed = true; end; assert(failed);
failed = false; try, extractBefore('abc', 1.5); catch, failed = true; end; assert(failed);
failed = false; try, extractAfter('abc', NaN); catch, failed = true; end; assert(failed);
failed = false; try, pad('abc', -1); catch, failed = true; end; assert(failed);
failed = false; try, join({'a', 'b'}, '-', 1.5); catch, failed = true; end; assert(failed);
failed = false; try, compose('%q', 3); catch, failed = true; end; assert(failed);
