% Dizgi ve metin
%% dizgip-contains-char | contains string | Karisik desen, mantiksal skaler
x = contains('abc', string('b'));
assert(islogical(x) && isscalar(x) && x);
%% dizgip-contains-cell | contains string | Cellstr boyutu ve birden cok dizgi deseni
x = contains({'abc', 'def'; '', 'xyz'}, string({'bc', 'xy'}));
assert(isequal(x, [true false; false true]));
%% dizgip-contains-ignorecase | contains string | Karisik desen ve secenek adi
assert(contains('ABC', string('b'), string('IgnoreCase'), true));
%% dizgip-replace-old | replace string | Char ilk girdi char sonuc verir
x = replace('abcabc', string('b'), 'X');
assert(ischar(x) && strcmp(x, 'aXcaXc'));
%% dizgip-replace-new | replace string | Son girdide dizgi ve sondaki bosluklar
x = replace('abc', 'b', string('X '));
assert(ischar(x) && strcmp(x, 'aX c'));
%% dizgip-replace-cell | replace string | Cellstr ve eslestirilmis coklu desen
x = replace({'abc'; 'cab'}, string({'ab', 'c'}), string({'X', 'Y'}));
assert(iscell(x) && isequal(x, {'XY'; 'YX'}));
%% dizgip-erase-char | erase string | Char ve cellstr sonuc tipi korunur
x = erase('banana', string('an'));
y = erase({'abc'; 'bcd'}, string('b'));
assert(ischar(x) && strcmp(x, 'ba'));
assert(iscell(y) && isequal(y, {'ac'; 'cd'}));
%% dizgip-compose-scalar | compose string | Char bicim cellstr sonuc verir
x = compose('%d-%s', 3, string('a  '));
assert(iscell(x) && isequal(x, {'3-a  '}));
%% dizgip-compose-rows | compose string | Skaler dizgi verisi sayisal satirlara yayilir
x = compose('%d-%s', [1; 2], string('x'));
assert(isequal(x, {'1-x'; '2-x'}));
%% dizgip-split-match | split string | Karisik ayirac ve iki cellstr cikti
[x, m] = split('a,,b,', string(','));
assert(iscell(x) && isequal(x, {'a'; ''; 'b'; ''}));
assert(iscell(m) && isequal(m, {','; ','; ','}));
%% dizgip-split-cell | split string | Cellstr girdinin yeni boyutu
x = split({'a,b'; 'c,d'}, string(','));
assert(iscell(x) && isequal(x, {'a', 'b'; 'c', 'd'}));
%% dizgip-join-delimiters | join string | Cellstr ve her araliga ayri dizgi ayiraci
x = join({'a', 'b', 'c'}, string({'-', '/'}));
assert(iscell(x) && isequal(x, {'a-b/c'}));
%% dizgip-join-empty | join string | Acik bos dizgi ayiraci ve char girdisi
assert(isequal(join({'a', 'b'}, string('')), {'ab'}));
%% dizgip-join-char-ilk | join string | char ilk girdi char olarak döner; yamada henüz hücre dönüyor
x = join('abc', string('-'));
assert(ischar(x) && strcmp(x, 'abc'));
%% dizgip-count-mixed | count string | Karisik desen ve cakisan eslesme
assert(count('aaaa', string('aa')) == 2);
assert(isequal(count({'banana', 'none'}, string('ana')), [1 0]));
%% dizgip-contains-empty | contains string | Bos desen bos olmayan ve bos dizgilerde eslesir
s = string({'abc', ''});
assert(isequal(contains(s, ''), [true true]));
assert(isequal(contains(s, string({'z', ''})), [true true]));
assert(isequal(contains(s, '', 'IgnoreCase', true), [true true]));
%% dizgip-count-nonoverlap | count string | Dizgi dizisinde ortusen literal eslesmeler
s = string({'aaaa', 'banana', 'aaaaa', ''});
assert(isequal(count(s, 'aa'), [2 0 2 0]));
assert(isequal(count(s, 'ana'), [0 1 0 0]));
assert(count(string('AaAa'), 'aa', 'IgnoreCase', true) == 2);
%% dizgip-plus-char | string | Iki yonde char birlestirme ve sondaki bosluklar
s = string('a ');
assert(isequal(s + 'b ', string('a b ')));
assert(isequal('b ' + s, string('b a ')));
assert(isequal(s + '', s));
%% dizgip-plus-array | string | Dizgi dizisi ve skaler char birlestirme
s = string({'a', 'b'});
assert(isequal(s + 'x', string({'ax', 'bx'})));
assert(isequal('x' + s, string({'xa', 'xb'})));
%% dizgip-plus-numeric | string | Kucuk sonlu tam sayi skalerleri metne donusur
s = string('x');
assert(isequal(s + 3, string('x3')));
assert(isequal(-12 + s, string('-12x')));
assert(isequal(s + int32(7), string('x7')));
assert(isequal(s + 0, string('x0')));
%% dizgip-string-first-query | contains count string | Var olan dizgi-ilk sorgu boyutu ve secenekleri
s = string({'ABC', 'banana'; 'none', 'a.b'});
assert(isequal(contains(s, 'b', 'IgnoreCase', true), [true true; false true]));
assert(isequal(count(s, 'a'), [0 3; 0 1]));
%% dizgip-string-first-edit | replace erase string | Var olan dizgi-ilk donus tipi
s = string({'abc'; 'cab'});
assert(isequal(replace(s, 'b', 'X'), string({'aXc'; 'caX'})));
assert(isequal(erase(s, 'b'), string({'ac'; 'ca'})));
%% dizgip-string-first-layout | compose split join string | Var olan dizgi-ilk bicim ve bolme tipleri
assert(isequal(compose(string('%d-%s'), 3, 'a'), string('3-a')));
[x, m] = split(string('a,b'), ',');
assert(isequal(x, string({'a'; 'b'})) && isequal(m, string(',')));
assert(isequal(join(string({'a', 'b'}), '-'), string('a-b')));
