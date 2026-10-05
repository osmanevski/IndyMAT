% Dosya ve veri G/Ç
%% dosya-metin-okuma-yazma | fopen fprintf fgetl fclose feof fileread
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fprintf(fid, 'bir\niki %d\n', 2); fclose(fid);
fid = fopen(dosya, 'r'); a = fgetl(fid); b = fgetl(fid); c = fgetl(fid); son = feof(fid); fclose(fid);
metin = fileread(dosya); delete(dosya);
assert(strcmp(a, 'bir') && strcmp(b, 'iki 2') && isequal(c, -1) && son);
assert(strcmp(metin, sprintf('bir\niki 2\n')));
%% dosya-ikili | fwrite fread fseek ftell
dosya = [tempname() '.bin'];
fid = fopen(dosya, 'w'); fwrite(fid, [1 2 3 4], 'uint16'); fclose(fid);
fid = fopen(dosya, 'r'); fseek(fid, 2, 'bof'); konum = ftell(fid); v = fread(fid, 2, 'uint16'); fclose(fid);
delete(dosya);
assert(konum == 2 && isequal(v, [2; 3]));
%% dosya-mat | save load
dosya = [tempname() '.mat'];
x = magic(3); ad = 'deneme'; s = struct('a', {1, 2});
save(dosya, 'x', 'ad', 's');
y = load(dosya); delete(dosya);
assert(isequal(y.x, magic(3)) && strcmp(y.ad, 'deneme') && isequal(y.s, s));
%% dosya-matris-csv | writematrix readmatrix
dosya = [tempname() '.csv'];
writematrix([1 2; 3 4.5], dosya);
M = readmatrix(dosya); delete(dosya);
assert(isequal(M, [1 2; 3 4.5]));
%% dosya-klasik-csv | csvwrite csvread dlmwrite dlmread
dosya = [tempname() '.csv'];
csvwrite(dosya, [1 2; 3 4]);
assert(isequal(csvread(dosya), [1 2; 3 4]));
dlmwrite(dosya, [5 6; 7 8], ';');
assert(isequal(dlmread(dosya, ';'), [5 6; 7 8]));
delete(dosya);
%% dosya-textscan | textscan
c = textscan(sprintf('a 1\nb 2'), '%s %f');
assert(isequal(c{1}, {'a'; 'b'}) && isequal(c{2}, [1; 2]));
%% dosya-satirlar | readlines writelines
dosya = [tempname() '.txt'];
writelines({'bir', 'iki'}, dosya);
s = readlines(dosya); delete(dosya);
assert(numel(s) >= 2 && strcmp(char(s(1)), 'bir') && strcmp(char(s(2)), 'iki'));
%% dosya-hucre | writecell readcell
dosya = [tempname() '.csv'];
writecell({'a', 1; 'b', 2}, dosya);
c = readcell(dosya); delete(dosya);
assert(isequal(c, {'a', 1; 'b', 2}));
%% dosya-json | jsonencode jsondecode
assert(strcmp(jsonencode(struct('a', 1, 'b', [1 2 3])), '{"a":1,"b":[1,2,3]}'));
v = jsondecode('{"x":[1,2],"ad":"y","ic":{"z":true}}');
assert(isequal(v.x, [1; 2]) && strcmp(v.ad, 'y') && v.ic.z == true);
%% dosya-yol | fullfile fileparts filesep tempdir
[klasor, ad, uzanti] = fileparts('/a/b/c.txt');
assert(strcmp(klasor, '/a/b') && strcmp(ad, 'c') && strcmp(uzanti, '.txt'));
assert(strcmp(fullfile('a', 'b', 'c.m'), ['a' filesep 'b' filesep 'c.m']));
assert(exist(tempdir(), 'dir') == 7);
%% dosya-klasor-islemleri | mkdir rmdir dir isfile isfolder copyfile movefile delete
kok = tempname(); mkdir(kok);
assert(isfolder(kok));
fid = fopen(fullfile(kok, 'a.txt'), 'w'); fclose(fid);
copyfile(fullfile(kok, 'a.txt'), fullfile(kok, 'b.txt'));
movefile(fullfile(kok, 'b.txt'), fullfile(kok, 'c.txt'));
liste = dir(fullfile(kok, '*.txt'));
assert(isequal(sort({liste.name}), {'a.txt', 'c.txt'}));
assert(isfile(fullfile(kok, 'c.txt')) && ~isfile(fullfile(kok, 'b.txt')));
delete(fullfile(kok, 'a.txt')); delete(fullfile(kok, 'c.txt')); rmdir(kok);
assert(~isfolder(kok));
%% dosya-goruntu | imwrite imread
dosya = [tempname() '.png'];
I = uint8(reshape(0:15, 4, 4) * 16);
imwrite(I, dosya);
J = imread(dosya); delete(dosya);
assert(isequal(I, J));
%% dosya-ses | audiowrite audioread
dosya = [tempname() '.wav'];
y = sin(2 * pi * 440 * (0:799)' / 8000) * 0.5;
audiowrite(dosya, y, 8000);
[z, fs] = audioread(dosya); delete(dosya);
assert(fs == 8000 && numel(z) == 800 && max(abs(z - y)) < 1e-3);
%% dosya-excel | writetable readtable
dosya = [tempname() '.xlsx'];
writetable(table([1; 2], 'VariableNames', {'n'}), dosya);
R = readtable(dosya); delete(dosya);
assert(isequal(R.n, [1; 2]));
