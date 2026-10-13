% İstatistik: dalga 26 gerçek MATLAB norminv tablosu
%% wave26-norminv-fourth-input | norminv | R2025b round2: tek çıktıda dördüncü char veya sayısal girdinin etkisizliği
p=[0 .1 .5 .9 1]; baseline=norminv(p,0,1);
assert(baseline(1)==-Inf && baseline(5)==Inf);
assert(max(abs(baseline(2:4)-[-1.28155 0 1.28155]))<5e-6);
for fourth={'upper','lower','bogus'}, assert(isequal(norminv(p,0,1,fourth{1}),baseline)); end
assert(isequal(norminv([.1 .5 .9],0,1,zeros(2,3)),baseline(2:4)));
