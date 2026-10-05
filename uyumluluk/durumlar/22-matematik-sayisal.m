% Matematik ve doğrusal cebir

%% mats-lsqminnorm-underdetermined | lsqminnorm | Minimum norm, not a basic QR solution
x = lsqminnorm([1 1],[2 4]);
assert(norm(x-[1 2;1 2],'fro') < 1e-12);

%% mats-lsqminnorm-rank-deficient | lsqminnorm | Rank-one inconsistent least squares
A = [1 2 3;2 4 6;3 6 9]; B = [1 2;0 1;2 0];
x = lsqminnorm(A,B); reference = [1;2;3]*([1 2 3]*B)/196;
assert(norm(x-reference,'fro') < 1e-12);

%% mats-lsqminnorm-tall | lsqminnorm | Full column rank and multiple right-hand sides
A = [1 0;0 1;1 1]; B = [1 2;2 0;4 1]; x = lsqminnorm(A,B);
assert(norm(A'*(A*x-B),'fro') < 1e-12);

%% mats-lsqminnorm-tolerance | lsqminnorm | Explicit absolute QR threshold
A = diag([2 1e-9]); x = lsqminnorm(A,[4;1],1e-7);
assert(norm(x-[2;0]) < 1e-12);
y = lsqminnorm(A,[4;1],1e-12); assert(norm(y-[2;1e9])/1e9 < 1e-12);

%% mats-lsqminnorm-complex | lsqminnorm | Complex adjoints and minimum norm
A = [1 1i]; x = lsqminnorm(A,2);
assert(norm(x-[1;-1i]) < 1e-12 && norm(A*x-2) < 1e-12);

%% mats-lsqminnorm-zero | lsqminnorm | Rank zero, empty and single precision
x = lsqminnorm(zeros(2,3),[1;2]); assert(isequal(x,zeros(3,1)));
x = lsqminnorm(zeros(0,3),zeros(0,2)); assert(isequal(size(x),[3 2]));
x = lsqminnorm(single([1 1]),single(2)); assert(isa(x,'single') && norm(double(x)-[1;1]) < 1e-6);
