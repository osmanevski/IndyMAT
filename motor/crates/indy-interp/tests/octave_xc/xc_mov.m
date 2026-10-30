1;
function show(name, v)
  printf('%s: size=%s |', name, mat2str(size(v)));
  printf(' %.9g', v(:)); printf('\n');
end
x = [3 1 4 1 5 9 2 6]; A = [1 5 3; 4 2 8; 7 7 1; 2 9 4];
show('pder', polyder([1 2 3 4])); show('pder1', polyder(5)); show('pder prod', polyder([1 2],[1 3]));
[q,d] = polyder([1 2],[1 3 1]); show('pder q', q); show('pder d', d);
show('pint', polyint([3 2 1])); show('pint k', polyint([3 2 1], 5));
show('fftshift', fftshift(1:6)); show('fftshift 5', fftshift(1:5)); show('ifftshift 5', ifftshift(1:5));
show('fftshift m', fftshift(magic(4))); show('fftshift d', fftshift(magic(4),1)); show('ifftshift m', ifftshift(magic(5)));
if exist('movmedian') 
 show('movmed', movmedian(x,3)); show('movmed4', movmedian(x,4)); show('movmax', movmax(x,[2 0])); show('movmin', movmin(x,3));
 show('movvar', movvar(x,3)); show('movstd', movstd(x,4)); show('movprod', movprod(x,3)); show('movmedA', movmedian(A,3)); show('movmedA2', movmedian(A,3,2));
 show('movmed disc', movmedian(x,3,'Endpoints','discard')); show('movmed fill', movmedian(x,3,'Endpoints','fill')); show('movmed val', movmedian(x,3,'Endpoints',0));
 show('movmed nan', movmedian([1 NaN 3 4 5],3,'omitnan')); show('movmax nan', movmax([1 NaN 3 4 5],3));
end
show('del2 v', del2([1 4 9 16 25 36])); show('del2 m', del2(magic(5))); show('del2 h', del2(magic(4).^2, 0.5)); show('del2 col', del2((1:5)'.^3));
show('del2 hxy', del2(magic(5), 2, 3));
