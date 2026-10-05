%% Sönümlü salınım — ode45
clear; close all;
omega = 2*pi;
zeta = 0.12;
f = @(t,x) [x(2); -2*zeta*omega*x(2)-omega^2*x(1)];
[t, x] = ode45(f, [0 6], [1; 0]);
figure('Name','Sönümlü salınım');
subplot(2,1,1); plot(t,x(:,1),'LineWidth',1.5); grid on;
xlabel('Zaman (s)'); ylabel('Konum'); title('İkinci dereceden sistem');
subplot(2,1,2); plot(x(:,1),x(:,2),'LineWidth',1.5); grid on;
xlabel('Konum'); ylabel('Hız'); title('Faz portresi');
