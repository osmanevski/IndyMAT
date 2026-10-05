% Dizgi ve metin
%% dizgi-bicimleme | sprintf num2str int2str mat2str
assert(strcmp(sprintf('%5.2f|%d|%s', pi, 7, 'a'), ' 3.14|7|a'));
assert(strcmp(num2str(pi), '3.1416') && strcmp(num2str(3), '3'));
assert(strcmp(num2str(pi, 8), '3.1415927'));
assert(strcmp(int2str(2.7), '3'));
assert(strcmp(mat2str([1 2; 3 4]), '[1 2;3 4]'));
%% dizgi-sayiya-donusum | str2double str2num sscanf
assert(str2double('3.5e2') == 350 && isnan(str2double('abc')));
assert(isequaln(str2double({'1', 'x', '2.5'}), [1 NaN 2.5]));
assert(isequal(str2num('[1 2 3]'), [1 2 3]));
assert(isequal(sscanf('3 4 5', '%d')', [3 4 5]));
%% dizgi-bolme-birlestirme | strsplit strjoin strcat strtrim strrep
assert(isequal(strsplit('a,b,,c', ','), {'a', 'b', 'c'}));
assert(isequal(strsplit('a,b,,c', ',', 'CollapseDelimiters', false), {'a', 'b', '', 'c'}));
assert(strcmp(strjoin({'a', 'b', 'c'}, '-'), 'a-b-c'));
assert(strcmp(strcat('ab', 'cd'), 'abcd'));
assert(strcmp(strtrim(sprintf('  a b \t')), 'a b'));
assert(strcmp(strrep('abcabc', 'b', 'X'), 'aXcaXc'));
%% dizgi-karsilastirma | strcmp strcmpi strncmp strfind upper lower
assert(strcmp('a', 'a') && ~strcmp('a', 'A') && strcmpi('a', 'A') && strncmp('abcd', 'abxx', 2));
assert(isequal(strcmp({'a', 'b'}, 'a'), [true false]));
assert(isequal(strfind('abcabc', 'bc'), [2 5]));
assert(strcmp(upper('aB'), 'AB') && strcmp(lower('aB'), 'ab'));
%% dizgi-regexp | regexp regexprep
[tok, eslesme] = regexp('ab12cd345', '(\d+)', 'tokens', 'match');
assert(isequal(eslesme, {'12', '345'}) && strcmp(tok{2}{1}, '345'));
adlar = regexp('x=10', '(?<ad>\w)=(?<deger>\d+)', 'names');
assert(strcmp(adlar.ad, 'x') && strcmp(adlar.deger, '10'));
assert(strcmp(regexprep('abc123', '\d+', '#'), 'abc#'));
assert(strcmp(regexprep('ab', '(a)(b)', '$2$1'), 'ba'));
assert(regexp('hello', 'l+') == 3);
%% dizgi-icerik-sorgu | contains startsWith endsWith
assert(contains('abcdef', 'cd') && ~contains('abcdef', 'x'));
assert(isequal(contains({'elma', 'armut'}, 'ma'), [true false]));
assert(contains('ABC', 'b', 'IgnoreCase', true));
assert(startsWith('abcdef', 'abc') && endsWith('abcdef', 'def'));
%% dizgi-char-sayma-degistirme | count replace erase reverse
assert(count('banana', 'an') == 2);
assert(strcmp(replace('abc', 'b', 'X'), 'aXc'));
assert(strcmp(erase('banana', 'an'), 'ba'));
assert(strcmp(reverse('abc'), 'cba'));
%% dizgi-char-kirpma | strip pad
assert(strcmp(strip('  a  '), 'a'));
assert(strcmp(pad('a', 3), 'a  '));
%% dizgi-strlength-char | strlength
assert(strlength('abc') == 3);
assert(isequal(strlength({'a', 'bcd'}), [1 3]));
%% dizgi-char-cikarma | extractBefore extractAfter extractBetween
assert(strcmp(extractBefore('abcdef', 'cd'), 'ab'));
assert(strcmp(extractAfter('abcdef', 'cd'), 'ef'));
assert(strcmp(char(extractBetween('a[xy]b', '[', ']')), 'xy'));
%% dizgi-char-split-join | split join splitlines
assert(isequal(split('a,b', ','), {'a'; 'b'}));
assert(strcmp(char(join({'a', 'b'}, '-')), 'a-b'));
assert(isequal(splitlines(sprintf('a\nb')), {'a'; 'b'}));
%% dizgi-karakter-sinifi | isstrprop isspace isletter blanks deblank newline
assert(isequal(isstrprop('a1 ', 'digit'), [false true false]));
assert(isequal(isspace('a b'), [false true false]) && isequal(isletter('a1'), [true false]));
assert(numel(blanks(3)) == 3 && strcmp(deblank('ab  '), 'ab'));
assert(double(newline) == 10);
%% dizgi-hucre-donusum | cellstr char iscellstr ischar
assert(iscellstr(cellstr(['ab'; 'cd'])) && ischar('a'));
assert(isequal(char({'a', 'bcd'}), ['a  '; 'bcd']));
%% dizgi-compose | compose
assert(isequal(compose('%d-%s', 3, 'a'), {'3-a'}));
%% dizgi-dizisi-islemleri | string strlength upper
s = string({'bir', 'iki'});
assert(isequal(size(s), [1 2]));
assert(isequal(strlength(s), [3 3]));
assert(isequal(upper(s), string({'BIR', 'IKI'})));
assert(isequal(s(1) + s(2), string('biriki')));
%% dizgi-dizisi-char-toplama | string | string + char birleştirmesi
s = string('bir');
assert(isequal(char(s + 'x'), 'birx'));
%% dizgi-dizisi-sayi | string double
assert(strcmp(char(string(3.5)), '3.5'));
assert(double(string('2.5')) == 2.5);
%% dizgi-dizisi-sorgu | contains replace split join
s = string({'elma', 'armut'});
assert(isequal(contains(s, 'ma'), [true false]));
assert(isequal(replace(s, 'a', 'A'), string({'elmA', 'Armut'})));
assert(isequal(join(s, '-'), string('elma-armut')));
assert(isequal(split(string('a,b'), ','), string({'a'; 'b'})));
