1;
function show(name, v)
  printf('%s: size=%s |', name, mat2str(size(v)));
  printf(' %.9g', v(:)); printf('\n');
end
x = [1 2 4 7 8]; y = [3 1 4 1 5];
q = [0 1 1.5 3 4.5 7.9 8 9];
for m = {'linear','nearest','pchip','cubic','spline'}
  if any(strcmp(m{1},{'linear','nearest'})), show(['i1 ' m{1}], interp1(x,y,q,m{1})); end
  show(['i1x ' m{1}], interp1(x,y,q,m{1},'extrap'));
  show(['i1f ' m{1}], interp1(x,y,q,m{1},-7));
end
% next/previous: in-range queries only (extrapolation differs between MATLAB and Octave)
show('i1 next', interp1(x,y,q(2:7),'next')); show('i1 previous', interp1(x,y,q(2:7),'previous'));
show('i1 default', interp1(x,y,q));
show('i1 implicit', interp1(y,[1.5 2.5 6]));
Y = [y; y.^2]';
show('i1 mat', interp1(x,Y,[1.5 3 5]));
show('i1 mat sp', interp1(x,Y,[1.5 3 5],'spline'));
show('i1 qmat', interp1(x,y,[1.5 3; 5 6]));
show('i1 unsorted', interp1([4 1 2],[10 20 30],[1.5 3]));
show('i1 nan', interp1(x,[3 NaN 4 1 5],[1 1.5 2 3 4.5]));
pp = spline(x,y); show('spl coefs', pp.coefs); show('spl breaks', pp.breaks);
show('spl q', spline(x,y,q));
show('spl clamped', spline(x,[0 y 0],q));
show('spl 2pt', spline([0 1],[0 2],[0.5 2]));
show('spl 3pt', spline([0 1 3],[0 2 1],[0.5 2 4]));
show('spl mat', spline(x,[y;y.^2],[1.5 3]));
pp = pchip(x,y); show('pch coefs', pp.coefs);
show('pch q', pchip(x,y,q));
show('pch mono', pchip(1:6,[0 0 1 1 2 5],1:0.5:6));
show('ppval mat', ppval(spline(x,[y;y.^2]), [1.5 3; 4 5]));
[b,c,l,k,d] = unmkpp(spline(x,y)); show('unmk', [l k d]);
pp = mkpp([0 1 2],[1 0 0; 2 1 0]); show('mkpp', ppval(pp,[0.5 1.5 3]));
% interp2
[X,Y2] = meshgrid(1:4,1:3); Z = X.^2 + 3*Y2;
show('i2 lin', interp2(X,Y2,Z,[1.5 2.2 3.9],[1.2 2.5 1]));
show('i2 vec', interp2(1:4,1:3,Z,[1.5 2.2],[1.2;2.5]));
show('i2 near', interp2(X,Y2,Z,[1.4 2.6],[1.4 2.6],'nearest'));
show('i2 out', interp2(Z,[0.5 2],[1 2]));
show('i2 extrapv', interp2(X,Y2,Z,[0 2],[1 2],'linear',99));
% polyfit
xx = 0:6; yy = [1 3 2 5 4 7 8];
[p,S,mu] = polyfit(xx,yy,2); show('pf p', p); show('pf df', S.df); show('pf normr', S.normr); show('pf mu', mu(:)');
p = polyfit(xx,yy,1); show('pf1', p);
[p,S] = polyfit(xx,yy,3); [yf,dl] = polyval(p,[1.5 4],S); show('pv', yf);
% Octave's own S.R is column-permuted, so the delta oracle is a direct QR computation
V = vander(xx(:),4); [~,R] = qr(V,0); nr = norm(yy(:)-V*p(:)); d = zeros(1,2);
for k = 1:2, v = [1.5 4](k).^(3:-1:0); d(k) = sqrt(1+sum((v/R).^2))*nr/sqrt(3); end
show('pv delta ref', d);
if exist('OCTAVE_VERSION','builtin') == 0, assert(max(abs(dl-d)) < 1e-10); end
[p,S,mu] = polyfit(xx,yy,2); [yf,dl] = polyval(p,[1.5 4],S,mu); show('pv mu', yf);
% windows
for w = {'hamming','blackman','bartlett'}
  f = str2func(w{1});
  show([w{1} ' 7'], f(7)); show([w{1} ' 8'], f(8)); if ~strcmp(w{1},'bartlett'), show([w{1} ' p6'], f(6,'periodic')); end; show([w{1} ' 1'], f(1));
end
