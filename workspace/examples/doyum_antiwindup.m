%% PI doyumu ve anti-windup — hesap ve referans kontrolleri
% Basamak -> Toplama (r-y) -> PI -> Doyum [-1,1] -> 1/(s+1) -> geri besleme.
% PI integratörü isteğe bağlı sıkıştırma (clamping) ile korunur:
% doyumdayken hata kontrolü daha da dışarı itiyorsa integral güncellenmez.
% r=2 (0<=t<5), sonra r=0.5; sıfır ilk durum, zaman saniye.
% Sabit adım: tesis ZOH ile tam, integral ileri Euler ile güncellenir.
Kp = 2; Ki = 1; limit = 1;
hler = [0.002 0.001]; sonuclar = cell(1,2); zamanlar = cell(1,2);
for q = 1:2
  h = hler(q); t = (0:h:12)';
  r = 2*ones(size(t)); r(t >= 5) = 0.5;
  y = zeros(numel(t),2); z = zeros(1,2); u = zeros(numel(t),2);
  a = exp(-h);
  for n = 1:numel(t)-1
    e = r(n)-y(n,:); ham = Kp*e+Ki*z;
    u(n,:) = min(max(ham, -limit), limit);
    izin = [true, ~((ham(2)>limit && e(2)>0) || (ham(2)<-limit && e(2)<0))];
    % Eski integralden kontrol hesaplanır; sonra iki durum da güncellenir.
    y(n+1,:) = a*y(n,:)+(1-a)*u(n,:);
    z = z+h*e.*izin;
  end
  u(end,:) = min(max(Kp*(r(end)-y(end,:))+Ki*z, -limit), limit);
  ilk = t <= 5;
  if max(max(abs(y(ilk,:)-(1-exp(-t(ilk)))*ones(1,2)))) > 1e-10 || ...
     max(abs(u(:))) > limit+1e-12
    error('Doyumun ilk evresi analitik referanstan ayrıldı.');
  end
  sonuclar{q} = y; zamanlar{q} = t;
  if q == 1, u_grafik = u; r_grafik = r; end
end
% Bağımsız adım inceltme kontrolü: h/2 çözümü ortak zamanlarda karşılaştırılır.
t = zamanlar{1}; y = sonuclar{1}; y_ince = sonuclar{2}(1:2:end,:);
fark = max(max(abs(y-y_ince)));
sonra = t >= 5;
alan = trapz(t(sonra), abs(y(sonra,:)-0.5));
alan_ref = trapz(t(sonra), abs(y_ince(sonra,:)-0.5));
if fark > 0.005 || any(abs(alan-alan_ref) > 0.01) || alan(2) >= alan(1)
  error('Anti-windup adım inceltme veya hata alanı kontrolü başarısız.');
end
fprintf('Adım inceltme azami fark=%.6f\n', fark);
fprintf('5 s sonrası mutlak hata alanı: korumasız=%.6f, clamping=%.6f\n', alan);
fprintf('REFERANS OK: doyum ve anti-windup\n');

%% Grafikler — sayısal test bu işaretten önce durur
figure('Name', 'PI doyumu ve integral yığılması');
subplot(2,1,1); plot(t, r_grafik, 'k--', t, y, 'LineWidth', 1.2); grid on;
ylabel('Çıkış'); title('Hedef düşürülünce integral yığılmasının etkisi');
legend('Hedef', 'Korumasız PI', 'Clamping PI');
subplot(2,1,2); plot(t, u_grafik, 'LineWidth', 1.2); grid on;
xlabel('Zaman (s)'); ylabel('Aktüatör'); legend('Korumasız PI', 'Clamping PI');
