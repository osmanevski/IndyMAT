% Matematik ve doğrusal cebir
%% mat-yuvarlama | round fix floor ceil mod rem
assert(round(2.5) == 3 && round(-2.5) == -3);
assert(fix(-2.7) == -2 && floor(-2.7) == -3 && ceil(-2.2) == -2);
assert(mod(-7, 3) == 2 && rem(-7, 3) == -1);
%% mat-yuvarlama-basamak | round
assert(abs(round(2.567, 2) - 2.57) < 1e-12);
assert(round(1234, -2) == 1200);
%% mat-sayi-kurami | factorial nchoosek primes isprime gcd lcm factor
assert(factorial(5) == 120 && nchoosek(5, 2) == 10);
assert(isequal(primes(20), [2 3 5 7 11 13 17 19]));
assert(isprime(97) && ~isprime(91));
assert(gcd(12, 18) == 6 && lcm(4, 6) == 12);
assert(isequal(factor(60), [2 2 3 5]));
%% mat-toplam-tum | sum max min
A = [1 2; 3 4];
assert(sum(A, 'all') == 10);
assert(max(A, [], 'all') == 4 && min(A, [], 'all') == 1);
assert(isequal(sum(A, 2), [3; 7]));
%% mat-nan-atlama | sum mean max
x = [1 NaN 3];
assert(max(x) == 3);
assert(sum(x, 'omitnan') == 4);
assert(mean(x, 'omitnan') == 2);
%% mat-dizi-islemleri | reshape repmat permute circshift flip kron cat
assert(isequal(reshape(1:6, 2, 3), [1 3 5; 2 4 6]));
assert(isequal(repmat([1 2], 2, 2), [1 2 1 2; 1 2 1 2]));
assert(isequal(size(permute(zeros(2, 3, 4), [3 1 2])), [4 2 3]));
assert(isequal(circshift(1:5, 2), [4 5 1 2 3]));
assert(isequal(flip([1 2 3]), [3 2 1]));
assert(isequal(kron([1 2], [1; 1]), [1 2; 1 2]));
assert(isequal(cat(3, 1, 2), reshape([1 2], 1, 1, 2)));
%% mat-kume-islemleri | unique union intersect setdiff ismember sort
[u, i, j] = unique([3 1 3 2]);
assert(isequal(u, [1 2 3]) && isequal(u(j), [3 1 3 2]));
assert(isequal(unique([3 1 3 2], 'stable'), [3 1 2]));
assert(isequal(union([1 2], [2 3]), [1 2 3]));
assert(isequal(intersect([1 2 3], [2 3 4]), [2 3]));
assert(isequal(setdiff([1 2 3], 2), [1 3]));
assert(isequal(ismember([1 5], [1 2 3]), [true false]));
[s, k] = sort([3 1 2], 'descend');
assert(isequal(s, [3 2 1]) && isequal(k, [1 3 2]));
%% mat-dogrusal-cozum | mldivide det inv rank trace norm
A = [4 -2; 1 1]; b = [2; 3];
x = mldivide(A, b);
assert(norm(A * x - b) < 1e-12);
assert(abs(det(A) - 6) < 1e-12);
assert(norm(inv(A) * A - eye(2)) < 1e-12);
assert(rank([1 2; 2 4]) == 1 && trace(A) == 5);
assert(abs(norm([3 4]) - 5) < 1e-12 && norm([1 2; 3 4], 'fro') - sqrt(30) < 1e-12);
%% mat-ayristirmalar | eig svd lu qr chol
A = [2 0; 0 3];
assert(isequal(eig(A), [2; 3]));
[V, D] = eig([2 1; 1 2]);
assert(norm([2 1; 1 2] * V - V * D) < 1e-12);
assert(norm(svd([3 0; 0 4]) - [4; 3]) < 1e-12);
M = [4 3; 6 3];
[L, U, P] = lu(M);
assert(norm(P * M - L * U) < 1e-12);
[Q, R] = qr(M);
assert(norm(Q * R - M) < 1e-12);
C = chol([4 2; 2 3]);
assert(norm(C' * C - [4 2; 2 3]) < 1e-12);
%% mat-matris-fonksiyonlari | expm null pinv
assert(norm(expm(zeros(2)) - eye(2)) < 1e-12);
assert(norm(expm([0 1; 0 0]) - [1 1; 0 1]) < 1e-12);
n = null([1 1; 1 1]);
assert(norm([1 1; 1 1] * n) < 1e-12);
assert(norm(pinv([1; 2]) - [1 2] / 5) < 1e-12);
%% mat-vecnorm | vecnorm
assert(norm(vecnorm([3 0; 4 5]) - [5 5]) < 1e-12);
assert(norm(vecnorm([3 4; 6 8], 2, 2) - [5; 10]) < 1e-12);
%% mat-vektor-carpimlari | cross dot
assert(isequal(cross([1 0 0], [0 1 0]), [0 0 1]));
assert(dot([1 2 3], [4 5 6]) == 32);
%% mat-seyrek | sparse full nnz speye
S = sparse([1 2], [1 2], [5 7], 3, 3);
assert(nnz(S) == 2 && issparse(S));
assert(isequal(full(S(2, 2)), 7));
assert(isequal(full(speye(2)), eye(2)));
%% mat-taban-donusum | dec2bin bin2dec dec2hex hex2dec bitand bitshift
assert(strcmp(dec2bin(5), '101') && bin2dec('101') == 5);
assert(strcmp(dec2hex(255), 'FF') && hex2dec('FF') == 255);
assert(bitand(12, 10) == 8 && bitshift(1, 3) == 8);
%% mat-rastgele-tohum | rng rand randi randperm
rng(42); a = rand(1, 3);
rng(42); b = rand(1, 3);
assert(isequal(a, b));
r = randi(6, 1, 50);
assert(all(r >= 1 & r <= 6));
assert(isequal(sort(randperm(5)), 1:5));
%% mat-rastgele-matlab-dizisi | rng | MATLAB twister dizisiyle aynı sayılar
rng(0);
assert(abs(rand() - 0.814723686393179) < 1e-12);
