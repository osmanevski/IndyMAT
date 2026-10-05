% İstatistik
%% istat-normal-dagilim | normpdf normcdf norminv
assert(abs(normpdf(0) - 1 / sqrt(2 * pi)) < 1e-12);
assert(abs(normcdf(0) - 0.5) < 1e-12);
assert(abs(norminv(0.975) - 1.959963984540054) < 1e-9);
assert(abs(normpdf(3, 1, 2) - exp(-0.5) / (2 * sqrt(2 * pi))) < 1e-12);
%% istat-t-ki-kare-f | tcdf tinv chi2cdf chi2inv fcdf finv
assert(abs(tcdf(0, 5) - 0.5) < 1e-12);
assert(abs(tinv(0.975, 10) - 2.228138851986274) < 1e-8);
assert(abs(chi2cdf(3, 2) - (1 - exp(-1.5))) < 1e-12);
assert(abs(chi2inv(0.95, 2) + 2 * log(0.05)) < 1e-9);
assert(abs(fcdf(1, 5, 5) - 0.5) < 1e-10);
assert(abs(finv(0.5, 7, 7) - 1) < 1e-9);
%% istat-ayrik-dagilimlar | binopdf binocdf poisspdf poisscdf
assert(abs(binopdf(2, 4, 0.5) - 0.375) < 1e-12);
assert(abs(binocdf(1, 4, 0.5) - 5 / 16) < 1e-12);
assert(abs(poisspdf(0, 2) - exp(-2)) < 1e-12);
assert(abs(poisscdf(1, 2) - 3 * exp(-2)) < 1e-12);
%% istat-surekli-dagilimlar | exppdf expcdf expinv unifpdf unifcdf
assert(abs(exppdf(1, 2) - 0.5 * exp(-0.5)) < 1e-12);
assert(abs(expcdf(2, 2) - (1 - exp(-1))) < 1e-12);
assert(abs(expinv(0.5, 2) - 2 * log(2)) < 1e-12);
assert(abs(unifpdf(3, 2, 6) - 0.25) < 1e-12);
assert(abs(unifcdf(0.25, 0, 1) - 0.25) < 1e-12);
%% istat-gamma-beta | gampdf gamcdf betapdf betacdf
assert(abs(gampdf(1, 1, 1) - exp(-1)) < 1e-12);
assert(abs(gamcdf(2, 1, 2) - (1 - exp(-1))) < 1e-12);
assert(abs(betapdf(0.5, 2, 2) - 1.5) < 1e-12);
assert(abs(betacdf(0.5, 2, 2) - 0.5) < 1e-12);
%% istat-adla-dagilim | pdf cdf icdf
assert(abs(pdf('Normal', 0, 0, 1) - 1 / sqrt(2 * pi)) < 1e-12);
assert(abs(cdf('Normal', 0, 0, 1) - 0.5) < 1e-12);
assert(abs(icdf('Exponential', 0.5, 2) - 2 * log(2)) < 1e-12);
%% istat-bicim-olculeri | skewness kurtosis range
assert(abs(skewness([1 2 3])) < 1e-12);
assert(abs(kurtosis([1 2 3 4 5]) - 1.7) < 1e-12);
assert(range([1 5 3]) == 4);
%% istat-ortalamalar | geomean harmmean
assert(abs(geomean([1 4]) - 2) < 1e-12);
assert(abs(harmmean([1 3]) - 1.5) < 1e-12);
%% istat-standartlastirma | zscore
z = zscore([1 2 3]);
assert(max(abs(z - [-1 0 1])) < 1e-12);
%% istat-yuzdelik | prctile quantile iqr
assert(prctile([1 2 3 4 5], 50) == 3);
assert(abs(quantile([1 2 3 4], 0.5) - 2.5) < 1e-12);
assert(abs(iqr([1 2 3 4]) - 2) < 1e-12);
%% istat-siklik-tablosu | tabulate crosstab
t = tabulate([1 1 2]);
assert(isequal(t(:, 1:2), [1 2; 2 1]) && abs(t(1, 3) - 200 / 3) < 1e-10);
assert(isequal(crosstab([1 1 2 2], [1 2 1 2]), ones(2)));
%% istat-ttest | ttest
[h, p] = ttest([1.1 0.9 1.3 1.2 0.8], 1);
assert(h == 0 && abs(p - 0.5529) < 1e-3);
%% istat-ttest2 | ttest2
[h, p] = ttest2([1 2 3 4 5], [6 7 8 9 10]);
assert(h == 1 && p < 0.01);
%% istat-anova1 | anova1
p = anova1([1 2 11; 2 3 12; 3 4 13; 4 5 14], [], 'off');
assert(isscalar(p) && p < 1e-3);
%% istat-korelasyon | corr
assert(abs(corr([1 2 3 4]', [2 4 6 8]') - 1) < 1e-12);
assert(abs(corr([1 2 3 4]', [1 4 9 20]', 'Type', 'Spearman') - 1) < 1e-12);
%% istat-regress | regress
x = (1:5)';
b = regress(2 * x + 1, [ones(5, 1) x]);
assert(max(abs(b - [1; 2])) < 1e-10);
%% istat-fitlm | fitlm
x = (1:6)';
m = fitlm(x, 2 * x + 1 + [0.1; -0.1; 0.05; -0.05; 0.02; -0.02]);
assert(isa(m, 'LinearModel'));
assert(m.Rsquared.Ordinary > 0.99);
%% istat-dagilim-nesnesi | makedist
pd = makedist('Normal', 'mu', 2, 'sigma', 3);
assert(mean(pd) == 2 && std(pd) == 3);
assert(abs(cdf(pd, 2) - 0.5) < 1e-12);
%% istat-dagilim-uydurma | fitdist normfit
v = [1 2 3 4 5]';
pd = fitdist(v, 'Normal');
assert(abs(pd.mu - 3) < 1e-12 && abs(pd.sigma - std(v)) < 1e-12);
[m, s] = normfit(v);
assert(abs(m - 3) < 1e-12 && abs(s - std(v)) < 1e-12);
%% istat-rastgele-boyut | normrnd exprnd randsample
assert(isequal(size(normrnd(0, 1, 3, 2)), [3 2]));
assert(isequal(size(exprnd(2, 4, 1)), [4 1]));
r = randsample(10, 3);
assert(numel(unique(r)) == 3 && all(r >= 1 & r <= 10));
%% istat-cok-degiskenli-normal | mvnpdf mvnrnd
assert(abs(mvnpdf([0 0]) - 1 / (2 * pi)) < 1e-12);
assert(isequal(size(mvnrnd([0 0], eye(2), 5)), [5 2]));
%% istat-mesafe | pdist squareform pdist2
assert(abs(pdist([0 0; 3 4]) - 5) < 1e-12);
assert(isequal(squareform([1 2 3]), [0 1 2; 1 0 3; 2 3 0]));
assert(abs(pdist2([0 0], [3 4]) - 5) < 1e-12);
%% istat-kmeans | kmeans
idx = kmeans([0 0; 0 1; 10 10; 10 11], 2);
assert(idx(1) == idx(2) && idx(3) == idx(4) && idx(1) ~= idx(3));
%% istat-hiyerarsik-kumeleme | linkage cluster
Z = linkage([0 0; 0 1; 10 10; 10 11]);
assert(isequal(size(Z), [3 3]));
T = cluster(Z, 'maxclust', 2);
assert(T(1) == T(2) && T(3) == T(4) && T(1) ~= T(3));
%% istat-linkage-sutun-vektoru | linkage | sütun vektörü gözlem sayılır, uzaklık vektörü değil
Z = linkage([0; 1; 10; 11]);
assert(isequal(size(Z), [3 3]));
%% istat-pca | pca
[c, s, l] = pca([1 2; 2 4; 3 6]);
assert(abs(abs(c(1, 1)) - 1 / sqrt(5)) < 1e-10);
assert(abs(l(1) - 5) < 1e-10);
%% istat-ecdf | ecdf
[f, x] = ecdf([1 2 3 4]);
assert(numel(f) == 5 && f(1) == 0 && f(end) == 1);
%% istat-knn-siniflandirma | fitcknn predict
mdl = fitcknn([0; 1; 10; 11], [1; 1; 2; 2]);
assert(isequal(predict(mdl, [0.5; 10.5]), [1; 2]));
%% istat-capraz-dogrulama | cvpartition
c = cvpartition(10, 'KFold', 5);
assert(c.NumTestSets == 5);
