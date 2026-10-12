%% Ayrık kontrol — hesap ve referans kontrolleri
% Basamak -> Toplama (r-y[k]) -> Ayrık kazanç -> ZOH -> Tesis -> Örnekleyici.
% Örnekleyici çıkışı geri beslenir. Tek oran, sıfır ilk durum, gecikmesiz kazanç.
% Önce tesis ayrıklaştırılır, sonra çevrim kapatılır; tersi aynı deney değildir.
if exist('OCTAVE_VERSION', 'builtin')
  try
    pkg load control;
  catch
    error('control paketi bulunamadı. Projedeki paket kurulum rehberini izleyin.');
  end
end
G = tf(1, [1 1]); K = 2; Tsler = [0.05 0.5];
zaman = cell(1,2); cikis = cell(1,2);
for j = 1:2
  Ts = Tsler(j); t = (0:Ts:8)';
  Gd = c2d(G, Ts, 'zoh'); Cd = tf(K, 1, Ts);
  Td = feedback(Cd*Gd, 1);
  yd = lsim(Td, ones(size(t)), t);
  ys = step(Td, t);
  % ZOH altında tam tesis güncellemesi ve ayrık denetleyici:
  % x[k+1] = a*x[k] + (1-a)*K*(1-x[k]), a=exp(-Ts).
  a = exp(-Ts); kutup = a-K*(1-a);
  yr = zeros(size(t));
  for n = 1:numel(t)-1
    yr(n+1) = a*yr(n)+(1-a)*K*(1-yr(n));
  end
  y_formul = K/(1+K)*(1-kutup.^(0:numel(t)-1)');
  yc = K/(1+K)*(1-exp(-(1+K)*t));
  fark = max(abs(yd-yc)); ref_fark = max(abs(y_formul-yc));
  if abs(kutup) >= 1 || max(abs(yd-yr)) > 1e-10 || ...
     max(abs(ys-yr)) > 1e-10 || max(abs(yr-y_formul)) > 1e-10 || ...
     abs(fark-ref_fark) > 1e-10
    error('ZOH / fark denklemi referans kontrolü başarısız.');
  end
  fprintf('Ts=%.2f s: sürekli çevrimden azami fark=%.6f\n', Ts, fark);
  zaman{j} = t; cikis{j} = yd;
end
fprintf('REFERANS OK: ayrık kontrol\n');

%% Grafikler — sayısal test bu işaretten önce durur
figure('Name', 'Örnekleme periyodunun etkisi');
plot(zaman{1}, 2/3*(1-exp(-3*zaman{1})), 'k--'); hold on;
stairs(zaman{1}, cikis{1}); stairs(zaman{2}, cikis{2}); grid on;
xlabel('Zaman (s)'); ylabel('Çıkış'); title('Ayrık kazanç ve ZOH ile birim basamak');
legend('Sürekli çevrim', 'Ts = 0.05 s', 'Ts = 0.5 s');
