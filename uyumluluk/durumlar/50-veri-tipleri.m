% Veri tipleri ve kapsayıcılar
%% tip-tamsayi-doygunluk | int8 uint8 intmax
assert(int8(127) + int8(1) == 127);
assert(uint8(3) / uint8(2) == 2);
assert(isa(int8(1) + 1, 'int8'));
assert(uint8(5) - uint8(9) == 0);
assert(intmax('int32') == 2147483647);
%% tip-donusum | cast typecast double single logical
assert(isa(cast(3, 'uint16'), 'uint16'));
assert(isequal(typecast(uint8([1 0 0 0]), 'uint32'), uint32(1)));
assert(isa(single(1) + 1, 'single'));
assert(isa(double(single(1)), 'double') && logical(1));
assert(islogical(true & 1) && isnumeric(5) && isfloat(5) && ~isinteger(5));
%% tip-hucre | cell num2cell cell2mat iscell
c = num2cell([1 2; 3 4]);
assert(iscell(c) && isequal(size(c), [2 2]) && isequal(cell2mat(c), [1 2; 3 4]));
assert(isequal(size(cell(2, 3)), [2 3]));
%% tip-yapi | struct setfield getfield rmfield struct2cell cell2struct orderfields
s = struct('b', 2, 'a', 1);
s = setfield(s, 'c', 3);
assert(getfield(s, 'c') == 3);
assert(isequal(fieldnames(rmfield(s, 'c')), {'b'; 'a'}));
assert(isequal(struct2cell(s), {2; 1; 3}));
t = cell2struct({1; 'x'}, {'n', 'ad'}, 1);
assert(t.n == 1 && strcmp(t.ad, 'x'));
assert(isequal(fieldnames(orderfields(s)), {'a'; 'b'; 'c'}));
ad = 'dinamik'; s.(ad) = 9;
assert(s.dinamik == 9);
%% tablo-olusturma | table height width istable
T = table([1; 2; 3], {'a'; 'b'; 'c'}, 'VariableNames', {'sayi', 'ad'});
assert(istable(T) && height(T) == 3 && width(T) == 2);
assert(isequal(T.sayi, [1; 2; 3]));
assert(isequal(T.Properties.VariableNames, {'sayi', 'ad'}));
assert(isequal(size(T), [3 2]));
%% tablo-indeksleme | table
T = table([1; 2; 3], [10; 20; 30], 'VariableNames', {'a', 'b'});
S = T(T.a > 1, :);
assert(isequal(S.b, [20; 30]));
assert(isequal(T{2, 'b'}, 20));
assert(isequal(T{:, {'a', 'b'}}, [1 10; 2 20; 3 30]));
T.c = T.a + T.b;
assert(isequal(T.c, [11; 22; 33]) && width(T) == 3);
%% tablo-donusum | array2table table2array struct2table table2struct cell2table table2cell
T = array2table([1 2; 3 4], 'VariableNames', {'x', 'y'});
assert(isequal(table2array(T), [1 2; 3 4]));
U = struct2table(struct('p', {1; 2}, 'q', {3; 4}));
assert(isequal(U.q, [3; 4]));
s = table2struct(T);
assert(isequal(size(s), [2 1]) && s(2).y == 4);
V = cell2table({1, 'a'; 2, 'b'}, 'VariableNames', {'n', 'h'});
assert(isequal(V.n, [1; 2]));
assert(isequal(table2cell(V), {1, 'a'; 2, 'b'}));
%% tablo-siralama-birlestirme | sortrows innerjoin
T = table([3; 1; 2], {'c'; 'a'; 'b'}, 'VariableNames', {'k', 'h'});
S = sortrows(T, 'k');
assert(isequal(S.k, [1; 2; 3]) && isequal(S.h, {'a'; 'b'; 'c'}));
R = table([1; 2], [100; 200], 'VariableNames', {'k', 'v'});
J = innerjoin(T, R);
assert(isequal(sort(J.k), [1; 2]) && height(J) == 2);
%% tablo-degisken-duzenleme | addvars removevars renamevars
T = table([1; 2], 'VariableNames', {'a'});
T = addvars(T, [5; 6], 'NewVariableNames', 'b');
assert(isequal(T.b, [5; 6]));
T = renamevars(T, 'b', 'c');
assert(isequal(T.Properties.VariableNames, {'a', 'c'}));
T = removevars(T, 'a');
assert(width(T) == 1);
%% tablo-grup-ozeti | groupsummary
T = table({'x'; 'y'; 'x'}, [1; 2; 3], 'VariableNames', {'g', 'v'});
G = groupsummary(T, 'g', 'mean', 'v');
assert(isequal(G.mean_v, [2; 2]) && isequal(G.GroupCount, [2; 1]));
%% tablo-varfun | varfun
T = table([1; 2], [3; 5], 'VariableNames', {'a', 'b'});
M = varfun(@mean, T);
assert(isequal(M.mean_a, 1.5) && isequal(M.mean_b, 4));
%% tablo-bas-son | head tail
T = table((1:10)', 'VariableNames', {'n'});
assert(height(head(T, 3)) == 3);
S = tail(T, 2);
assert(isequal(S.n, [9; 10]));
%% tablo-dosya | writetable readtable
dosya = [tempname() '.csv'];
T = table([1; 2], [0.5; 1.5], 'VariableNames', {'n', 'x'});
writetable(T, dosya);
R = readtable(dosya);
delete(dosya);
assert(isequal(R.n, [1; 2]) && isequal(R.x, [0.5; 1.5]));
assert(isequal(R.Properties.VariableNames, {'n', 'x'}));
%% kategorik | categorical categories countcats iscategorical
c = categorical({'a', 'b', 'a', 'c'});
assert(iscategorical(c));
assert(isequal(categories(c), {'a'; 'b'; 'c'}));
assert(isequal(countcats(c), [2 1 1]));
assert(isequal(c == 'a', [true false true false]));
%% eksik-deger-tipi | missing ismissing
m = missing;
assert(ismissing(m));
