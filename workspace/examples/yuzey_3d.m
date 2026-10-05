%% Üç boyutlu yüzey
clear; close all;
[x, y] = meshgrid(linspace(-3, 3, 80));
z = peaks(x, y);
figure('Name', 'Peaks · 3D yüzey', 'Position', [0 0 900 600]);
surf(x,y,z); shading interp; colormap turbo; colorbar;
xlabel('x'); ylabel('y'); zlabel('z'); title('Peaks fonksiyonu');
view(-35, 30); grid on;
