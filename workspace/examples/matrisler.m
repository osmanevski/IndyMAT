%% Doğrusal cebir laboratuvarı
clear; close all;
A = [4 1 2; 1 3 0; 2 0 5];
b = [7; 8; 9];
x = A \ b;
[V, D] = eig(A);
[U, S, W] = svd(A);
hata = norm(A*x-b);
assert(hata < 1e-10);
fprintf('Çözüm hatası: %.2e\n', hata);
disp('x ='); disp(x);
figure('Name', 'Matrisin yapısı');
subplot(1,2,1); imagesc(A); axis equal tight; colorbar; title('A matrisi');
subplot(1,2,2); bar(diag(D)); title('Özdeğerler'); grid on;
