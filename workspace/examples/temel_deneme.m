clear; clc; close all;
A = [2 1; 1 3];
b = [5; 7];
x = A \ b;
disp('Denklem sistemi cozum sonucu:');
disp(x);
assert(norm(A*x-b) < 1e-10);
toplam = 0;
for k = 1:10
    toplam = toplam + k;
end
assert(toplam == 55);
assert(isequal(kare_al([1 2 3]), [1 4 9]));
t = linspace(0, 2*pi, 500);
figure('Name', 'Octave ile temel hesaplama', 'Color', 'w');
subplot(1, 2, 1);
plot(t, sin(t), 'b', t, cos(t), 'r', 'LineWidth', 1.5);
grid on; xlabel('t'); ylabel('Genlik');
legend('sin(t)', 'cos(t)'); title('2D grafik');
subplot(1, 2, 2);
[u, v] = meshgrid(-2:0.1:2);
surf(u, v, exp(-(u.^2 + v.^2)));
xlabel('x'); ylabel('y'); zlabel('z'); title('3D grafik');
disp('Matris, dongu ve ayri dosyada fonksiyon: BASARILI');
