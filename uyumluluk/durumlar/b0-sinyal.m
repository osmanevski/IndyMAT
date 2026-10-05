% Sinyal işleme
%% sinyal-butter | butter
[b, a] = butter(2, 0.5);
assert(norm(b - [0.292893218813452 0.585786437626905 0.292893218813452]) < 1e-10);
assert(norm(a - [1 0 0.171572875253810]) < 1e-10);
[b, a] = butter(2, [0.2 0.4], 'bandpass');
assert(numel(b) == 5 && numel(a) == 5);
[z, p, k] = butter(2, 0.5, 'high');
assert(numel(p) == 2);
%% sinyal-diger-iir | cheby1 cheby2 ellip
[b, a] = cheby1(2, 1, 0.5);
assert(numel(b) == 3 && abs(sum(b) / sum(a) - 10^(-1 / 20)) < 1e-10);
[b, a] = cheby2(2, 40, 0.5);
assert(numel(b) == 3);
[b, a] = ellip(2, 1, 40, 0.5);
assert(numel(b) == 3);
%% sinyal-fir | fir1 hamming hann
b = fir1(10, 0.4);
assert(numel(b) == 11 && abs(sum(b) - 1) < 1e-10 && norm(b - fliplr(b)) < 1e-12);
assert(norm(hamming(5)' - [0.08 0.54 1 0.54 0.08]) < 1e-12);
assert(norm(hann(5)' - [0 0.5 1 0.5 0]) < 1e-12);
%% sinyal-filtfilt | filtfilt
[b, a] = butter(2, 0.2);
x = sin(2 * pi * 0.02 * (0:499));
y = filtfilt(b, a, x);
[~, i1] = max(x(100:150)); [~, i2] = max(y(100:150));
assert(i1 == i2 && numel(y) == numel(x));
%% sinyal-freqz | freqz
[h, w] = freqz(1, [1 -0.5], 8);
assert(abs(h(1) - 2) < 1e-12 && numel(w) == 8 && w(1) == 0);
%% sinyal-korelasyon | xcorr
[r, gecikme] = xcorr([1 2 3]);
assert(norm(r - [3 8 14 8 3]) < 1e-10 && isequal(gecikme, -2:2));
%% sinyal-tepe-bulma | findpeaks
[tepe, yer] = findpeaks([0 1 0 2 0]);
assert(isequal(tepe, [1 2]) && isequal(yer, [2 4]));
[tepe, yer] = findpeaks([0 1 0 3 0 2 0], 'MinPeakHeight', 1.5);
assert(isequal(tepe, [3 2]) && isequal(yer, [4 6]));
%% sinyal-ornekleme | resample decimate upsample downsample
assert(numel(resample(ones(1, 40), 2, 1)) == 80);
assert(numel(decimate(ones(1, 40), 2)) == 20);
assert(isequal(upsample([1 2], 2), [1 0 2 0]));
assert(isequal(downsample(1:6, 2), [1 3 5]));
%% sinyal-pwelch-varsayilan | pwelch | varsayılan pencere: 8 parça, %50 örtüşme
fs = 1000; t = (0:999) / fs;
x = sin(2 * pi * 100 * t);
[P, f] = pwelch(x, [], [], [], fs);
[~, i] = max(P);
assert(abs(f(i) - 100) < 5);
%% sinyal-pwelch-ornek-ortusme | pwelch | örtüşme örnek sayısı olarak verilir
fs = 1000; t = (0:999) / fs;
x = sin(2 * pi * 100 * t);
[P, f] = pwelch(x, 256, 128, 256, fs);
[~, i] = max(P);
assert(numel(f) == 129 && abs(f(i) - 100) < 4);
%% sinyal-periodogram | periodogram
fs = 1000; t = (0:999) / fs;
x = sin(2 * pi * 100 * t);
[P, f] = periodogram(x, [], [], fs);
[~, i] = max(P);
assert(abs(f(i) - 100) < 2);
%% sinyal-spektrogram | spectrogram
x = sin(2 * pi * 0.1 * (0:999));
[S, f, t] = spectrogram(x, 128, 64, 128, 1);
assert(size(S, 1) == 65 && size(S, 2) == numel(t));
%% sinyal-hilbert | hilbert
x = cos(2 * pi * (0:63) / 16);
assert(norm(imag(hilbert(x)) - sin(2 * pi * (0:63) / 16)) < 1e-10);
%% sinyal-medyan-filtre | medfilt1
assert(isequal(medfilt1([1 9 2 8 3], 3), [1 2 8 3 3]));
%% sinyal-donusumler | tf2zp zp2tf
[z, p, k] = tf2zp([1 0], [1 -0.5]);
assert(abs(p - 0.5) < 1e-12 && k == 1);
[b, a] = zp2tf([], [-1; -2], 1);
assert(norm(a - [1 3 2]) < 1e-12);
%% sinyal-dalga-ureteci | square sawtooth chirp sinc
assert(isequal(square([0 pi / 2 3 * pi / 2]), [1 1 -1]));
assert(abs(sawtooth(pi) - 0) < 1e-12);
assert(abs(sinc(0) - 1) < 1e-12 && abs(sinc(1)) < 1e-12);
assert(numel(chirp(0:0.01:1, 0, 1, 10)) == 101);
%% sinyal-rms | rms
assert(abs(rms([3 4]) - sqrt(12.5)) < 1e-12);
%% sinyal-bandpower | bandpower
assert(abs(bandpower([1 -1 1 -1]) - 1) < 1e-12);
%% sinyal-designfilt | designfilt
d = designfilt('lowpassfir', 'FilterOrder', 10, 'CutoffFrequency', 0.4);
assert(~isempty(d));
%% sinyal-hazir-filtreler | lowpass
y = lowpass(randn(1, 200), 0.2);
assert(numel(y) == 200);
%% sinyal-zarf | envelope
[ust, alt] = envelope(sin(2 * pi * (0:199) / 20));
assert(numel(ust) == 200);
%% sinyal-db | db2mag mag2db pow2db db2pow
assert(abs(db2mag(20) - 10) < 1e-12 && abs(mag2db(10) - 20) < 1e-12);
assert(abs(pow2db(100) - 20) < 1e-12 && abs(db2pow(20) - 100) < 1e-12);
