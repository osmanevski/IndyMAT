%% Kütle-yay-sönüm — hesap ve referans kontrolleri
% Kuvvet -> Toplama (F-c*v-k*x) -> 1/m -> İntegratör (v) -> İntegratör (x).
% x ve v, kazançlar üzerinden toplama noktasına geri beslenir.
% Serbest salınım: F=0, x(0)=1 m, v(0)=0 m/s; zaman saniye.
if exist('OCTAVE_VERSION', 'builtin')
  try
    pkg load control;
  catch
    error('control paketi bulunamadı. Projedeki paket kurulum rehberini izleyin.');
  end
end
m = 1; k = 4; c = 0.8;
A = [0 1; -k/m -c/m]; B = [0; 1/m];
sistem = ss(A, B, [1 0], 0);
t = (0:0.01:10)'; x0 = [1; 0];
y_ss = lsim(sistem, zeros(size(t)), t, x0);
rhs = @(tt,x) [x(2); (-c*x(2)-k*x(1))/m];
[to, x_ode] = ode45(rhs, t, x0, odeset('RelTol', 1e-10, 'AbsTol', 1e-12));
alfa = c/(2*m); wd = sqrt(k/m-alfa^2);
y_analitik = exp(-alfa*t).*(cos(wd*t)+alfa/wd*sin(wd*t));
fark = max(abs(y_ss-x_ode(:,1)));
if fark > 1e-8 || max(abs(y_ss-y_analitik)) > 1e-10 || ...
   max(abs(x_ode(:,1)-y_analitik)) > 1e-8
  error('Kütle-yay-sönüm analitik referans kontrolü başarısız.');
end
fprintf('ss / ode45 azami fark=%.3g m\n', fark);
fprintf('REFERANS OK: kütle-yay-sönüm\n');

%% Grafikler — sayısal test bu işaretten önce durur
figure('Name', 'Kütle-yay-sönüm ilk koşul yanıtı');
plot(t, y_ss, '-', to, x_ode(:,1), '--', 'LineWidth', 1.3); grid on;
xlabel('Zaman (s)'); ylabel('Konum (m)');
title('Serbest salınım: durum uzayı ve açık diferansiyel denklem');
legend('ss / lsim', 'ode45');
