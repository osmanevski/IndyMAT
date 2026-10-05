% Sinyal işleme
%% sinyaly-spectrogram-tone | spectrogram | merkezli zamanlar ve tek taraflı frekans vektörü
x = cos(2 * pi * (0:63) / 8);
[s, f, t, ps] = spectrogram(x, 16, 8, 32, 1);
assert(size(s, 1) == 17 && size(s, 2) == 7 && isequal(size(ps), size(s)));
assert(isequal(f, (0:16)' / 32) && isequal(t, 8:8:56));
[~, i] = max(abs(s(:, 1)));
assert(f(i) == 1 / 8);
assert(norm(s(1:3, 1) - [0.1644316; -0.18505924-0.59350296i; -1.8250426+0.38547742i]) < 1e-6);
assert(norm(ps(1:3, 1) - [0.0045309096; 0.1295347; 1.1661271]) < 1e-6);
%% sinyaly-spectrogram-symmetric-window | spectrogram | tamsayı pencere simetrik Hamming'dir
x = ones(4, 1);
[s, f, t] = spectrogram(x, 4, 0, 4, 1);
hh = fft(hamming(4));
assert(norm(s(:, 1) - hh(1:3)) < 1e-12);
assert(isequal(f, [0; 0.25; 0.5]) && t == 2);
%% sinyaly-spectrogram-explicit-window | spectrogram | vektör pencere doğrudan kullanılır
x = [1 2 3 4];
[s, f, t] = spectrogram(x, [1 0 0 0], 0, 4, 2);
assert(isequal(s(:, 1), [1; 1; 1]) && isequal(f, [0; 0.5; 1]) && t == 1);
%% sinyaly-spectrogram-complex | spectrogram | karmaşık girişte varsayılan iki taraflı eksen sıfırdan başlar
x = exp(1i * 2 * pi * (0:31) / 8);
[s, f] = spectrogram(x, ones(8, 1), 0, 8, 1);
assert(size(s, 1) == 8 && isequal(f, (0:7)' / 8));
assert(abs(s(2, 1) - 8) < 1e-12);
[sc, fc] = spectrogram(x, ones(8, 1), 0, 8, 1, 'centered');
assert(isequal(fc, (-3:4)' / 8) && abs(sc(5, 1) - 8) < 1e-12);
%% sinyaly-spectrogram-complex-odd | spectrogram | tek FFT uzunluğunda merkezli seçenek
x = exp(1i * 2 * pi * (0:14) / 5);
[s, f] = spectrogram(x, ones(5, 1), 0, 5, 1);
assert(size(s, 1) == 5 && max(abs(f - (0:4)' / 5)) < 1e-12);
[~, fc] = spectrogram(x, ones(5, 1), 0, 5, 1, 'centered');
assert(max(abs(fc - (-2:2)' / 5)) < 1e-12);
%% sinyaly-spectrogram-psd | spectrogram | PSD ölçeği pencere enerjisine göre normalize edilir
x = ones(8, 1); w = ones(4, 1);
[~, f, ~, ps] = spectrogram(x, w, 0, 4, 2);
assert(isequal(f, [0; 0.5; 1]) && norm(ps(:, 1) - [2; 0; 0]) < 1e-12);
%% sinyaly-spectrogram-power | spectrogram | power ölçeği pencere toplamına göre normalize edilir
[~, ~, ~, ps] = spectrogram(ones(4, 1), ones(4, 1), 0, 4, 1, 'power');
assert(norm(ps(:, 1) - [1; 0; 0]) < 1e-12);
%% sinyaly-bandpower-scalar | bandpower | güç sabit sinyal için genlik karesidir
assert(abs(bandpower(ones(16, 1)) - 1) < 1e-12);
%% sinyaly-bandpower-matrix | bandpower | matris sütunları ayrı hesaplanır
x = [ones(32, 1), 2 * ones(32, 1)];
p = bandpower(x);
assert(isequal(size(p), [1 2]) && norm(p - [1 4]) < 1e-12);
%% sinyaly-bandpower-range | bandpower | frekans aralığı verilen sinüs bileşenini yakalar
fs = 64; n = 0:255; x = cos(2 * pi * 8 * n / fs);
p = bandpower(x, fs, [7 9]);
assert(abs(p - 0.5) < 0.02);
%% sinyaly-bandpower-psd | bandpower | verilen PSD dikdörtgen bin alanıyla toplanır
f = (0:4)'; pxx = ones(5, 1);
assert(bandpower(pxx, f, 'psd') == 5);
%% sinyaly-bandpower-psd-range | bandpower | aralık uçları örneklenmiş binleri içerir
f = (0:4)'; pxx = [1; 2; 3; 4; 5];
assert(bandpower(pxx, f, [1 3], 'psd') == 9);
%% sinyaly-envelope-analytic | envelope | saf tonun analitik zarfı sabittir
x = sin(2 * pi * (0:63) / 8);
[upper, lower] = envelope(x);
assert(isequal(size(upper), size(x)) && isequal(size(lower), size(x)));
assert(norm(upper - 1) < 1e-10 && norm(lower + 1) < 1e-10);
%% sinyaly-envelope-offset | envelope | ortalama zarfın merkezine eklenir
x = 3 + cos(2 * pi * (0:63) / 8);
[upper, lower] = envelope(x);
assert(norm(upper - 4) < 1e-10 && norm(lower - 2) < 1e-10);
%% sinyaly-spectrogram-overlap-columns | spectrogram | örtüşme segment sayısını belirler
x = 1:20; [s, f, t] = spectrogram(x, ones(5, 1), 2, 8, 4);
assert(size(s, 2) == 6 && norm(t - (0.625:0.75:4.375)) < 1e-12 && numel(f) == 5);
%% sinyaly-spectrogram-row-column | spectrogram | vektör yönü çıktı boyutlarını değiştirmez
x = 1:16; [s1, f1, t1] = spectrogram(x, ones(4, 1), 2, 8, 1);
[s2, f2, t2] = spectrogram(x', ones(4, 1), 2, 8, 1);
assert(isequal(s1, s2) && isequal(f1, f2) && isequal(t1, t2));
assert(isequal(size(t1), [1 7]) && isequal(size(f1), [5 1]));
%% sinyaly-spectrogram-default | spectrogram | varsayılan pencere, FFT ve rad tabanlı eksenler
x = cos(2 * pi * (0:999) / 10);
[s, f, t] = spectrogram(x);
assert(isequal(size(s), [129 8]) && abs(f(2) - 0.024543693) < 1e-6 && abs(f(end) - pi) < 1e-6);
assert(max(abs(t - (111:111:888) / (2*pi))) < 1e-12);
assert(abs(t(1) - 17.666199) < 1e-6);
[s100, ~, t100] = spectrogram(x, 100);
assert(isequal(size(s100), [129 19]) && max(abs(t100(1:3) - [7.9577472 15.915494 23.873241])) < 1e-6);
[s10, f10, t10] = spectrogram(x, [], [], [], 10);
assert(isequal(size(s10), [129 8]) && abs(f10(2) - 0.0390625) < 1e-12 && abs(f10(end) - 5) < 1e-12);
assert(norm(t10(1:2) - [11.1 22.2]) < 1e-12);
%% sinyaly-bandpower-matrix-columns | bandpower | PSD matris sütunları ayrı tümleştirilir
f = (0:4)'; pxx = [ones(5, 1), 2 * ones(5, 1)];
p = bandpower(pxx, f, 'psd');
assert(isequal(size(p), [1 2]) && isequal(p, [5 10]));
%% sinyaly-bandpower-row-psd | bandpower | satır PSD ile satır frekansı kabul edilir
f = 0:4; pxx = [1 2 3 4 5];
assert(bandpower(pxx, f, 'psd') == 15);
%% sinyaly-envelope-column | envelope | sütun giriş sütun çıktısı verir
x = cos(2 * pi * (0:31)' / 8); [upper, lower] = envelope(x);
assert(isequal(size(upper), size(x)) && norm(upper - 1) < 1e-10 && norm(lower + 1) < 1e-10);
%% sinyaly-goertzel-bin | goertzel | MATLAB bir tabanlı kutular FFT değerleriyle eşleşir
x = (1:16)'; k = [1 4 9]; y = goertzel(x, k);
xf = fft(x); assert(norm(y(:) - xf(k(:))) < 1e-6);
assert(isequal(size(y), [3 1]) && norm(y(:).' - [136 -8+11.972846i -8]) < 1e-6);
assert(isequal(size(goertzel(x', k)), [1 3]));
%% sinyaly-goertzel-all | goertzel | tüm kutularla DFT elde edilir
x = [1 2 3 4]; y = goertzel(x);
assert(isequal(size(y), [1 4]) && norm(y - fft(x)) < 1e-12);
%% sinyaly-tf2zpk | tf2zpk | kazanç ve kökler polinomlardan çıkarılır
[z, p, k] = tf2zpk([2 -2], [2 -1]);
assert(abs(z - 1) < 1e-12 && abs(p - 0.5) < 1e-12 && k == 1);
%% sinyaly-tf2zpk-constant | tf2zpk | sabit katsayıların kök kümeleri boştur
[z, p, k] = tf2zpk(3, 2);
assert(isempty(z) && isempty(p) && k == 1.5);
%% sinyaly-firtype | firtype | simetri ve uzunluk türü dört tipi belirler
assert(firtype([1 2 1]) == 1 && firtype([1 2 2 1]) == 2);
assert(firtype([1 0 -1]) == 3 && firtype([1 2 -2 -1]) == 4);
%% sinyaly-islinphase | islinphase | simetrik ve antisymetrik FIR doğrusaldır
assert(islinphase([1 2 1]) && islinphase([1 0 -1]) && ~islinphase([1 2 3]));
%% sinyaly-phasedelay-zero | phasedelay | tanımsız sıfır frekansında NaN döner
[tau, w] = phasedelay(1, 1, 16);
assert(numel(tau) == 16 && isequal(size(tau), size(w)) && isnan(tau(1)) && all(tau(2:end) == 0));
%% sinyaly-phasedelay-first-order | phasedelay | ölçülen ilk mertebe gecikmesi
[tau, w] = phasedelay([1 2 1], 1, 8);
assert(isnan(tau(1)) && max(abs(tau(2:end) - 1)) < 1e-6 && max(abs(w(:)' - (0:7)*pi/8)) < 1e-6);
%% sinyaly-zerophase-symmetric | zerophase | simetrik FIR cevabında doğrusal faz kaldırılır
[h, w, phi] = zerophase([1 2 1], 1, 16);
assert(numel(h) == 16 && isequal(size(h), size(w)) && norm(h - (2 + 2 * cos(w))) < 1e-10);
assert(norm(phi + w) < 1e-12);
[hshort, ~] = zerophase([1 2 1], 16);
assert(numel(hshort) == 512 && abs(hshort(1) - 0.25) < 1e-12);
%% sinyaly-rc-poly-roundtrip | rc2poly poly2rc | yansıma katsayıları ileri geri dönüşür
k = [0.25 -0.4 0.1]; [a, efinal] = rc2poly(k, 2); [recovered, r0] = poly2rc(a, 1.5);
assert(norm(recovered - k(:)) < 1e-12 && abs(a(1) - 1) < 1e-12);
assert(abs(efinal - 1.55925) < 1e-6 && abs(r0 - 1.9240019) < 1e-6);
%% sinyaly-rlevinson | rlevinson | birinci dereceli Yule-Walker çözümü
[r, U, k] = rlevinson([1 -0.5], 0.75);
assert(norm(r - [1; 0.5]) < 1e-12 && isequal(U, [1 -0.5; 0 1]) && abs(k + 0.5) < 1e-12);
assert(norm(poly2ac([1 -0.5], 0.75) - [1; 0.5]) < 1e-12);
%% sinyaly-envelope-rms | envelope | pencere uçlarında mevcut örneklerle RMS
x = [0 1 0 -1 0 1 0 -1 0 1 0 -1];
[upper, lower] = envelope(x, 3, 'rms');
assert(max(abs(upper - [0.70710678 0.57735027 0.81649658 0.57735027 0.81649658 0.57735027 0.81649658 0.57735027 0.81649658 0.57735027 0.81649658 0.70710678])) < 1e-6);
assert(max(abs(lower - [-0.70710678 -0.57735027 -0.81649658 -0.57735027 -0.81649658 -0.57735027 -0.81649658 -0.57735027 -0.81649658 -0.57735027 -0.81649658 -0.70710678])) < 1e-6);
%% sinyaly-bandpower-measured-tone | bandpower | ölçülen PSD ve aralık güçleri
x = cos(2 * pi * (0:255)' / 8);
assert(abs(bandpower(x) - 0.5) < 1e-6 && abs(bandpower(x, 1, [0.1 0.15]) - 0.50000005) < 1e-6);
%% sinyaly-seqperiod | seqperiod | en kısa tekrar dönemi bulunur
assert(seqperiod([1 2 1 2 1 2]) == 2 && seqperiod([1 2 3 4]) == 4);
