%% PID kapalı çevrim — hesap ve referans kontrolleri
% Basamak -> Toplama (r-y) -> P / PI / filtreli PID -> Tesis -> geri besleme.
% Tesis: y'' + 2*y' + y = u. İlk durumlar sıfır, zaman saniye.
% Bunlar denklem betikleridir; bir görsel model veya model dosyası değildir.
if exist('OCTAVE_VERSION', 'builtin')
  try
    pkg load control;
  catch
    error('control paketi bulunamadı. Projedeki paket kurulum rehberini izleyin.');
  end
end
G = tf(1, [1 2 1]);
t = (0:0.01:40)';
kazanclar = [2 0 0; 2 1 0; 2 1 1];
adlar = {'P', 'PI', 'PID'};
Tf = 0.1; % Türev filtresi: Kd*s/(Tf*s+1); ideal türev kullanılmaz.
y = zeros(numel(t), 3);
secenek = odeset('RelTol', 1e-9, 'AbsTol', 1e-11);
for j = 1:3
  Kp = kazanclar(j,1); Ki = kazanclar(j,2); Kd = kazanclar(j,3);
  C = pid(Kp, Ki, Kd, Tf);
  T = feedback(C*G, 1); % Negatif birim geri besleme.
  y(:,j) = step(T, t);
  % Bağımsız durum denklemleri: [y; y'; integral(e); filtrelenmiş e].
  rhs = @(tt,x) [x(2); -2*x(2)-x(1)+Kp*(1-x(1))+Ki*x(3)+Kd/Tf*(1-x(1)-x(4)); ...
                 1-x(1); (1-x(1)-x(4))/Tf];
  [to, xo] = ode45(rhs, t, zeros(4,1), secenek);
  yr = xo(:,1);
  if max(abs(y(:,j)-yr)) > 2e-7
    error('PID yanıtı açık durum denklemlerinden ayrıldı.');
  end
  if Ki == 0
    son = Kp/(1+Kp);
  else
    son = 1;
  end
  % Yükselme: son değerin %%10 -> %%90 eşikleri, ilk örnek geçişleri.
  % Aşım son değere göre; kalıcı hata birim hedefe göre tanımlanır.
  metrik = zeros(2,3);
  for q = 1:2
    if q == 1, v = y(:,j); else, v = yr; end
    i10 = find(v >= 0.1*son, 1); i90 = find(v >= 0.9*son, 1);
    if isempty(i10) || isempty(i90), error('Yükselme eşikleri bulunamadı.'); end
    metrik(q,:) = [t(i90)-t(i10), max(0, (max(v)-son)/son)*100, 1-v(end)];
  end
  if any(abs(metrik(1,:)-metrik(2,:)) > [0.02 0.001 2e-7]) || ...
     abs(metrik(1,3)-(1-son)) > 0.001
    error('PID metrik referans kontrolü başarısız.');
  end
  fprintf('%s: yükselme=%.3f s, aşım=%.3f %%, kalıcı hata=%.6f\n', adlar{j}, metrik(1,:));
end
fprintf('REFERANS OK: PID kapalı çevrim\n');

%% Grafikler — sayısal test bu işaretten önce durur
figure('Name', 'P, PI ve PID kapalı çevrim');
plot(t, y, 'LineWidth', 1.3); grid on;
xlabel('Zaman (s)'); ylabel('Çıkış'); title('Birim basamak ve negatif geri besleme');
legend(adlar); xlim([0 15]);
