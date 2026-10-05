% Veri analizi ve sayısal yöntemler
%% veri-ozet-istatistik | mean median mode std var
x = [2 4 4 4 5 5 7 9];
assert(mean(x) == 5 && median(x) == 4.5 && mode(x) == 4);
assert(abs(std(x, 1) - 2) < 1e-12);
assert(abs(var([1 2 3 4]) - 5 / 3) < 1e-12);
%% veri-kovaryans | cov corrcoef
R = corrcoef([1 2 3 4], [2 4 6 8]);
assert(abs(R(1, 2) - 1) < 1e-12);
C = cov([1 2 3 4], [2 4 6 8]);
assert(isequal(size(C), [2 2]) && abs(C(1, 2) - 10 / 3) < 1e-12);
%% veri-histcounts | histcounts
[N, kenar] = histcounts([1 2 2 3 5], [0 2 4 6]);
assert(isequal(N, [1 3 1]) && isequal(kenar, [0 2 4 6]));
%% veri-accumarray | accumarray
assert(isequal(accumarray([1; 2; 1], [10; 20; 30]), [40; 20]));
%% veri-hareketli | movmean movmedian movsum
assert(norm(movmean([1 2 3 4 5], 3) - [1.5 2 3 4 4.5]) < 1e-12);
assert(isequal(movsum([1 2 3 4], 2), [1 3 5 7]));
assert(isequal(movmedian([1 9 2 8 3], 3), [5 2 8 3 5.5]));
%% veri-olcekleme | rescale normalize
assert(norm(rescale([1 2 3]) - [0 0.5 1]) < 1e-12);
assert(norm(normalize([1 2 3]) - [-1 0 1]) < 1e-12);
%% veri-eksik-deger | rmmissing fillmissing ismissing
assert(isequal(rmmissing([1 NaN 2]), [1 2]));
assert(isequal(fillmissing([1 NaN 3], 'linear'), [1 2 3]));
assert(isequal(fillmissing([1 NaN 3], 'constant', 0), [1 0 3]));
assert(isequal(ismissing([1 NaN 3]), [false true false]));
%% veri-aykiri | isoutlier
assert(isequal(isoutlier([1 2 2 3 2 100]), [false false false false false true]));
%% veri-sinirlar | bounds
[alt, ust] = bounds([3 1 2]);
assert(alt == 1 && ust == 3);
%% veri-yuzdelik | prctile quantile
assert(abs(median([1 2 3 4]) - 2.5) < 1e-12);
assert(abs(prctile([1 2 3 4 5], 50) - 3) < 1e-12);
assert(abs(quantile([1 2 3 4 5], 0.5) - 3) < 1e-12);
%% veri-turev-integral | diff gradient trapz cumtrapz
assert(isequal(diff([1 4 9]), [3 5]));
assert(isequal(gradient([1 4 9]), [3 4 5]));
assert(trapz([1 2 3]) == 4);
assert(isequal(cumtrapz([1 2 3]), [0 1.5 4]));
%% veri-interpolasyon | interp1 interp2 spline pchip
assert(interp1([1 2 3], [10 20 30], 2.5) == 25);
assert(isnan(interp1([1 2 3], [10 20 30], 4)));
assert(interp1([1 2 3], [10 20 30], 4, 'linear', 'extrap') == 40);
[X, Y] = meshgrid(1:2, 1:2);
assert(abs(interp2(X, Y, [1 2; 3 4], 1.5, 1.5) - 2.5) < 1e-12);
assert(abs(spline([0 1 2 3], [0 1 8 27], 1.5) - 3.375) < 1e-12);
assert(abs(pchip([1 2 3], [1 4 9], 2) - 4) < 1e-12);
%% veri-polinom | polyfit polyval roots conv deconv polyder polyint
p = polyfit([0 1 2], [1 3 7], 2);
assert(norm(p - [1 1 1]) < 1e-10);
assert(polyval([1 1 1], 2) == 7);
assert(norm(sort(roots([1 -3 2])) - [1; 2]) < 1e-12);
assert(isequal(conv([1 1], [1 -1]), [1 0 -1]));
[q, r] = deconv([1 0 -1], [1 1]);
assert(isequal(q, [1 -1]) && all(r == 0));
assert(isequal(polyder([1 2 3]), [2 2]) && isequal(polyint([2 2]), [1 2 0]));
%% veri-fft | fft ifft fftshift
x = [1 2 3 4];
X = fft(x);
assert(norm(X - [10, -2+2i, -2, -2-2i]) < 1e-12);
assert(norm(ifft(X) - x) < 1e-12);
assert(isequal(fftshift([1 2 3 4]), [3 4 1 2]));
%% veri-filtre | filter
assert(isequal(filter(1, [1 -0.5], [1 0 0]), [1 0.5 0.25]));
assert(isequal(filter([1 1] / 2, 1, [2 4 6]), [1 3 5]));
%% sayisal-integral | integral quadgk integral2
assert(abs(integral(@(x) x.^2, 0, 1) - 1 / 3) < 1e-10);
assert(abs(quadgk(@(x) exp(-x.^2), -Inf, Inf) - sqrt(pi)) < 1e-8);
assert(abs(integral2(@(x, y) x .* y, 0, 1, 0, 2) - 1) < 1e-8);
%% sayisal-integral-secenek | integral
assert(norm(integral(@(x) [x, 2 * x], 0, 1, 'ArrayValued', true) - [0.5 1]) < 1e-10);
%% sayisal-kok-ve-enkucuk | fzero fminsearch fminbnd
assert(abs(fzero(@cos, [1 2]) - pi / 2) < 1e-8);
x = fminsearch(@(v) (v(1) - 1)^2 + (v(2) + 2)^2, [0 0]);
assert(norm(x - [1 -2]) < 1e-3);
assert(abs(fminbnd(@(t) (t - 2)^2, 0, 5) - 2) < 1e-4);
%% sayisal-fsolve | fsolve optimset
ayar = optimset('Display', 'off');
x = fsolve(@(v) [v(1) + v(2) - 3; v(1) - v(2) - 1], [0; 0], ayar);
assert(norm(x - [2; 1]) < 1e-6);
%% sayisal-ode45 | ode45 odeset
[t, y] = ode45(@(t, y) -y, [0 1], 1);
assert(abs(y(end) - exp(-1)) < 1e-3 && t(end) == 1);
ayar = odeset('RelTol', 1e-8, 'AbsTol', 1e-10);
[t, y] = ode45(@(t, y) [y(2); -y(1)], [0 pi], [0; 1], ayar);
assert(norm(y(end, :) - [0 -1]) < 1e-5);
%% sayisal-ode-yapi-ve-deval | ode45 deval
cozum = ode45(@(t, y) -y, [0 1], 1);
assert(abs(deval(cozum, 0.5) - exp(-0.5)) < 1e-3);
%% sayisal-ode-digerleri | ode23 ode15s ode23s
[t, y] = ode23(@(t, y) -y, [0 1], 1);
assert(abs(y(end) - exp(-1)) < 1e-2);
[t, y] = ode15s(@(t, y) -50 * (y - cos(t)), [0 1], 0);
assert(abs(y(end) - cos(1)) < 0.05);
[t, y] = ode23s(@(t, y) -y, [0 1], 1);
assert(abs(y(end) - exp(-1)) < 1e-2);
%% sayisal-ode113 | ode113
[t, y] = ode113(@(t, y) -y, [0 1], 1);
assert(abs(y(end) - exp(-1)) < 1e-3);
%% sayisal-lsqnonneg | lsqnonneg
x = lsqnonneg([1 0; 0 1], [1; -1]);
assert(norm(x - [1; 0]) < 1e-10);
