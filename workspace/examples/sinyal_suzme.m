%% Sinyal süzme — hesap ve referans kontrolleri
% Sinüs + yüksek frekanslı sinüs + Gürültü -> Toplama -> FIR / IIR -> Gözlem / FFT.
% Sonlu kayıt, sıfır filtre ilk durumu; nedensel filter gecikmesi korunur.
if exist('OCTAVE_VERSION', 'builtin')
  try
    pkg load signal;
  catch
    error('signal paketi bulunamadı. Projedeki paket kurulum rehberini izleyin.');
  end
end
rng(25, 'twister'); % Gürültü her yeni çalıştırmada tekrarlanır.
Fs = 500; N = 1000; t = (0:N-1)'/Fs;
x = sin(2*pi*10*t)+0.6*sin(2*pi*120*t)+0.2*randn(N,1);
b_fir = fir1(40, 50/(Fs/2)); a_fir = 1;
[b_iir, a_iir] = butter(4, 50/(Fs/2));
katsayilar = {b_fir, a_fir; b_iir, a_iir};
adlar = {'FIR', 'IIR'}; y = zeros(N,2);
for j = 1:2
  b = katsayilar{j,1}; a = katsayilar{j,2};
  y(:,j) = filter(b, a, x);
  % Bağımsız doğrudan fark denklemi; giriş ve çıkış geçmişi başlangıçta sıfır.
  yr = zeros(N,1);
  for n = 1:N
    toplam = 0;
    for k = 1:min(numel(b),n), toplam = toplam+b(k)*x(n-k+1); end
    for k = 2:min(numel(a),n), toplam = toplam-a(k)*yr(n-k+1); end
    yr(n) = toplam/a(1);
  end
  w = 2*pi*[10; 120]/Fs;
  H = freqz(b, a, w);
  H_ref = zeros(2,1);
  for k = 1:2
    H_ref(k) = sum(b.*exp(-1i*w(k)*(0:numel(b)-1))) / ...
               sum(a.*exp(-1i*w(k)*(0:numel(a)-1)));
  end
  if max(abs(y(:,j)-yr)) > 1e-10 || max(abs(H-H_ref)) > 1e-10 || ...
     abs(H(1)) < 0.95 || abs(H(2)) > 0.1
    error('Filtre fark denklemi / frekans referans kontrolü başarısız.');
  end
  fprintf('%s kazancı: 10 Hz=%.6f, 120 Hz=%.6f\n', adlar{j}, abs(H));
end
% Tek taraflı genlik spektrumu; DC ve Nyquist ikiyle çarpılmaz.
X = fft([x y]); spektrum = abs(X(1:N/2+1,:))/N;
spektrum(2:end-1,:) = 2*spektrum(2:end-1,:);
f = (0:N/2)'*Fs/N;
if max(abs(sum(abs(X).^2)/N-sum([x y].^2))) > 1e-8
  error('FFT Parseval enerji kontrolü başarısız.');
end
fprintf('REFERANS OK: sinyal süzme\n');

%% Grafikler — sayısal test bu işaretten önce durur
figure('Name', 'FIR ve IIR ile sinyal süzme');
subplot(2,1,1); plot(t, [x y]); grid on; xlim([0 0.4]);
xlabel('Zaman (s)'); ylabel('Genlik'); title('Nedensel filtreler: gecikme ve başlangıç geçişi');
legend('Giriş', 'FIR', 'IIR');
subplot(2,1,2); plot(f, spektrum); grid on;
xlabel('Frekans (Hz)'); ylabel('Tek taraflı genlik'); title('Sonlu kaydın FFT spektrumu');
legend('Giriş', 'FIR', 'IIR');
