% Dosya ve veri G/Ç
%% dosyay-matris-15-basamak | writematrix | MATLAB metin çıktısı 15 anlamlı basamak kullanır
dosya = [tempname() '.csv'];
A = [0.1 pi 1/3 realmin; -0.25 eps 1e100 -1e-100; 1 NaN Inf -Inf];
writematrix(A, dosya); metin = fileread(dosya); delete(dosya);
beklenen = sprintf(['0.1,3.14159265358979,0.333333333333333,2.2250738585072e-308\n' ...
    '-0.25,2.22044604925031e-16,1e+100,-1e-100\n1,NaN,Inf,-Inf\n']);
assert(strcmp(metin, beklenen));
%% dosyay-matris-toleransli-tur | writematrix readmatrix | metin turu kayıpsız değildir
dosya = [tempname() '.csv'];
A = [0.1 pi 1/3 realmin; -0.25 eps 1e100 -1e-100];
writematrix(A, dosya); B = readmatrix(dosya); delete(dosya);
assert(~isequal(A, B) && isequal(size(A), size(B)));
assert(all(abs((B(:) - A(:)) ./ A(:)) < 5e-15));
%% dosyay-matris-ozel-ayrac | writematrix readmatrix
dosya = [tempname() '.txt'];
writematrix([1 2; 3 4], dosya, 'Delimiter', ';');
A = readmatrix(dosya, 'Delimiter', ';'); delete(dosya);
assert(isequal(A, [1 2; 3 4]));
%% dosyay-matris-baslik | readmatrix
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, 'ad,x,y\nsatir,1,2\nsatir,3,4\n'); fclose(fid);
A = readmatrix(dosya, 'Delimiter', ',', 'NumHeaderLines', 1); delete(dosya);
assert(isequaln(A, [NaN 1 2; NaN 3 4]));
%% dosyay-matris-eksik-metin | readmatrix
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, '1,,x\n4,5,6\n'); fclose(fid);
A = readmatrix(dosya, 'Delimiter', ','); delete(dosya);
assert(isequaln(A, [1 NaN NaN; 4 5 6]));
%% dosyay-matris-ozel-deger | writematrix readmatrix
dosya = [tempname() '.dat'];
A = [NaN Inf -Inf; 0 -1 2]; writematrix(A, dosya);
B = readmatrix(dosya); delete(dosya);
assert(isequaln(A, B));
%% dosyay-matris-ekle | writematrix readmatrix
dosya = [tempname() '.csv'];
writematrix([1 2], dosya); writematrix([3 4], dosya, 'WriteMode', 'append');
A = readmatrix(dosya); delete(dosya);
assert(isequal(A, [1 2; 3 4]));
%% dosyay-matris-uzerine-yaz | writematrix readmatrix
dosya = [tempname() '.csv'];
writematrix([1 2], dosya); writematrix([9 8], dosya);
A = readmatrix(dosya); delete(dosya);
assert(isequal(A, [9 8]));
%% dosyay-hucre-karisik | writecell readcell
dosya = [tempname() '.csv'];
C = {'ad', 'deger'; 'bir', 1; 'iki', 2.5};
writecell(C, dosya); D = readcell(dosya); delete(dosya);
assert(isequal(C, D));
%% dosyay-hucre-15-basamak-mantiksal | writecell readcell
dosya = [tempname() '.csv'];
writecell({'a', 1.5, true; 'b c', pi, 'x,y'}, dosya);
metin = fileread(dosya); C = readcell(dosya); delete(dosya);
assert(strcmp(metin, sprintf('a,1.5,1\nb c,3.14159265358979,"x,y"\n')));
assert(isa(C{1,3}, 'double') && isequal(C{1,3}, 1));
assert(strcmp(C{2,1}, 'b c') && strcmp(C{2,3}, 'x,y') && abs(C{2,2} - pi) < 5e-15);
%% dosyay-hucre-ozel-ayrac | writecell readcell
dosya = [tempname() '.dat'];
C = {'a', 1; 'b', -2}; writecell(C, dosya, 'Delimiter', '|');
D = readcell(dosya, 'Delimiter', '|'); delete(dosya);
assert(isequal(C, D));
%% dosyay-hucre-baslik | readcell
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, 'ilk satir\nikinci satir\na,1\nb,2\n'); fclose(fid);
C = readcell(dosya, 'Delimiter', ',', 'NumHeaderLines', 2); delete(dosya);
assert(isequal(C, {'a', 1; 'b', 2}));
%% dosyay-hucre-alinti | writecell readcell
dosya = [tempname() '.csv'];
C = {'a,b', 1; 'c"d', 2}; writecell(C, dosya);
D = readcell(dosya, 'Delimiter', ','); delete(dosya);
assert(isequal(C, D));
%% dosyay-hucre-ekle | writecell readcell
dosya = [tempname() '.csv'];
writecell({'a', 1}, dosya); writecell({'b', 2}, dosya, 'WriteMode', 'append');
C = readcell(dosya); delete(dosya);
assert(isequal(C, {'a', 1; 'b', 2}));
%% dosyay-hucre-eksik | readcell
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, 'a,,3\n'); fclose(fid);
C = readcell(dosya, 'Delimiter', ','); delete(dosya);
assert(strcmp(C{1,1}, 'a') && ismissing(C{1,2}) && isequal(C{1,3}, 3));
%% dosyay-satir-sonlandirilmamis | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fprintf(fid, 'bir\niki'); fclose(fid);
S = readlines(dosya); delete(dosya);
assert(isequal(size(S), [2 1]) && strcmp(char(S(1)), 'bir') && strcmp(char(S(2)), 'iki'));
%% dosyay-satir-sonlandirilmis | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fprintf(fid, 'bir\niki\n'); fclose(fid);
S = readlines(dosya); delete(dosya);
assert(isequal(size(S), [3 1]) && strcmp(char(S(2)), 'iki') && isempty(char(S(3))));
%% dosyay-satir-bos | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fprintf(fid, 'bir\n\niki\n'); fclose(fid);
S = readlines(dosya); delete(dosya);
assert(isequal(size(S), [4 1]) && strcmp(char(S(1)), 'bir') && isempty(char(S(2))) && strcmp(char(S(3)), 'iki') && isempty(char(S(4))));
%% dosyay-satir-bos-dosya | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fclose(fid);
S = readlines(dosya); delete(dosya);
assert(isa(S, 'string') && isequal(size(S), [1 1]) && isempty(char(S(1))));
%% dosyay-satir-bos-atla | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fprintf(fid, 'bir\n\niki\n'); fclose(fid);
S = readlines(dosya, 'EmptyLineRule', 'skip');
R = readlines(dosya, 'EmptyLineRule', 'read'); delete(dosya);
assert(isequal(size(S), [2 1]) && strcmp(char(S(1)), 'bir') && strcmp(char(S(2)), 'iki'));
assert(isequal(size(R), [4 1]) && isempty(char(R(2))) && isempty(char(R(4))));
%% dosyay-satir-bos-dosya-atla | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'w'); fclose(fid);
S = readlines(dosya, 'EmptyLineRule', 'skip'); delete(dosya);
assert(isa(S, 'string') && isempty(S) && isequal(size(S), [0 1]));
%% dosyay-satir-yaz-char | writelines readlines
dosya = [tempname() '.txt'];
writelines('tek satir', dosya); S = readlines(dosya); metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('tek satir\n')));
assert(isequal(size(S), [2 1]) && strcmp(char(S(1)), 'tek satir') && isempty(char(S(2))));
%% dosyay-satir-yaz-cellstr | writelines
dosya = [tempname() '.txt'];
writelines({'bir'; 'iki'}, dosya); metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('bir\niki\n')));
%% dosyay-satir-yaz-string | writelines readlines
dosya = [tempname() '.txt'];
writelines(string({'alfa'; 'beta'}), dosya); S = readlines(dosya); delete(dosya);
assert(isequal(size(S), [3 1]) && strcmp(char(S(1)), 'alfa') && strcmp(char(S(2)), 'beta') && isempty(char(S(3))));
%% dosyay-satir-ekle | writelines readlines
dosya = [tempname() '.txt'];
writelines({'a', 'b'}, dosya); writelines(string({'x'; 'y'}), dosya, 'WriteMode', 'append');
S = readlines(dosya); metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('a\nb\nx\ny\n')));
assert(isequal(size(S), [5 1]) && strcmp(char(S(3)), 'x') && strcmp(char(S(4)), 'y') && isempty(char(S(5))));
%% dosyay-satir-uzerine-yaz | writelines readlines
dosya = [tempname() '.txt'];
writelines('eski', dosya); writelines('yeni', dosya);
S = readlines(dosya); metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('yeni\n')));
assert(isequal(size(S), [2 1]) && strcmp(char(S(1)), 'yeni') && isempty(char(S(2))));
%% dosyay-matris-single | writematrix
dosya = [tempname() '.csv'];
writematrix([single(pi) single(1/3) single(1e10) -0 NaN Inf], dosya);
metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('3.141593,0.3333333,1e+10,-0,NaN,Inf\n')));
%% dosyay-matris-tamsayi | writematrix
dosya = [tempname() '.csv'];
writematrix(int32([1 2; 3 4]), dosya);
metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('1,2\n3,4\n')));
%% dosyay-hucre-single-tamsayi | writecell
dosya = [tempname() '.csv'];
writecell({single(pi), int8(5), uint16(7)}, dosya);
metin = fileread(dosya); delete(dosya);
assert(strcmp(metin, sprintf('3.141593,5,7\n')));
%% dosyay-hucre-bom | readcell
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'wb');
fwrite(fid, [uint8([239 187 191]) unicode2native(sprintf('İzmir,1\nAnkara,2\n'), 'UTF-8')], 'uint8'); fclose(fid);
C = readcell(dosya); delete(dosya);
assert(isequal(C, {'İzmir', 1; 'Ankara', 2}));
%% dosyay-matris-bom | readmatrix
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'wb'); fwrite(fid, [uint8([239 187 191]) uint8(sprintf('1,2\n3,4\n'))], 'uint8'); fclose(fid);
A = readmatrix(dosya); delete(dosya);
assert(isequal(A, [1 2; 3 4]));
%% dosyay-satir-bom | readlines
dosya = [tempname() '.txt'];
fid = fopen(dosya, 'wb');
fwrite(fid, [uint8([239 187 191]) unicode2native(sprintf('İzmir\nAnkara\n'), 'UTF-8')], 'uint8'); fclose(fid);
S = readlines(dosya); delete(dosya);
assert(isequal(size(S), [3 1]) && strcmp(char(S(1)), 'İzmir') && strcmp(char(S(2)), 'Ankara') && isempty(char(S(3))));
%% dosyay-hucre-alintili-ayrac-algila | readcell
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, '"a,b";2\n"x,y";3\n'); fclose(fid);
C = readcell(dosya); delete(dosya);
assert(isequal(C, {'a,b', 2; 'x,y', 3}));
%% dosyay-hucre-noktali-virgul-algila | readcell
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, 'a;1;x\nb;2;y\n'); fclose(fid);
C = readcell(dosya); delete(dosya);
assert(isequal(C, {'a', 1, 'x'; 'b', 2, 'y'}));
%% dosyay-matris-tab-algila | readmatrix
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, '1\t2\n3\t4\n'); fclose(fid);
A = readmatrix(dosya); delete(dosya);
assert(isequal(A, [1 2; 3 4]));
%% dosyay-matris-seyrek-reddet | writematrix
dosya = [tempname() '.csv']; hata = false;
try
    writematrix(sparse([1 0; 0 2]), dosya);
catch
    hata = true;
end
if exist(dosya, 'file') == 2, delete(dosya); end
assert(hata);
%% dosyay-ayrac-virgul-onceligi | readcell readmatrix | R2025b virgülü seçer
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, '1,23;4,56\n7,89;0,12\n'); fclose(fid);
C = readcell(dosya); A = readmatrix(dosya); delete(dosya);
assert(isequal(size(C), [2 3]) && isa(C{1,1}, 'double') && isequal(C{1,1}, 1));
assert(isequaln(A, [1 NaN 56; 7 NaN 12]));
%% dosyay-alinti-oncesi-bosluk | readcell | baştaki boşluk alıntılı alan başlatmaz
dosya = [tempname() '.csv'];
fid = fopen(dosya, 'w'); fprintf(fid, ' "a,b",2\n "x,y",3\n'); fclose(fid);
C = readcell(dosya); delete(dosya);
assert(isequal(size(C), [2 3]) && strcmp(C{1,1}, 'a'));
%% dosyay-satir-char-matris-reddet | writelines
dosya = [tempname() '.txt']; hata = false;
try
    writelines(['a '; 'bb'], dosya);
catch
    hata = true;
end
if exist(dosya, 'file') == 2, delete(dosya); end
assert(hata);
