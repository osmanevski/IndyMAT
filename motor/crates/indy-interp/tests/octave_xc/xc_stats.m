1;
function show(name, v)
  printf('%s: size=%s |', name, mat2str(size(v)));
  printf(' %.9g', v(:)); printf('\n');
end
x = [3 1 4 1 5 9 2 6]; A = [1 5 3; 4 2 8; 7 7 1; 2 9 4];
for m = 1:9
  show(sprintf('q m%d', m), quantile(x, [0 0.1 0.25 0.5 0.9 1], 2, m));
  show(sprintf('qA m%d', m), quantile(A, [0.2 0.6], 1, m));
end
show('q def', quantile(x, [0.3 0.7]));
show('q col', quantile(x', [0.3 0.7]));
show('q A', quantile(A, 0.5));
show('q A2', quantile(A, [0.25 0.75], 2));
show('q nan', quantile([1 NaN 3 4 NaN], [0.25 0.5]));
show('q def p', quantile(x));
show('pr', prctile(x, [10 50 90]));
show('pr A', prctile(A, 50));
show('pr A2', prctile(A, [25 75], 2));
show('pr def', prctile(A));
show('iqr', iqr(x)); show('iqr A', iqr(A)); show('iqr A2', iqr(A,2));
show('cov v', cov(x)); show('cov A', cov(A)); show('cov A1', cov(A,1)); show('cov xy', cov([1 2 3 4],[2 4 7 8]));
show('cov nan', cov([1 2 NaN; 3 5 6; 7 1 2; 4 4 4]));
show('cc A', corrcoef(A)); show('cc xy', corrcoef([1 2 3 4],[2 4 7 8]));
[R,P] = corrcoef(A); show('cc P', P);
[R,P,L,U] = corrcoef([1 2 3 4 5 6]', [2 1 4 3 7 5]'); show('cc R', R); show('cc P2', P); show('cc L', L); show('cc U', U);
show('cc rows', corrcoef([1 2 NaN; 3 5 6; 7 1 2; 4 4 4; 2 2 9], 'Rows', 'complete'));
show('cc pair', corrcoef([1 2 NaN; 3 5 6; 7 1 2; 4 4 4; 2 2 9], 'Rows', 'pairwise'));
% existing stats core
show('mean dim', mean(A, 2)); show('mean all', mean(A(:))); show('mean nan', mean([1 NaN 3], 'omitnan'));
show('median A', median(A)); show('median A2', median(A,2)); show('median nan', median([1 NaN 3 4], 'omitnan'));
show('mode', mode([1 2 2 3 3])); show('var', var(A)); show('var 1', var(A,1)); show('var w', var(A,[1 2 3 4]')); show('var 2', var(A,0,2));
show('std', std(A)); show('std 1', std(A,1,2)); show('var omit', var([1 NaN 3 5],'omitnan'));
show('cumsum', cumsum(A,2)); show('cumprod', cumprod(A)); show('cumsum rev', cumsum(A,'reverse'));
show('cummax', cummax(A)); show('cummin', cummin(A,2)); [m,i] = cummax([1 3 2 5 4]); show('cummax i', i);
show('movmean', movmean([1 2 3 4 5 6], 3)); show('movmean 4', movmean([1 2 3 4 5 6], 4));
show('movmean [1 2]', movmean([1 2 3 4 5 6], [1 2])); show('movsum', movsum(A, 3)); show('movmean e', movmean([1 2 3 4 5], 3, 'Endpoints','discard'));
show('movmean omit', movmean([1 NaN 3 4 5], 3, 'omitnan'));
show('rescale', rescale([1 2 3 5], -1, 1)); show('rescale A', rescale(A));
show('trapz', trapz([1 2 4 8])); show('trapz xy', trapz([0 1 3 6],[1 2 4 8])); show('trapz dim', trapz(A, 2)); show('trapz xdim', trapz([1 2 4 7], A, 1));
show('cumtrapz', cumtrapz([1 2 4 8])); show('cumtrapz A', cumtrapz(A,2)); show('cumtrapz xy', cumtrapz([0 1 3 6], A, 1));
show('diff', diff([1 4 9 16 25], 2)); show('diff A', diff(A, 1, 2)); show('diff A3', diff(A,3,1));
show('gradient', gradient([1 4 9 16 25])); show('gradient x', gradient([1 4 9 16 25], [0 1 3 4 7]));
[gx,gy] = gradient(A); show('gx', gx); show('gy', gy);
show('kron', kron([1 2; 3 4], [1 1; 0 1])); show('conv', conv([1 2 3],[1 1])); show('conv same', conv([1 2 3 4],[1 1 1],'same'));
[q,r] = deconv([1 5 6 3],[1 2]); show('deconv q', q); show('deconv r', r);
show('roots', sort(roots([1 -6 11 -6]))'); show('poly', poly([1 2 3]));
