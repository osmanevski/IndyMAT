% Dil ve programlama
%% dil-ortuk-genisleme | :ortuk-genisleme
assert(isequal([1;2;3] + [10 20], [11 21; 12 22; 13 23]));
%% dil-mantiksal-indeks | find any all
A = magic(4);
assert(isequal(A(A > 14), [16; 15]));
assert(isequal(find([0 3 0 5]), [2 4]));
assert(any([0 0 1]) && ~all([1 0 1]));
B = 1:5; B(end + 1) = 6; B([1 end]) = [];
assert(isequal(B, 2:5));
%% dil-hucre-ve-yapi | cellfun arrayfun struct fieldnames isfield
c = cellfun(@(s) numel(s), {'a', 'bcd', ''});
assert(isequal(c, [1 3 0]));
u = cellfun(@(s) [s '!'], {'a', 'b'}, 'UniformOutput', false);
assert(isequal(u, {'a!', 'b!'}));
s = struct('ad', {'x', 'y'}, 'deger', {1, 2});
assert(isequal(size(s), [1 2]));
assert(isequal([s.deger], [1 2]));
assert(isequal(fieldnames(s), {'ad'; 'deger'}));
assert(isfield(s, 'ad') && ~isfield(s, 'yok'));
assert(isequal(arrayfun(@(k) k^2, 1:4), [1 4 9 16]));
%% dil-hata-yakalama | error
try
  error('Paket:kimlik', 'Deger %d gecersiz', 7);
catch err
  assert(strcmp(err.identifier, 'Paket:kimlik'));
  assert(strcmp(err.message, 'Deger 7 gecersiz'));
end
%% dil-mexception-nesnesi | MException throw
ME = MException('Modul:hata', 'Kod %d', 3);
yakalandi = false;
try
  throw(ME);
catch e
  yakalandi = strcmp(e.identifier, 'Modul:hata') && strcmp(e.message, 'Kod 3');
end
assert(yakalandi);
assert(isa(ME, 'MException'));
%% dil-inputparser | inputParser addRequired addParameter parse
p = inputParser;
addRequired(p, 'x');
addParameter(p, 'Olcek', 1);
parse(p, 5, 'Olcek', 3);
assert(p.Results.x == 5 && p.Results.Olcek == 3);
%% dil-arguments-blogu | :arguments
assert(uy_argumanlar(2, 3) == 6);
assert(uy_argumanlar(2) == 20);
%% dil-arguments-dogrulama | :arguments
hata = false;
try
  uy_argumanlar([1 2 3], 2);
catch
  hata = true;
end
assert(hata);
%% dil-classdef-deger | :classdef
p = uy_nokta(3, 4);
assert(p.norm() == 5);
q = p.olcekle(2);
assert(q.x == 6 && p.x == 3);
b = uy_nokta.birim();
assert(b.x == 1 && isa(b, 'uy_nokta'));
%% dil-classdef-handle | :classdef
s = uy_sayac();
s.artir(); s.artir();
t = s; t.artir();
assert(s.deger == 3);
assert(isa(s, 'handle'));
%% dil-classdef-enumeration | :enumeration
assert(uy_renk.Kirmizi ~= uy_renk.Mavi);
assert(strcmp(char(uy_renk.Mavi), 'Mavi'));
%% dil-fonksiyon-tutamaci | str2func feval nargin
f = @(x, y) x + 2 * y;
assert(f(1, 2) == 5);
g = str2func('@(t) t.^2');
assert(g(3) == 9);
assert(nargin(f) == 2);
assert(feval(@max, [1 5 2]) == 5);
%% dil-func2str-bicimi | func2str
f = @(x,y) x+2*y;
assert(strcmp(func2str(f), '@(x,y)x+2*y'));
%% dil-degisken-cikti | nargout deal
[a, b] = deal(1, 2);
assert(a == 1 && b == 2);
[q, r] = deal(7);
assert(q == 7 && r == 7);
[m, n, o] = uy_cikti('a', 'b');
assert(isequal([m n o], [2 4 6]));
assert(nargout(@uy_cikti) == -1);
%% dil-oncleanup | onCleanup
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fclose(fid);
temizle = onCleanup(@() delete(dosya));
assert(exist(dosya, 'file') == 2);
clear temizle
assert(exist(dosya, 'file') == 0);
%% dil-cift-tirnak-string | :cift-tirnak-string
s = "abc";
assert(isa(s, 'string'));
assert(isequal(size(s), [1 1]));
%% dil-string-birlestirme | :cift-tirnak-string
assert(isequal(char("abc" + "def"), 'abcdef'));
%% dil-containers-map | containers.Map isKey keys values
m = containers.Map();
m('a') = 1; m('b') = 2;
assert(isKey(m, 'a') && ~isKey(m, 'z'));
assert(isequal(keys(m), {'a', 'b'}));
assert(isequal(values(m), {1, 2}));
assert(m.Count == 2);
k = containers.Map('KeyType', 'double', 'ValueType', 'any');
k(3) = 'uc';
assert(strcmp(k(3), 'uc'));
%% dil-dictionary | dictionary
d = dictionary(["a" "b"], [1 2]);
assert(d("b") == 2);
%% dil-validateattributes | validateattributes
validateattributes(5, {'numeric'}, {'positive', 'scalar'});
hata = false;
try
  validateattributes(-1, {'numeric'}, {'positive'});
catch
  hata = true;
end
assert(hata);
%% dil-mustbe-dogrulayicilar | mustBePositive mustBeNumeric mustBeMember
mustBePositive(3); mustBeNumeric(1); mustBeMember('a', {'a', 'b'});
hata = false;
try
  mustBePositive(-1);
catch
  hata = true;
end
assert(hata);
%% dil-evalc | evalc eval
assert(strcmp(strtrim(evalc('disp(42)')), '42'));
eval('z = 3;');
assert(z == 3);
%% dil-zaman-olcumu | tic toc
t0 = tic; gecen = toc(t0);
assert(gecen >= 0 && gecen < 5);
%% dil-timer | timer
t = timer('TimerFcn', @(varargin) 0, 'StartDelay', 0.01);
start(t); wait(t); delete(t);
